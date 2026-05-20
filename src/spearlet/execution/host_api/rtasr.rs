use crate::spearlet::execution::ai::ir::{Operation, Payload, RoutingHints, SpeechToTextPayload};
use crate::spearlet::execution::ai::streaming::{StreamingPlan, StreamingWebsocketPlan};
use crate::spearlet::execution::host_api::errno::{
    SPEAR_EAGAIN, SPEAR_EBADF, SPEAR_EINVAL, SPEAR_EIO,
};
use crate::spearlet::execution::host_api::DefaultHostApi;
use crate::spearlet::execution::hostcall::types::{
    FdEntry, FdFlags, FdInner, FdKind, PollEvents, RtAsrConnState, RtAsrSendItem,
};
use serde_json::json;
use std::collections::HashMap;
use std::collections::HashSet;

use crate::spearlet::param_keys::{chat as chat_keys, rtasr as rtasr_keys};

mod readiness;
mod segmentation;
mod stub;
mod websocket;

impl DefaultHostApi {
    pub fn rtasr_create(&self) -> i32 {
        self.fd_table.alloc(FdEntry {
            kind: FdKind::RtAsr,
            flags: FdFlags::default(),
            poll_mask: PollEvents::OUT,
            watchers: HashSet::new(),
            closed: false,
            inner: FdInner::RtAsr(Box::default()),
        })
    }

