use crate::spearlet::execution::ai::streaming::{StreamingPrepareStep, StreamingWebsocketPlan};
use crate::spearlet::execution::host_api::DefaultHostApi;
use crate::spearlet::execution::hostcall::fd_table::FdTable;
use crate::spearlet::execution::hostcall::types::{
    FdInner, PollEvents, RtAsrConnState, RtAsrSendItem,
};
use std::collections::HashMap;

use super::super::util::{
    build_ws_request_with_headers, expand_json_templates, expand_template, extract_json_path,
};
use super::segmentation::maybe_enqueue_autoflush_locked;

type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;
type WsWrite = futures::stream::SplitSink<WsStream, tokio_tungstenite::tungstenite::Message>;
type WsRead = futures::stream::SplitStream<WsStream>;

fn set_rtasr_error(table: &FdTable, fd: i32, msg: String) {
    if let Some(entry) = table.get(fd) {
        if let Ok(mut e) = entry.lock() {
            if let FdInner::RtAsr(st) = &mut e.inner {
                st.last_error = Some(msg);
                st.state = RtAsrConnState::Error;
            }
            e.poll_mask.insert(PollEvents::ERR);
        }
        table.notify_watchers(fd);
    }
}

fn set_rtasr_hup(table: &FdTable, fd: i32) {
    if let Some(entry) = table.get(fd) {
        if let Ok(mut e) = entry.lock() {
            e.poll_mask.insert(PollEvents::HUP);
        }
        table.notify_watchers(fd);
    }
}

fn push_rtasr_event(table: &FdTable, fd: i32, payload: Vec<u8>) {
    if let Some(entry) = table.get(fd) {
        let mut notify = false;
        if let Ok(mut e) = entry.lock() {
            if e.closed {
                return;
            }
            let old = e.poll_mask;
            let is_closed = e.closed;

            let mut new_mask: Option<PollEvents> = None;
            if let FdInner::RtAsr(st) = &mut e.inner {
                st.recv_queue_bytes = st.recv_queue_bytes.saturating_add(payload.len());
                st.recv_queue.push_back(payload);
                while st.recv_queue_bytes > st.max_recv_queue_bytes {
                    let Some(oldest) = st.recv_queue.pop_front() else {
                        st.recv_queue_bytes = 0;
                        break;
                    };
                    st.recv_queue_bytes = st.recv_queue_bytes.saturating_sub(oldest.len());
                    st.dropped_events = st.dropped_events.wrapping_add(1);
                }

                let mut mask = PollEvents::EMPTY;
                if !st.recv_queue.is_empty() {
                    mask.insert(PollEvents::IN);
                }
                let writable = st.send_queue_bytes < st.max_send_queue_bytes
                    && st.state != RtAsrConnState::Draining
                    && st.state != RtAsrConnState::Closed
                    && st.state != RtAsrConnState::Error
                    && !is_closed;
                if writable {
                    mask.insert(PollEvents::OUT);
                }
                if st.state == RtAsrConnState::Error {
                    mask.insert(PollEvents::ERR);
                }
                if is_closed || st.state == RtAsrConnState::Closed {
                    mask.insert(PollEvents::HUP);
                }
                new_mask = Some(mask);
            }
            if let Some(m) = new_mask {
                e.poll_mask = m;
            }

            notify = e.poll_mask.bits() != old.bits();
        }
        if notify {
            table.notify_watchers(fd);
        }
    }
}

async fn prepare_vars(
    plan: &StreamingWebsocketPlan,
    client_secret_override: Option<String>,
    global_env: &HashMap<String, String>,
) -> Result<HashMap<String, String>, String> {
    let mut vars: HashMap<String, String> = HashMap::new();
    if let Some(s) = client_secret_override {
        vars.insert("client_secret".to_string(), s);
        return Ok(vars);
    }

    for step in plan.prepare.iter() {
        match step {
            StreamingPrepareStep::HttpJson(p) => {
                let url = expand_template(&p.url, &vars, global_env);
                let body = expand_json_templates(p.body.clone(), &vars, global_env);
                let mut req = reqwest::Client::new()
                    .request(p.method.parse().unwrap_or(reqwest::Method::POST), url);
                for (k, v) in p.headers.iter() {
                    let hv = expand_template(v, &vars, global_env);
                    req = req.header(k, hv);
                }
                let resp = tokio::time::timeout(
                    std::time::Duration::from_secs(20),
                    req.json(&body).send(),
                )
                .await
                .map_err(|_| "prepare http request timed out".to_string())?
                .map_err(|e| format!("prepare http request failed: {e}"))?;
                let status = resp.status();
                let bytes = resp
                    .bytes()
                    .await
                    .map_err(|e| format!("prepare http read failed: {e}"))?;
                let json_v: serde_json::Value = serde_json::from_slice(&bytes)
                    .map_err(|e| format!("prepare http invalid json: {e}"))?;
                if !status.is_success() {
                    let msg = json_v
                        .get("error")
                        .and_then(|x| x.get("message"))
                        .and_then(|x| x.as_str())
                        .unwrap_or("upstream error");
                    return Err(format!("prepare http failed: {}: {}", status.as_u16(), msg));
                }
                let extracted = extract_json_path(&json_v, &p.extract_json_path)
                    .ok_or_else(|| format!("prepare extract failed: {}", p.extract_json_path))?;
                vars.insert(p.extract_to_var.clone(), extracted);
            }
        }
    }
    Ok(vars)
}

