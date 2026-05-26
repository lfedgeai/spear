//! Endpoint gateway handlers / Endpoint 网关处理器
//!
//! This module implements `GET /e/{endpoint}/ws` which upgrades to WebSocket and proxies SSF
//! frames to one or more upstream executions (via Spearlet execution stream WS).
//! 本模块实现 `GET /e/{endpoint}/ws`：升级为 WebSocket，并将 SSF 帧代理到一个或多个上游 execution
//!（通过 Spearlet execution stream WS）。

use axum::{
    extract::{
        ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
};
use futures::{SinkExt, StreamExt};
use serde_json::json;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc;
use tonic::Request;
use tracing::{info, warn};
use uuid::Uuid;

use super::common::ErrorResponse;
use crate::proto::sms::{
    ExecutionStatus, GetExecutionRequest, GetNodeRequest, InstanceStatus,
    ListInstanceExecutionsRequest, ListTaskInstancesRequest, ResolveEndpointRequest,
};
use crate::sms::gateway::GatewayState;

const GATEWAY_ENDPOINT_MAX_LEN: usize = 64;

const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;
const MAX_ACTIVE_STREAMS_PER_CONN: usize = 1024;

const HANDSHAKE_TOTAL_TIMEOUT: Duration = Duration::from_secs(30);
const EXECUTION_START_TIMEOUT: Duration = Duration::from_secs(10);
const UPSTREAM_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

/// Endpoint gateway WS handler / Endpoint 网关 WS 处理器
pub async fn endpoint_ws_proxy(
    Path(endpoint): Path<String>,
    State(state): State<GatewayState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let normalized = match normalize_gateway_endpoint(&endpoint) {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "INVALID_ENDPOINT".to_string(),
                    message: e,
                }),
            )
                .into_response();
        }
    };

    let task = match tokio::time::timeout(
        HANDSHAKE_TOTAL_TIMEOUT,
        resolve_task_by_gateway_endpoint(&state, &normalized),
    )
    .await
    {
        Ok(Ok(Some(task))) => task,
        Ok(Ok(None)) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "ENDPOINT_NOT_FOUND".to_string(),
                    message: "endpoint not registered".to_string(),
                }),
            )
                .into_response();
        }
        Ok(Err(e)) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(ErrorResponse {
                    error: "TASK_RESOLVE_FAILED".to_string(),
                    message: e,
                }),
            )
                .into_response();
        }
        Err(_) => {
            return (
                StatusCode::GATEWAY_TIMEOUT,
                Json(ErrorResponse {
                    error: "UPSTREAM_TIMEOUT".to_string(),
                    message: "resolve task timeout".to_string(),
                }),
            )
                .into_response();
        }
    };

    let task_id = task.task_id.clone();
    let preferred_node_uuid = task.node_uuid.clone();

    let mut candidate_executions = match list_running_executions_for_task(&state, &task_id).await {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(ErrorResponse {
                    error: "EXECUTION_INDEX_ERROR".to_string(),
                    message: e,
                }),
            )
                .into_response();
        }
    };

    if candidate_executions.is_empty() {
        match tokio::time::timeout(
            HANDSHAKE_TOTAL_TIMEOUT,
            start_execution_for_task(&state, &task_id, &preferred_node_uuid),
        )
        .await
        {
            Ok(Ok(exec_id)) => candidate_executions.push(exec_id),
            Ok(Err(e)) => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(ErrorResponse {
                        error: "NO_EXECUTION_AVAILABLE".to_string(),
                        message: e,
                    }),
                )
                    .into_response();
            }
            Err(_) => {
                return (
                    StatusCode::GATEWAY_TIMEOUT,
                    Json(ErrorResponse {
                        error: "UPSTREAM_TIMEOUT".to_string(),
                        message: "start execution timeout".to_string(),
                    }),
                )
                    .into_response();
            }
        }
    }

    let accepted_protocol = pick_subprotocol(&headers);
    let ws = if accepted_protocol.is_some() {
        ws.protocols(["ssf.v1"])
    } else {
        ws
    };

    ws.on_upgrade(move |socket| async move {
        endpoint_ws_proxy_loop(state, normalized, task_id, candidate_executions, socket).await;
    })
}