    pub fn rtasr_ctl(
        &self,
        fd: i32,
        cmd: i32,
        payload: Option<&[u8]>,
    ) -> Result<Option<Vec<u8>>, i32> {
        const RTASR_CTL_SET_PARAM: i32 = 1;
        const RTASR_CTL_CONNECT: i32 = 2;
        const RTASR_CTL_GET_STATUS: i32 = 3;
        const RTASR_CTL_SEND_EVENT: i32 = 4;
        const RTASR_CTL_FLUSH: i32 = 5;
        const RTASR_CTL_CLEAR: i32 = 6;
        const RTASR_CTL_SET_AUTOFLUSH: i32 = 7;
        const RTASR_CTL_GET_AUTOFLUSH: i32 = 8;

        let Some(entry) = self.fd_table.get(fd) else {
            return Err(-SPEAR_EBADF);
        };

        match cmd {
            RTASR_CTL_SET_PARAM => {
                let bytes = payload.ok_or(-SPEAR_EINVAL)?;
                let v: serde_json::Value =
                    serde_json::from_slice(bytes).map_err(|_| -SPEAR_EINVAL)?;
                let key = v.get("key").and_then(|x| x.as_str()).unwrap_or("");
                let value = v.get("value").cloned().unwrap_or(serde_json::Value::Null);
                if key.is_empty() {
                    return Err(-SPEAR_EINVAL);
                }

                let notify = {
                    let mut e = entry.lock().map_err(|_| -SPEAR_EIO)?;
                    if e.closed {
                        return Err(-SPEAR_EBADF);
                    }
                    let FdInner::RtAsr(st) = &mut e.inner else {
                        return Err(-SPEAR_EBADF);
                    };
                    st.params.insert(key.to_string(), value);

                    if st.state == RtAsrConnState::Init {
                        st.state = RtAsrConnState::Configured;
                    }

                    if key == rtasr_keys::MAX_SEND_QUEUE_BYTES {
                        if let Some(n) = st
                            .params
                            .get(rtasr_keys::MAX_SEND_QUEUE_BYTES)
                            .and_then(|x| x.as_u64())
                        {
                            st.max_send_queue_bytes = n as usize;
                        }
                    }
                    if key == rtasr_keys::MAX_RECV_QUEUE_BYTES {
                        if let Some(n) = st
                            .params
                            .get(rtasr_keys::MAX_RECV_QUEUE_BYTES)
                            .and_then(|x| x.as_u64())
                        {
                            st.max_recv_queue_bytes = n as usize;
                        }
                    }

                    let old = e.poll_mask;
                    self.recompute_rtasr_readiness_locked(&mut e);
                    e.poll_mask.bits() != old.bits()
                };
                if notify {
                    self.fd_table.notify_watchers(fd);
                }
                Ok(None)
            }
            RTASR_CTL_CONNECT => {
                let mut spawn_stub = false;
                let mut spawn_ws = false;
                let mut ws_plan: Option<StreamingWebsocketPlan> = None;
                let mut ws_url_override: Option<String> = None;
                let mut client_secret_override: Option<String> = None;
                let mut model_override: Option<String> = None;
                let notify = {
                    let mut e = entry.lock().map_err(|_| -SPEAR_EIO)?;
                    if e.closed {
                        return Err(-SPEAR_EBADF);
                    }
                    let FdInner::RtAsr(st) = &mut e.inner else {
                        return Err(-SPEAR_EBADF);
                    };

                    if st.state == RtAsrConnState::Closed {
                        return Err(-SPEAR_EBADF);
                    }
                    if st.state == RtAsrConnState::Error {
                        return Err(-SPEAR_EIO);
                    }

                    if !st.stub_connected {
                        st.stub_connected = true;
                        st.state = RtAsrConnState::Connected;
                        let transport = st
                            .params
                            .get(rtasr_keys::TRANSPORT)
                            .and_then(|x| x.as_str())
                            .unwrap_or("stub");

                        if let Some(s) = st.params.get(rtasr_keys::WS_URL).and_then(|x| x.as_str())
                        {
                            ws_url_override = Some(s.to_string());
                        }
                        if let Some(s) = st
                            .params
                            .get(rtasr_keys::CLIENT_SECRET)
                            .and_then(|x| x.as_str())
                        {
                            client_secret_override = Some(s.to_string());
                        }
                        if let Some(s) = st.params.get(rtasr_keys::MODEL).and_then(|x| x.as_str()) {
                            model_override = Some(s.to_string());
                        }

                        if transport == "websocket" {
                            let req =
                                crate::spearlet::execution::ai::ir::CanonicalRequestEnvelope {
                                    version: 1,
                                    request_id: "rtasr_connect".to_string(),
                                    operation: Operation::SpeechToText,
                                    meta: HashMap::new(),
                                    routing: RoutingHints {
                                        backend: st
                                            .params
                                            .get(chat_keys::BACKEND)
                                            .and_then(|x| x.as_str())
                                            .map(|s| s.to_string()),
                                        allowlist: vec![],
                                        denylist: vec![],
                                    },
                                    requirements:
                                        crate::spearlet::execution::ai::ir::Requirements {
                                            required_features: vec![],
                                            required_transports: vec!["websocket".to_string()],
                                        },
                                    timeout_ms: None,
                                    payload: Payload::SpeechToText(SpeechToTextPayload {
                                        model: model_override.clone(),
                                    }),
                                    extra: HashMap::new(),
                                };

                            if let Ok(inv) = self.ai_engine.invoke_streaming(&req) {
                                match inv.plan {
                                    StreamingPlan::Websocket(mut p) => {
                                        if p.websocket.supports_turn_detection {
                                            segmentation::apply_turn_detection_to_client_events(
                                                &mut p.websocket.client_events,
                                                &st.segmentation,
                                            );
                                        }
                                        ws_plan = Some(p);
                                        spawn_ws = true;
                                    }
                                }
                            } else {
                                spawn_stub = true;
                            }
                        } else {
                            spawn_stub = true;
                        }
                    }

                    let old = e.poll_mask;
                    self.recompute_rtasr_readiness_locked(&mut e);
                    e.poll_mask.bits() != old.bits()
                };
                if notify {
                    self.fd_table.notify_watchers(fd);
                }
                if spawn_ws {
                    let plan = ws_plan.ok_or(-SPEAR_EIO)?;
                    self.spawn_rtasr_websocket_tasks(
                        fd,
                        plan,
                        ws_url_override,
                        client_secret_override,
                    );
                } else if spawn_stub {
                    self.spawn_rtasr_stub_tasks(fd);
                }
                Ok(None)
            }
            RTASR_CTL_GET_STATUS => {
                let e = entry.lock().map_err(|_| -SPEAR_EIO)?;
                if e.closed {
                    return Err(-SPEAR_EBADF);
                }
                let FdInner::RtAsr(st) = &e.inner else {
                    return Err(-SPEAR_EBADF);
                };
                let state = match st.state {
                    RtAsrConnState::Init => "Init",
                    RtAsrConnState::Configured => "Configured",
                    RtAsrConnState::Connecting => "Connecting",
                    RtAsrConnState::Connected => "Connected",
                    RtAsrConnState::Draining => "Draining",
                    RtAsrConnState::Closed => "Closed",
                    RtAsrConnState::Error => "Error",
                };
                let body = json!({
                    "state": state,
                    "last_error": st.last_error,
                    "send_queue_bytes": st.send_queue_bytes,
                    "recv_queue_bytes": st.recv_queue_bytes,
                    "max_send_queue_bytes": st.max_send_queue_bytes,
                    "max_recv_queue_bytes": st.max_recv_queue_bytes,
                    "dropped_events": st.dropped_events,
                });
                let bytes = serde_json::to_vec(&body).map_err(|_| -SPEAR_EIO)?;
                Ok(Some(bytes))
            }
            RTASR_CTL_SEND_EVENT => {
                let bytes = payload.ok_or(-SPEAR_EINVAL)?;
                let v: serde_json::Value =
                    serde_json::from_slice(bytes).map_err(|_| -SPEAR_EINVAL)?;
                let txt = serde_json::to_string(&v).map_err(|_| -SPEAR_EIO)?;

                let mut e = entry.lock().map_err(|_| -SPEAR_EIO)?;
                if e.closed {
                    return Err(-SPEAR_EBADF);
                }
                let old = e.poll_mask;

                {
                    let FdInner::RtAsr(st) = &mut e.inner else {
                        return Err(-SPEAR_EBADF);
                    };
                    if st.state == RtAsrConnState::Closed {
                        return Err(-SPEAR_EBADF);
                    }
                    if st.state == RtAsrConnState::Error {
                        return Err(-SPEAR_EIO);
                    }
                    let n = txt.len();
                    if st.send_queue_bytes.saturating_add(n) > st.max_send_queue_bytes {
                        return Err(-SPEAR_EAGAIN);
                    }
                    st.send_queue.push_back(RtAsrSendItem::WsText(txt));
                    st.send_queue_bytes = st.send_queue_bytes.saturating_add(n);
                }
                self.recompute_rtasr_readiness_locked(&mut e);
                let notify = e.poll_mask.bits() != old.bits();
                drop(e);

                if notify {
                    self.fd_table.notify_watchers(fd);
                }
                Ok(None)
            }
            RTASR_CTL_FLUSH => {
                let mut e = entry.lock().map_err(|_| -SPEAR_EIO)?;
                if e.closed {
                    return Err(-SPEAR_EBADF);
                }
                let FdInner::RtAsr(st) = &mut e.inner else {
                    return Err(-SPEAR_EBADF);
                };
                if st.state == RtAsrConnState::Closed {
                    return Err(-SPEAR_EBADF);
                }
                if st.state == RtAsrConnState::Error {
                    return Err(-SPEAR_EIO);
                }
                let txt = segmentation::rtasr_flush_event_text();
                let n = txt.len();
                if st.send_queue_bytes.saturating_add(n) > st.max_send_queue_bytes {
                    return Err(-SPEAR_EAGAIN);
                }
                st.send_queue.push_back(RtAsrSendItem::WsText(txt));
                st.send_queue_bytes = st.send_queue_bytes.saturating_add(n);
                st.pending_flush = true;
                st.buffered_audio_bytes_since_flush = 0;
                st.last_flush_at = std::time::Instant::now();

                let old = e.poll_mask;
                self.recompute_rtasr_readiness_locked(&mut e);
                let notify = e.poll_mask.bits() != old.bits();
                drop(e);
                if notify {
                    self.fd_table.notify_watchers(fd);
                }
                Ok(None)
            }
            RTASR_CTL_CLEAR => {
                let mut e = entry.lock().map_err(|_| -SPEAR_EIO)?;
                if e.closed {
                    return Err(-SPEAR_EBADF);
                }
                let FdInner::RtAsr(st) = &mut e.inner else {
                    return Err(-SPEAR_EBADF);
                };
                if st.state == RtAsrConnState::Closed {
                    return Err(-SPEAR_EBADF);
                }
                if st.state == RtAsrConnState::Error {
                    return Err(-SPEAR_EIO);
                }
                let txt = segmentation::rtasr_clear_event_text();
                let n = txt.len();
                if st.send_queue_bytes.saturating_add(n) > st.max_send_queue_bytes {
                    return Err(-SPEAR_EAGAIN);
                }
                st.send_queue.push_back(RtAsrSendItem::WsText(txt));
                st.send_queue_bytes = st.send_queue_bytes.saturating_add(n);
                st.pending_flush = false;
                st.buffered_audio_bytes_since_flush = 0;
                st.last_flush_at = std::time::Instant::now();

                let old = e.poll_mask;
                self.recompute_rtasr_readiness_locked(&mut e);
                let notify = e.poll_mask.bits() != old.bits();
                drop(e);
                if notify {
                    self.fd_table.notify_watchers(fd);
                }
                Ok(None)
            }
            RTASR_CTL_SET_AUTOFLUSH => {
                let bytes = payload.ok_or(-SPEAR_EINVAL)?;
                let v: serde_json::Value =
                    serde_json::from_slice(bytes).map_err(|_| -SPEAR_EINVAL)?;
                let cfg = segmentation::parse_segmentation_config(&v)?;

                let mut e = entry.lock().map_err(|_| -SPEAR_EIO)?;
                if e.closed {
                    return Err(-SPEAR_EBADF);
                }
                let old = e.poll_mask;
                let now = std::time::Instant::now();
                let FdInner::RtAsr(st) = &mut e.inner else {
                    return Err(-SPEAR_EBADF);
                };
                st.segmentation = cfg;
                st.pending_flush = false;
                st.buffered_audio_bytes_since_flush = 0;
                st.last_flush_at = now;
                st.last_audio_at = now;

                self.recompute_rtasr_readiness_locked(&mut e);
                let notify = e.poll_mask.bits() != old.bits();
                drop(e);
                if notify {
                    self.fd_table.notify_watchers(fd);
                }
                Ok(None)
            }
            RTASR_CTL_GET_AUTOFLUSH => {
                let e = entry.lock().map_err(|_| -SPEAR_EIO)?;
                if e.closed {
                    return Err(-SPEAR_EBADF);
                }
                let FdInner::RtAsr(st) = &e.inner else {
                    return Err(-SPEAR_EBADF);
                };
                let body = segmentation::segmentation_config_to_json(&st.segmentation);
                let bytes = serde_json::to_vec(&body).map_err(|_| -SPEAR_EIO)?;
                Ok(Some(bytes))
            }
            _ => Err(-SPEAR_EINVAL),
        }
    }