fn build_ws_url(plan: &StreamingWebsocketPlan, ws_url_override: Option<String>) -> String {
    ws_url_override.unwrap_or_else(|| plan.websocket.url.clone())
}

fn build_ws_request(
    ws_url: &str,
    plan: &StreamingWebsocketPlan,
    vars: &HashMap<String, String>,
    global_env: &HashMap<String, String>,
) -> Result<tokio_tungstenite::tungstenite::handshake::client::Request, String> {
    build_ws_request_with_headers(ws_url, &plan.websocket.headers, vars, global_env)
}

async fn connect_ws(
    request: tokio_tungstenite::tungstenite::handshake::client::Request,
) -> Result<WsStream, String> {
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        tokio_tungstenite::connect_async(request).await
    })
    .await
    .map_err(|_| "websocket connect timed out".to_string())?
    .map(|(ws_stream, _)| ws_stream)
    .map_err(|e| format!("websocket connect failed: {e}"))
}

async fn send_client_events(
    ws_write: &mut WsWrite,
    client_events: &[serde_json::Value],
) -> Result<(), String> {
    use futures::SinkExt;
    for ev in client_events.iter() {
        let txt =
            serde_json::to_string(ev).map_err(|e| format!("websocket event encode failed: {e}"))?;
        ws_write
            .send(tokio_tungstenite::tungstenite::Message::Text(txt))
            .await
            .map_err(|e| format!("websocket send failed: {e}"))?;
    }
    Ok(())
}

async fn pop_send_item_and_update_mask(
    table: &FdTable,
    fd: i32,
) -> Result<Option<RtAsrSendItem>, String> {
    let Some(entry) = table.get(fd) else {
        return Ok(None);
    };
    let mut e = entry.lock().map_err(|_| "fd lock poisoned".to_string())?;
    if e.closed {
        return Ok(None);
    }
    let old = e.poll_mask;
    let is_closed = e.closed;
    let (item, mask) = {
        let FdInner::RtAsr(st) = &mut e.inner else {
            return Err("fd kind mismatch".to_string());
        };
        if st.state == RtAsrConnState::Error || st.state == RtAsrConnState::Closed {
            return Ok(None);
        }
        let item = st.send_queue.pop_front();
        if let Some(ref it) = item {
            st.send_queue_bytes = st.send_queue_bytes.saturating_sub(it.byte_len());
        }
        let mut mask = PollEvents::EMPTY;
        if !st.recv_queue.is_empty() {
            mask.insert(PollEvents::IN);
        }
        let writable = st.send_queue_bytes < st.max_send_queue_bytes
            && st.state != RtAsrConnState::Draining
            && st.state != RtAsrConnState::Closed
            && st.state != RtAsrConnState::Error
            && !is_closed;
        if writable {
            mask.insert(PollEvents::OUT);
        }
        if st.state == RtAsrConnState::Error {
            mask.insert(PollEvents::ERR);
        }
        if is_closed || st.state == RtAsrConnState::Closed {
            mask.insert(PollEvents::HUP);
        }
        (item, mask)
    };
    e.poll_mask = mask;
    let notify = e.poll_mask.bits() != old.bits();
    drop(e);
    if notify {
        table.notify_watchers(fd);
    }
    Ok(item)
}

async fn maybe_enqueue_autoflush(table: &FdTable, fd: i32) -> Result<bool, String> {
    let Some(entry) = table.get(fd) else {
        return Ok(false);
    };
    let mut e = entry.lock().map_err(|_| "fd lock poisoned".to_string())?;
    if e.closed {
        return Ok(false);
    }
    if let FdInner::RtAsr(st) = &mut e.inner {
        let before = st.send_queue_bytes;
        let now = std::time::Instant::now();
        maybe_enqueue_autoflush_locked(st, now);
        return Ok(st.send_queue_bytes != before);
    }
    Ok(false)
}

fn clear_pending_flush(table: &FdTable, fd: i32) -> Result<(), String> {
    let Some(entry) = table.get(fd) else {
        return Ok(());
    };
    let mut e = entry.lock().map_err(|_| "fd lock poisoned".to_string())?;
    if let FdInner::RtAsr(st) = &mut e.inner {
        st.pending_flush = false;
    }
    Ok(())
}