async fn resolve_task_by_gateway_endpoint(
    state: &GatewayState,
    gateway_endpoint: &str,
) -> Result<Option<crate::proto::sms::Task>, String> {
    let mut client = state.task_client.clone();
    let resp = client
        .resolve_endpoint(Request::new(ResolveEndpointRequest {
            endpoint: gateway_endpoint.to_string(),
        }))
        .await
        .map_err(|e| format!("task_service error: {e}"))?
        .into_inner();
    if !resp.found {
        return Ok(None);
    }
    let Some(task) = resp.task else {
        return Err("task_service returned empty task".to_string());
    };
    if task.task_id.is_empty() {
        return Err("task_service returned empty task".to_string());
    }
    Ok(Some(task))
}

async fn list_running_executions_for_task(
    state: &GatewayState,
    task_id: &str,
) -> Result<Vec<String>, String> {
    let mut idx_client = state.execution_index_client.clone();
    let mut page_token = String::new();
    let mut out = Vec::new();

    for _ in 0..10 {
        let resp = idx_client
            .list_task_instances(Request::new(ListTaskInstancesRequest {
                task_id: task_id.to_string(),
                limit: 50,
                page_token: page_token.clone(),
            }))
            .await
            .map_err(|e| format!("execution_index error: {e}"))?
            .into_inner();

        for inst in resp.instances {
            let status = InstanceStatus::try_from(inst.status).unwrap_or(InstanceStatus::Unknown);
            if !matches!(status, InstanceStatus::Running | InstanceStatus::Idle) {
                continue;
            }
            if !inst.current_execution_id.is_empty() {
                out.push(inst.current_execution_id);
                continue;
            }
            let executions = idx_client
                .list_instance_executions(Request::new(ListInstanceExecutionsRequest {
                    instance_id: inst.instance_id,
                    limit: 50,
                    page_token: String::new(),
                }))
                .await
                .map_err(|e| format!("execution_index error: {e}"))?
                .into_inner();
            for ex in executions.executions {
                let st = ExecutionStatus::try_from(ex.status).unwrap_or(ExecutionStatus::Unknown);
                if st == ExecutionStatus::Running {
                    out.push(ex.execution_id);
                }
            }
        }

        if resp.next_page_token.is_empty() {
            break;
        }
        page_token = resp.next_page_token;
    }

    out.sort();
    out.dedup();
    Ok(out)
}

async fn start_execution_for_task(
    state: &GatewayState,
    task_id: &str,
    preferred_node_uuid: &str,
) -> Result<String, String> {
    let request_id = Uuid::new_v4().to_string();
    let execution_id = Uuid::new_v4().to_string();

    if !preferred_node_uuid.is_empty() {
        let node = resolve_node_by_uuid(state, preferred_node_uuid).await?;
        invoke_on_node(
            state,
            &node.ip_address,
            node.port,
            task_id,
            &request_id,
            &execution_id,
        )
        .await?;
        wait_execution_visible(state, &execution_id).await?;
        return Ok(execution_id);
    }

    let mut placement = state.placement_client.clone();
    let placement_resp = placement
        .place_invocation(crate::proto::sms::PlaceInvocationRequest {
            request_id: request_id.clone(),
            task_id: task_id.to_string(),
            max_candidates: 3,
            labels: HashMap::new(),
        })
        .await
        .map_err(|e| format!("placement error: {e}"))?
        .into_inner();

    if placement_resp.candidates.is_empty() {
        return Err("no placement candidates".to_string());
    }

    for c in placement_resp.candidates {
        match invoke_on_node(
            state,
            &c.ip_address,
            c.port,
            task_id,
            &request_id,
            &execution_id,
        )
        .await
        {
            Ok(_) => {
                wait_execution_visible(state, &execution_id).await?;
                return Ok(execution_id);
            }
            Err(e) => {
                warn!(
                    task_id = %task_id,
                    node_uuid = %c.node_uuid,
                    error = %e,
                    "endpoint gateway invoke failed, trying next candidate"
                );
                let _ = placement
                    .report_invocation_outcome(crate::proto::sms::ReportInvocationOutcomeRequest {
                        decision_id: placement_resp.decision_id.clone(),
                        request_id: request_id.clone(),
                        task_id: task_id.to_string(),
                        node_uuid: c.node_uuid.clone(),
                        outcome_class: crate::proto::sms::InvocationOutcomeClass::Unavailable
                            as i32,
                        error_message: e,
                    })
                    .await;
            }
        }
    }

    Err("all placement candidates failed".to_string())
}

