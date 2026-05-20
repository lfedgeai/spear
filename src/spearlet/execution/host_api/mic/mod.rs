mod readiness;
mod source_device;
mod source_stub;

use crate::spearlet::execution::host_api::errno::{
    SPEAR_EAGAIN, SPEAR_EBADF, SPEAR_EINVAL, SPEAR_EIO,
};
use crate::spearlet::execution::host_api::DefaultHostApi;
use crate::spearlet::execution::hostcall::types::{
    FdEntry, FdFlags, FdInner, FdKind, MicConfig, MicState, PollEvents,
};
use base64::{engine::general_purpose, Engine as _};
use std::collections::HashSet;

impl DefaultHostApi {
    pub fn mic_create(&self) -> i32 {
        self.fd_table.alloc(FdEntry {
            kind: FdKind::Mic,
            flags: FdFlags::default(),
            poll_mask: PollEvents::default(),
            watchers: HashSet::new(),
            closed: false,
            inner: FdInner::Mic(MicState::default()),
        })
    }

    pub fn mic_ctl(
        &self,
        fd: i32,
        cmd: i32,
        payload: Option<&[u8]>,
    ) -> Result<Option<Vec<u8>>, i32> {
        const MIC_CTL_SET_PARAM: i32 = 1;
        const MIC_CTL_GET_STATUS: i32 = 2;

        let Some(entry) = self.fd_table.get(fd) else {
            return Err(-SPEAR_EBADF);
        };

        match cmd {
            MIC_CTL_SET_PARAM => {
                let bytes = payload.ok_or(-SPEAR_EINVAL)?;
                let v: serde_json::Value =
                    serde_json::from_slice(bytes).map_err(|_| -SPEAR_EINVAL)?;
                let sample_rate_hz = v
                    .get("sample_rate_hz")
                    .and_then(|x| x.as_u64())
                    .unwrap_or(24000) as u32;
                let channels = v.get("channels").and_then(|x| x.as_u64()).unwrap_or(1) as u8;
                let frame_ms = v.get("frame_ms").and_then(|x| x.as_u64()).unwrap_or(20) as u32;
                let format = v
                    .get("format")
                    .and_then(|x| x.as_str())
                    .unwrap_or("pcm16")
                    .to_string();

                let source = v.get("source").and_then(|x| x.as_str()).unwrap_or("device");
                let device_name = v
                    .get("device")
                    .and_then(|x| x.get("name"))
                    .and_then(|x| x.as_str())
                    .map(|s| s.to_string());

                let max_queue_bytes = v
                    .get("max_queue_bytes")
                    .and_then(|x| x.as_u64())
                    .map(|n| n as usize);
                let fallback_to_stub = v
                    .get("fallback")
                    .and_then(|x| x.get("to_stub"))
                    .and_then(|x| x.as_bool())
                    .unwrap_or(true);

                let stub_pcm16 = v
                    .get("stub_pcm16_base64")
                    .and_then(|x| x.as_str())
                    .map(|s| s.to_string());

                let cfg = MicConfig {
                    sample_rate_hz,
                    channels,
                    frame_ms,
                    format,
                };

                let (notify, generation) = {
                    let mut e = entry.lock().map_err(|_| -SPEAR_EIO)?;
                    if e.closed {
                        return Err(-SPEAR_EBADF);
                    }
                    let FdInner::Mic(st) = &mut e.inner else {
                        return Err(-SPEAR_EBADF);
                    };

                    st.running = false;
                    st.queue.clear();
                    st.queue_bytes = 0;
                    st.last_error = None;

                    st.generation = st.generation.wrapping_add(1);
                    let generation = st.generation;

                    st.config = Some(cfg.clone());
                    if let Some(mq) = max_queue_bytes {
                        st.max_queue_bytes = mq;
                    }
                    if let Some(s) = stub_pcm16.as_ref() {
                        let bytes = general_purpose::STANDARD
                            .decode(s.trim())
                            .map_err(|_| -SPEAR_EINVAL)?;
                        if bytes.len() > 10 * 1024 * 1024 {
                            return Err(-SPEAR_EINVAL);
                        }
                        st.stub_pcm16 = Some(bytes);
                        st.stub_pcm16_offset = 0;
                    }

                    st.running = true;

                    let old = e.poll_mask;
                    self.recompute_mic_readiness_locked(&mut e);
                    (e.poll_mask.bits() != old.bits(), generation)
                };
                if notify {
                    self.fd_table.notify_watchers(fd);
                }
                if source == "device" {
                    let req = source_device::DeviceMicStartRequest {
                        fd,
                        config: cfg,
                        device_name,
                        generation,
                    };
                    match self.spawn_mic_device_task(req) {
                        Ok(()) => {}
                        Err(source_device::DeviceMicStartError::NotImplemented) => {
                            if fallback_to_stub {
                                self.spawn_mic_stub_task(fd, generation);
                            } else {
                                let msg =
                                    "device mic not enabled (build without feature mic-device)";
                                let notify_err = {
                                    let mut e = entry.lock().map_err(|_| -SPEAR_EIO)?;
                                    if e.closed {
                                        return Err(-SPEAR_EBADF);
                                    }
                                    let FdInner::Mic(st) = &mut e.inner else {
                                        return Err(-SPEAR_EBADF);
                                    };
                                    st.running = false;
                                    st.generation = st.generation.wrapping_add(1);
                                    st.last_error = Some(msg.to_string());
                                    let old = e.poll_mask;
                                    self.recompute_mic_readiness_locked(&mut e);
                                    e.poll_mask.bits() != old.bits()
                                };
                                if notify_err {
                                    self.fd_table.notify_watchers(fd);
                                }
                                return Err(-SPEAR_EIO);
                            }
                        }
                        Err(source_device::DeviceMicStartError::Failed(msg)) => {
                            if fallback_to_stub {
                                self.spawn_mic_stub_task(fd, generation);
                            } else {
                                let notify_err = {
                                    let mut e = entry.lock().map_err(|_| -SPEAR_EIO)?;
                                    if e.closed {
                                        return Err(-SPEAR_EBADF);
                                    }
                                    let FdInner::Mic(st) = &mut e.inner else {
                                        return Err(-SPEAR_EBADF);
                                    };
                                    st.running = false;
                                    st.generation = st.generation.wrapping_add(1);
                                    st.last_error = Some(msg);
                                    let old = e.poll_mask;
                                    self.recompute_mic_readiness_locked(&mut e);
                                    e.poll_mask.bits() != old.bits()
                                };
                                if notify_err {
                                    self.fd_table.notify_watchers(fd);
                                }
                                return Err(-SPEAR_EIO);
                            }
                        }
                    }
                } else {
                    self.spawn_mic_stub_task(fd, generation);
                }
                Ok(None)
            }
            MIC_CTL_GET_STATUS => {
                let e = entry.lock().map_err(|_| -SPEAR_EIO)?;
                if e.closed {
                    return Err(-SPEAR_EBADF);
                }
                let FdInner::Mic(st) = &e.inner else {
                    return Err(-SPEAR_EBADF);
                };
                let body = serde_json::json!({
                    "running": st.running,
                    "last_error": st.last_error,
                    "queue_bytes": st.queue_bytes,
                    "max_queue_bytes": st.max_queue_bytes,
                    "dropped_frames": st.dropped_frames,
                    "generation": st.generation,
                    "config": st.config.as_ref().map(|c| serde_json::json!({
                        "sample_rate_hz": c.sample_rate_hz,
                        "channels": c.channels,
                        "frame_ms": c.frame_ms,
                        "format": c.format,
                    })),
                });
                let bytes = serde_json::to_vec(&body).map_err(|_| -SPEAR_EIO)?;
                Ok(Some(bytes))
            }
            _ => Err(-SPEAR_EINVAL),
        }
    }

    pub fn mic_read(&self, fd: i32) -> Result<Vec<u8>, i32> {
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
            let FdInner::Mic(st) = &mut e.inner else {
                return Err(-SPEAR_EBADF);
            };
            if let Some(p) = st.queue.pop_front() {
                st.queue_bytes = st.queue_bytes.saturating_sub(p.len());
                payload = Some(p);
            }
        }

        self.recompute_mic_readiness_locked(&mut e);
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

    pub fn mic_close(&self, fd: i32) -> i32 {
        self.fd_table.close(fd)
    }
}