fn spawn_ws_writer(
    table: std::sync::Arc<FdTable>,
    fd: i32,
    mut ws_write: WsWrite,
) -> tokio::task::JoinHandle<Result<(), String>> {
    tokio::spawn(async move {
        let res: Result<(), String> = async {
            use futures::SinkExt;
            loop {
                let item = pop_send_item_and_update_mask(table.as_ref(), fd).await?;
                let Some(item) = item else {
                    if maybe_enqueue_autoflush(table.as_ref(), fd).await? {
                        continue;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                    continue;
                };

                let sent_text = matches!(&item, RtAsrSendItem::WsText(_));
                match item {
                    RtAsrSendItem::Audio(chunk) => {
                        let audio_b64 = {
                            use base64::Engine;
                            base64::engine::general_purpose::STANDARD.encode(&chunk)
                        };
                        let body = serde_json::json!({
                            "type": "input_audio_buffer.append",
                            "audio": audio_b64,
                        });
                        let txt = serde_json::to_string(&body).map_err(|e| e.to_string())?;
                        ws_write
                            .send(tokio_tungstenite::tungstenite::Message::Text(txt))
                            .await
                            .map_err(|e| format!("websocket send failed: {e}"))?;
                    }
                    RtAsrSendItem::WsText(txt) => {
                        ws_write
                            .send(tokio_tungstenite::tungstenite::Message::Text(txt))
                            .await
                            .map_err(|e| format!("websocket send failed: {e}"))?;
                    }
                }

                if sent_text {
                    clear_pending_flush(table.as_ref(), fd)?;
                }
            }
            #[allow(unreachable_code)]
            Ok(())
        }
        .await;
        res
    })
}

fn spawn_ws_reader(
    table: std::sync::Arc<FdTable>,
    fd: i32,
    mut ws_read: WsRead,
) -> tokio::task::JoinHandle<Result<(), String>> {
    tokio::spawn(async move {
        let res: Result<(), String> = async {
            use futures::StreamExt;
            loop {
                let msg = ws_read
                    .next()
                    .await
                    .ok_or_else(|| "websocket closed".to_string())
                    .and_then(|r| r.map_err(|e| format!("websocket read failed: {e}")))?;

                let payload: Option<Vec<u8>> = match msg {
                    tokio_tungstenite::tungstenite::Message::Text(s) => Some(s.into_bytes()),
                    tokio_tungstenite::tungstenite::Message::Binary(b) => Some(b),
                    tokio_tungstenite::tungstenite::Message::Close(_) => {
                        set_rtasr_hup(table.as_ref(), fd);
                        return Ok::<(), String>(());
                    }
                    _ => None,
                };

                if let Some(p) = payload {
                    push_rtasr_event(table.as_ref(), fd, p);
                }
            }
            #[allow(unreachable_code)]
            Ok(())
        }
        .await;
        res
    })
}

impl DefaultHostApi {
    pub(super) fn spawn_rtasr_websocket_tasks(
        &self,
        fd: i32,
        plan: StreamingWebsocketPlan,
        ws_url_override: Option<String>,
        client_secret_override: Option<String>,
    ) {
        let table = self.fd_table.clone();
        let global_env = self.runtime_config.global_environment.clone();

        self.spawn_background(async move {
            let vars = match prepare_vars(&plan, client_secret_override, &global_env).await {
                Ok(v) => v,
                Err(e) => {
                    set_rtasr_error(&table, fd, e);
                    return;
                }
            };

            let ws_url = build_ws_url(&plan, ws_url_override);
            let request = match build_ws_request(&ws_url, &plan, &vars, &global_env) {
                Ok(r) => r,
                Err(e) => {
                    set_rtasr_error(&table, fd, e);
                    return;
                }
            };

            let ws_stream = match connect_ws(request).await {
                Ok(v) => v,
                Err(e) => {
                    set_rtasr_error(&table, fd, e);
                    return;
                }
            };

            use futures::StreamExt;
            let (mut ws_write, ws_read) = ws_stream.split();
            if let Err(e) = send_client_events(&mut ws_write, &plan.websocket.client_events).await {
                set_rtasr_error(&table, fd, e);
                return;
            }

            let t_writer = spawn_ws_writer(table.clone(), fd, ws_write);
            let t_reader = spawn_ws_reader(table.clone(), fd, ws_read);

            let res = tokio::select! {
                r = t_writer => match r {
                    Ok(Ok(())) => Ok(()),
                    Ok(Err(e)) => Err(e),
                    Err(e) => Err(format!("writer join error: {e}")),
                },
                r = t_reader => match r {
                    Ok(Ok(())) => Ok(()),
                    Ok(Err(e)) => Err(e),
                    Err(e) => Err(format!("reader join error: {e}")),
                },
            };

            if let Err(e) = res {
                set_rtasr_error(&table, fd, e);
            }
        });
    }
}