async fn resolve_node_by_uuid(
    state: &GatewayState,
    node_uuid: &str,
) -> Result<crate::proto::sms::Node, String> {
    let mut client = state.node_client.clone();
    let resp = client
        .get_node(Request::new(GetNodeRequest {
            uuid: node_uuid.to_string(),
        }))
        .await
        .map_err(|e| format!("node_service error: {e}"))?
        .into_inner();
    if !resp.found {
        return Err("node not found".to_string());
    }
    resp.node.ok_or_else(|| "node missing".to_string())
}

async fn invoke_on_node(
    _state: &GatewayState,
    ip_address: &str,
    port: i32,
    task_id: &str,
    request_id: &str,
    execution_id: &str,
) -> Result<(), String> {
    use crate::proto::spearlet::{
        invocation_service_client::InvocationServiceClient, ExecutionMode, InvokeRequest, Payload,
    };

    let url = format!("http://{}:{}", ip_address, port);
    let channel = tonic::transport::Channel::from_shared(url)
        .map_err(|e| format!("invalid node url: {e}"))?
        .connect_lazy();
    let mut invc = InvocationServiceClient::new(channel);

    let req = InvokeRequest {
        invocation_id: request_id.to_string(),
        execution_id: execution_id.to_string(),
        task_id: task_id.to_string(),
        function_name: crate::spearlet::execution::DEFAULT_ENTRY_FUNCTION_NAME.to_string(),
        input: Some(Payload {
            content_type: "application/octet-stream".to_string(),
            data: Vec::new(),
        }),
        headers: HashMap::new(),
        environment: HashMap::new(),
        timeout_ms: 0,
        session_id: String::new(),
        mode: ExecutionMode::Async as i32,
        force_new_instance: false,
        metadata: HashMap::from([("spear.endpoint_gateway".to_string(), "true".to_string())]),
    };

    tokio::time::timeout(UPSTREAM_CONNECT_TIMEOUT, invc.invoke(req))
        .await
        .map_err(|_| "invoke timeout".to_string())?
        .map_err(|e| format!("invoke error: {e}"))?;

    Ok(())
}