    pub fn rtasr_write(&self, fd: i32, bytes: &[u8]) -> i32 {
        let Some(entry) = self.fd_table.get(fd) else {
            return -SPEAR_EBADF;
        };
        let mut e = match entry.lock() {
            Ok(v) => v,
            Err(_) => return -SPEAR_EIO,
        };
        if e.closed {
            return -SPEAR_EBADF;
        }

        let old = e.poll_mask;
        let rc = {
            let FdInner::RtAsr(st) = &mut e.inner else {
                return -SPEAR_EBADF;
            };
            if st.state == RtAsrConnState::Error {
                return -SPEAR_EIO;
            }
            if st.send_queue_bytes.saturating_add(bytes.len()) > st.max_send_queue_bytes {
                -SPEAR_EAGAIN
            } else {
                st.send_queue
                    .push_back(RtAsrSendItem::Audio(bytes.to_vec()));
                st.send_queue_bytes = st.send_queue_bytes.saturating_add(bytes.len());
                st.buffered_audio_bytes_since_flush = st
                    .buffered_audio_bytes_since_flush
                    .saturating_add(bytes.len());
                let now = std::time::Instant::now();
                st.last_audio_at = now;
                segmentation::maybe_enqueue_autoflush_locked(st, now);
                bytes.len() as i32
            }
        };

        self.recompute_rtasr_readiness_locked(&mut e);
        let notify = e.poll_mask.bits() != old.bits();
        drop(e);

        if notify {
            self.fd_table.notify_watchers(fd);
        }
        rc
    }