async fn wait_execution_visible(state: &GatewayState, execution_id: &str) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + EXECUTION_START_TIMEOUT;
    loop {
        if tokio::time::Instant::now() > deadline {
            return Err("execution not visible in time".to_string());
        }
        let resp = state
            .execution_index_client
            .clone()
            .get_execution(Request::new(GetExecutionRequest {
                execution_id: execution_id.to_string(),
            }))
            .await
            .map_err(|e| format!("execution_index error: {e}"))?
            .into_inner();
        if resp.found {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

async fn resolve_spearlet_ws_url(
    state: &GatewayState,
    execution_id: &str,
) -> Result<String, String> {
    let mut idx_client = state.execution_index_client.clone();
    let resp = idx_client
        .get_execution(Request::new(GetExecutionRequest {
            execution_id: execution_id.to_string(),
        }))
        .await
        .map_err(|e| format!("execution_index error: {e}"))?
        .into_inner();
    if !resp.found {
        return Err("execution not found".to_string());
    }
    let node_uuid = resp
        .execution
        .as_ref()
        .map(|e| e.node_uuid.clone())
        .unwrap_or_default();
    if node_uuid.is_empty() {
        return Err("execution missing node_uuid".to_string());
    }

    let mut node_client = state.node_client.clone();
    let node = node_client
        .get_node(Request::new(GetNodeRequest { uuid: node_uuid }))
        .await
        .map_err(|e| format!("node_service error: {e}"))?
        .into_inner();
    if !node.found {
        return Err("node not found".to_string());
    }
    let Some(n) = node.node else {
        return Err("node missing".to_string());
    };
    let ip = n.ip_address;
    let http_port = if n.http_port > 0 {
        n.http_port as u16
    } else {
        n.metadata
            .get("http_port")
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(8081)
    };
    Ok(format!(
        "ws://{}:{}/api/v1/executions/{}/streams/ws",
        ip, http_port, execution_id
    ))
}

async fn endpoint_ws_proxy_loop(
    state: GatewayState,
    gateway_endpoint: String,
    task_id: String,
    initial_candidates: Vec<String>,
    socket: WebSocket,
) {
    let conn_id = Uuid::new_v4().to_string();
    info!(
        conn_id = %conn_id,
        gateway_endpoint = %gateway_endpoint,
        task_id = %task_id,
        "endpoint gateway ws connected"
    );

    let (mut client_tx, mut client_rx) = socket.split();

    let (client_out_tx, mut client_out_rx) = mpsc::unbounded_channel::<Message>();
    let client_writer = tokio::spawn(async move {
        while let Some(msg) = client_out_rx.recv().await {
            if client_tx.send(msg).await.is_err() {
                break;
            }
        }
    });

    let mut upstreams: HashMap<String, UpstreamState> = initial_candidates
        .into_iter()
        .map(|id| (id.clone(), UpstreamState::new(id)))
        .collect();

    let mut client_to_exec: HashMap<u32, String> = HashMap::new();
    let mut exec_to_client_id: HashMap<String, String> = HashMap::new();

    loop {
        tokio::select! {
            msg = client_rx.next() => {
                let Some(Ok(msg)) = msg else { break; };
                match msg {
                    Message::Binary(b) => {
                        if b.len() > MAX_FRAME_BYTES {
                            let _ = client_out_tx.send(Message::Close(Some(CloseFrame {
                                code: 1009,
                                reason: "frame too large".into(),
                            })));
                            break;
                        }
                        let hdr = match spear_ssf::parse_v1_header(&b) {
                            Ok(h) => h,
                            Err(_) => {
                                let _ = client_out_tx.send(Message::Close(Some(CloseFrame {
                                    code: 1008,
                                    reason: "invalid protocol".into(),
                                })));
                                break;
                            }
                        };

                        let client_stream_id = hdr.stream_id;
                        if spear_ssf::is_close(&hdr) {
                            if let Some(exec_id) = client_to_exec.remove(&client_stream_id) {
                                if let Some(up) = upstreams.get_mut(&exec_id) {
                                    up.on_stream_end();
                                }
                                let client_id = match exec_to_client_id.get(&exec_id) {
                                    Some(v) => v.clone(),
                                    None => {
                                        match state
                                            .execution_stream_pool
                                            .register_client(&state, &exec_id, client_out_tx.clone())
                                            .await
                                        {
                                            Ok(v) => {
                                                exec_to_client_id.insert(exec_id.clone(), v.clone());
                                                v
                                            }
                                            Err(_) => {
                                                let _ = client_out_tx.send(Message::Binary(build_ssf_error_frame(
                                                    client_stream_id,
                                                    "UPSTREAM_WS_FAILED",
                                                    "upstream ws failed",
                                                    true,
                                                )));
                                                continue;
                                            }
                                        }
                                    }
                                };
                                let _ = state
                                    .execution_stream_pool
                                    .forward_client_binary(&state, &exec_id, &client_id, &b)
                                    .await;
                            } else {
                                let _ = client_out_tx.send(Message::Binary(build_ssf_error_frame(
                                    client_stream_id,
                                    "STREAM_NOT_FOUND",
                                    "stream not found",
                                    false,
                                )));
                            }
                            continue;
                        }

                        if !client_to_exec.contains_key(&client_stream_id) {
                            if client_to_exec.len() >= MAX_ACTIVE_STREAMS_PER_CONN {
                                let _ = client_out_tx.send(Message::Binary(build_ssf_error_frame(
                                    client_stream_id,
                                    "RATE_LIMITED",
                                    "too many active streams",
                                    true,
                                )));
                                continue;
                            }

                            let selected = select_upstream(&mut upstreams);
                            let Some(exec_id) = selected else {
                                let _ = client_out_tx.send(Message::Binary(build_ssf_error_frame(
                                    client_stream_id,
                                    "NO_EXECUTION_AVAILABLE",
                                    "no upstream execution available",
                                    true,
                                )));
                                continue;
                            };

                            let up = upstreams.get_mut(&exec_id).expect("selected upstream must exist");
                            up.on_stream_start();
                            client_to_exec.insert(client_stream_id, exec_id.clone());
                        }

                        let exec_id = client_to_exec.get(&client_stream_id).expect("binding must exist").clone();
                        let client_id = match exec_to_client_id.get(&exec_id) {
                            Some(v) => v.clone(),
                            None => match state
                                .execution_stream_pool
                                .register_client(&state, &exec_id, client_out_tx.clone())
                                .await
                            {
                                Ok(v) => {
                                    exec_to_client_id.insert(exec_id.clone(), v.clone());
                                    v
                                }
                                Err(e) => {
                                    let hint = if e.contains("0.0.0.0")
                                        || e.contains("::")
                                        || e.contains("unspecified IP")
                                    {
                                        "check SPEARLET_ADVERTISE_IP/POD_IP and node ip_address"
                                            .to_string()
                                    } else if e.contains("HTTP 404")
                                        || e.contains("404 Not Found")
                                        || e.contains("HTTP error: 404")
                                    {
                                        "upstream returned 404; check node.http_port points to spearlet HTTP gateway and node.ip_address is reachable"
                                            .to_string()
                                    } else {
                                        String::new()
                                    };
                                    warn!(
                                        conn_id = %conn_id,
                                        execution_id = %exec_id,
                                        error = %e,
                                        hint = %hint,
                                        "register execution stream client failed"
                                    );
                                    let _ = client_out_tx.send(Message::Binary(build_ssf_error_frame(
                                        client_stream_id,
                                        "UPSTREAM_WS_FAILED",
                                        "upstream ws failed",
                                        true,
                                    )));
                                    continue;
                                }
                            },
                        };
                        if let Err(e) = state
                            .execution_stream_pool
                            .forward_client_binary(&state, &exec_id, &client_id, &b)
                            .await
                        {
                            warn!(
                                conn_id = %conn_id,
                                execution_id = %exec_id,
                                error = %e,
                                "forward to upstream failed"
                            );
                            let _ = client_out_tx.send(Message::Binary(build_ssf_error_frame(
                                client_stream_id,
                                "UPSTREAM_WS_FAILED",
                                "upstream ws failed",
                                true,
                            )));
                        }
                    }
                    Message::Ping(p) => {
                        let _ = client_out_tx.send(Message::Pong(p));
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
        }
    }

    for (exec_id, client_id) in exec_to_client_id {
        state
            .execution_stream_pool
            .unregister_client(&exec_id, &client_id)
            .await;
    }
    drop(client_out_tx);
    let _ = client_writer.await;
    info!(conn_id = %conn_id, "endpoint gateway ws disconnected");
}

struct UpstreamState {
    execution_id: String,
    active_streams: usize,
    healthy: bool,
}

impl UpstreamState {
    fn new(execution_id: String) -> Self {
        Self {
            execution_id,
            active_streams: 0,
            healthy: true,
        }
    }

    fn on_stream_start(&mut self) {
        self.active_streams = self.active_streams.saturating_add(1);
    }

    fn on_stream_end(&mut self) {
        self.active_streams = self.active_streams.saturating_sub(1);
    }

    fn mark_unhealthy(&mut self) {
        self.healthy = false;
    }
}

fn select_upstream(upstreams: &mut HashMap<String, UpstreamState>) -> Option<String> {
    upstreams
        .values()
        .filter(|u| u.healthy)
        .min_by_key(|u| u.active_streams)
        .map(|u| u.execution_id.clone())
}

fn build_ssf_error_frame(
    stream_id: u32,
    code: &str,
    message: &str,
    retryable: bool,
) -> prost::bytes::Bytes {
    let meta = serde_json::to_vec(&json!({
        "error": {
            "code": code,
            "message": message,
            "retryable": retryable,
        }
    }))
    .unwrap_or_else(|_| b"{}".to_vec());
    let data: Vec<u8> = Vec::new();
    prost::bytes::Bytes::from(spear_ssf::build_v1_frame(
        stream_id,
        spear_ssf::MsgType::Error.as_u16(),
        0,
        1,
        &meta,
        &data,
    ))
}

fn normalize_gateway_endpoint(v: &str) -> Result<String, String> {
    let trimmed = v.trim();
    if trimmed.is_empty() {
        return Err("endpoint is required".to_string());
    }
    if trimmed.len() > GATEWAY_ENDPOINT_MAX_LEN {
        return Err(format!(
            "endpoint too long: {} (max {})",
            trimmed.len(),
            GATEWAY_ENDPOINT_MAX_LEN
        ));
    }
    if !trimmed
        .as_bytes()
        .iter()
        .all(|c| c.is_ascii_alphanumeric() || *c == b'_' || *c == b'-')
    {
        return Err("endpoint must match ^[A-Za-z0-9_-]+$".to_string());
    }
    Ok(trimmed.to_ascii_lowercase())
}

fn pick_subprotocol(headers: &HeaderMap) -> Option<String> {
    let v = headers.get("sec-websocket-protocol")?.to_str().ok()?;
    for item in v.split(',') {
        let p = item.trim();
        if p.eq_ignore_ascii_case("ssf.v1") {
            return Some("ssf.v1".to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract_meta_json(frame: &[u8]) -> serde_json::Value {
        let header_len = u16::from_le_bytes([frame[6], frame[7]]) as usize;
        let meta_len = u32::from_le_bytes([frame[24], frame[25], frame[26], frame[27]]) as usize;
        let meta = &frame[header_len..header_len + meta_len];
        serde_json::from_slice(meta).unwrap()
    }

    #[test]
    fn normalize_gateway_endpoint_accepts_and_normalizes() {
        assert_eq!(normalize_gateway_endpoint("Echo_01").unwrap(), "echo_01");
        assert_eq!(normalize_gateway_endpoint("  E-CHO  ").unwrap(), "e-cho");
        assert!(normalize_gateway_endpoint("").is_err());
        assert!(normalize_gateway_endpoint(" ").is_err());
        assert!(normalize_gateway_endpoint("a/b").is_err());
        assert!(normalize_gateway_endpoint(&"a".repeat(GATEWAY_ENDPOINT_MAX_LEN + 1)).is_err());
    }

    #[test]
    fn pick_subprotocol_parses_header() {
        let mut headers = HeaderMap::new();
        assert!(pick_subprotocol(&headers).is_none());
        headers.insert("sec-websocket-protocol", "ssf.v1".parse().unwrap());
        assert_eq!(pick_subprotocol(&headers).unwrap(), "ssf.v1");
        headers.insert(
            "sec-websocket-protocol",
            "foo, SSF.V1 ,bar".parse().unwrap(),
        );
        assert_eq!(pick_subprotocol(&headers).unwrap(), "ssf.v1");
    }

    #[test]
    fn ssf_header_parse_and_rewrite_roundtrip() {
        let meta = br#"{"k":"v"}"#;
        let data = b"abc";
        let mut frame = spear_ssf::build_v1_frame(7, 2, 0, 1, meta, data);
        let hdr = spear_ssf::parse_v1_header(&frame).unwrap();
        assert_eq!(hdr.stream_id, 7);
        assert_eq!(hdr.msg_type, 2);

        spear_ssf::rewrite_stream_id_inplace(&mut frame, 42).unwrap();
        let hdr2 = spear_ssf::parse_v1_header(&frame).unwrap();
        assert_eq!(hdr2.stream_id, 42);
        assert_eq!(hdr2.msg_type, 2);
    }

    #[test]
    fn ssf_header_parse_rejects_invalid_frames() {
        let mut frame = spear_ssf::build_v1_frame(1, 1, 0, 1, b"{}", b"");
        frame[0] = b'X';
        assert!(spear_ssf::parse_v1_header(&frame).is_err());

        let mut frame = spear_ssf::build_v1_frame(1, 1, 0, 1, b"{}", b"");
        frame[4] = 2;
        frame[5] = 0;
        assert!(spear_ssf::parse_v1_header(&frame).is_err());

        let mut frame = spear_ssf::build_v1_frame(1, 1, 0, 1, b"{}", b"");
        frame.truncate(10);
        assert!(spear_ssf::parse_v1_header(&frame).is_err());

        let mut frame = spear_ssf::build_v1_frame(1, 1, 0, 1, b"{}", b"");
        frame[28..32].copy_from_slice(&1u32.to_le_bytes());
        assert!(spear_ssf::parse_v1_header(&frame).is_err());
    }

    #[test]
    fn build_ssf_error_frame_contains_structured_error_meta() {
        let frame = build_ssf_error_frame(9, "TASK_ERROR", "boom", true);
        let hdr = spear_ssf::parse_v1_header(&frame).unwrap();
        assert_eq!(hdr.stream_id, 9);
        assert_eq!(hdr.msg_type, spear_ssf::MsgType::Error.as_u16());

        let v = extract_meta_json(&frame);
        assert_eq!(v["error"]["code"].as_str().unwrap(), "TASK_ERROR");
        assert_eq!(v["error"]["message"].as_str().unwrap(), "boom");
        assert_eq!(v["error"]["retryable"].as_bool().unwrap(), true);
    }

    #[test]
    fn select_upstream_prefers_least_active_and_skips_unhealthy() {
        let mut upstreams = HashMap::from([
            ("a".to_string(), UpstreamState::new("a".to_string())),
            ("b".to_string(), UpstreamState::new("b".to_string())),
            ("c".to_string(), UpstreamState::new("c".to_string())),
        ]);

        upstreams.get_mut("a").unwrap().active_streams = 10;
        upstreams.get_mut("b").unwrap().active_streams = 3;
        upstreams.get_mut("c").unwrap().active_streams = 1;
        assert_eq!(select_upstream(&mut upstreams).unwrap(), "c");

        upstreams.get_mut("c").unwrap().mark_unhealthy();
        assert_eq!(select_upstream(&mut upstreams).unwrap(), "b");

        upstreams.get_mut("b").unwrap().mark_unhealthy();
        upstreams.get_mut("a").unwrap().mark_unhealthy();
        assert!(select_upstream(&mut upstreams).is_none());
    }

    #[test]
    fn upstream_stream_id_allocates_monotonically_and_tracks_active() {
        let mut up = UpstreamState::new("x".to_string());
        assert_eq!(up.active_streams, 0);
        up.on_stream_start();
        up.on_stream_start();
        assert_eq!(up.active_streams, 2);
        up.on_stream_end();
        assert_eq!(up.active_streams, 1);
    }
}