    pub fn rtasr_read(&self, fd: i32) -> Result<Vec<u8>, i32> {
        let Some(entry) = self.fd_table.get(fd) else {
            return Err(-SPEAR_EBADF);
        };
        let mut e = entry.lock().map_err(|_| -SPEAR_EIO)?;
        if e.closed {
            return Err(-SPEAR_EBADF);
        }

        let old = e.poll_mask;
        let mut payload: Option<Vec<u8>> = None;
        {
            let FdInner::RtAsr(st) = &mut e.inner else {
                return Err(-SPEAR_EBADF);
            };
            if let Some(p) = st.recv_queue.pop_front() {
                st.recv_queue_bytes = st.recv_queue_bytes.saturating_sub(p.len());
                payload = Some(p);
            }
        }

        self.recompute_rtasr_readiness_locked(&mut e);
        let notify = e.poll_mask.bits() != old.bits();
        drop(e);
        if notify {
            self.fd_table.notify_watchers(fd);
        }

        match payload {
            Some(p) => Ok(p),
            None => Err(-SPEAR_EAGAIN),
        }
    }

    pub fn rtasr_close(&self, fd: i32) -> i32 {
        if let Some(entry) = self.fd_table.get(fd) {
            if let Ok(mut e) = entry.lock() {
                if let FdInner::RtAsr(st) = &mut e.inner {
                    st.state = RtAsrConnState::Closed;
                }
            }
        }
        self.fd_table.close(fd)
    }
}
