//! Stream gateway handlers / 流网关处理器
//!
//! This module provides:
//! - Stream session creation (control plane) / 创建流会话（控制面）
//! - WebSocket proxy to spearlet (data plane) / 代理 WebSocket 到 spearlet（数据面）

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
};
use base64::Engine;
use futures::{SinkExt, StreamExt};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::sync::mpsc;
use tonic::Request;

use super::common::ErrorResponse;
use crate::proto::sms::{GetExecutionRequest, GetNodeRequest};
use crate::sms::gateway::GatewayState;

const STREAM_SESSION_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, Deserialize)]
pub struct StreamSessionQuery {
    pub token: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateStreamSessionResponse {
    pub execution_id: String,
    pub token: String,
    pub ws_url: String,
    pub expires_in_ms: u64,
}

fn make_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn ws_url_from_headers(headers: &HeaderMap, execution_id: &str, token: &str) -> String {
    let host = headers
        .get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("localhost");
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("http");
    let ws_scheme = if scheme.eq_ignore_ascii_case("https") {
        "wss"
    } else {
        "ws"
    };
    format!(
        "{}://{}/api/v1/executions/{}/streams/ws?token={}",
        ws_scheme, host, execution_id, token
    )
}

/// Create a stream session / 创建流会话
pub async fn create_stream_session(
    Path(execution_id): Path<String>,
    State(state): State<GatewayState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if execution_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "EXECUTION_ID_REQUIRED".to_string(),
                message: "execution_id is required".to_string(),
            }),
        )
            .into_response();
    }

    let mut idx_client = state.execution_index_client.clone();
    let req = Request::new(GetExecutionRequest {
        execution_id: execution_id.clone(),
    });
    let resp = match idx_client.get_execution(req).await {
        Ok(r) => r.into_inner(),
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(ErrorResponse {
                    error: "EXECUTION_INDEX_ERROR".to_string(),
                    message: format!("execution_index error: {e}"),
                }),
            )
                .into_response();
        }
    };
    if !resp.found {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "EXECUTION_NOT_FOUND".to_string(),
                message: "execution not found".to_string(),
            }),
        )
            .into_response();
    }

    let token = make_token();
    state
        .stream_sessions
        .insert(token.clone(), execution_id.clone(), STREAM_SESSION_TTL);
    let ws_url = ws_url_from_headers(&headers, &execution_id, &token);

    Json(CreateStreamSessionResponse {
        execution_id,
        token,
        ws_url,
        expires_in_ms: STREAM_SESSION_TTL.as_millis() as u64,
    })
    .into_response()
}

/// WebSocket proxy to spearlet / 代理 WebSocket 到 spearlet
pub async fn stream_ws_proxy(
    Path(execution_id): Path<String>,
    State(state): State<GatewayState>,
    Query(q): Query<StreamSessionQuery>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let Some(token) = q.token else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "TOKEN_REQUIRED".to_string(),
                message: "missing token".to_string(),
            }),
        )
            .into_response();
    };
    let Some(bound_execution_id) = state.stream_sessions.validate(&token) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "TOKEN_INVALID".to_string(),
                message: "invalid token".to_string(),
            }),
        )
            .into_response();
    };
    if bound_execution_id != execution_id {
        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "TOKEN_FORBIDDEN".to_string(),
                message: "token not allowed for this execution".to_string(),
            }),
        )
            .into_response();
    }

    ws.on_upgrade(move |socket| async move {
        stream_ws_proxy_loop(state, execution_id, socket).await;
    })
}

pub(crate) async fn resolve_spearlet_ws_url(
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
    let node_uuid = n.uuid.clone();
    let ip = n.ip_address;
    if ip
        .parse::<std::net::IpAddr>()
        .map(|ip| ip.is_unspecified())
        .unwrap_or(true)
    {
        return Err(format!(
            "node has invalid ip_address (node_uuid={node_uuid}): {ip}"
        ));
    }
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

async fn stream_ws_proxy_loop(state: GatewayState, execution_id: String, socket: WebSocket) {
    let (mut client_tx, mut client_rx) = socket.split();

    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Message>();
    let writer = tokio::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            if client_tx.send(msg).await.is_err() {
                break;
            }
        }
    });

    let client_id = match state
        .execution_stream_pool
        .register_client(&state, &execution_id, out_tx.clone())
        .await
    {
        Ok(v) => v,
        Err(_) => {
            let _ = out_tx.send(Message::Close(None));
            drop(out_tx);
            let _ = writer.await;
            return;
        }
    };

    loop {
        tokio::select! {
            msg = client_rx.next() => {
                let Some(Ok(msg)) = msg else { break; };
                match msg {
                    Message::Binary(b) => {
                        if state.execution_stream_pool.forward_client_binary(&state, &execution_id, &client_id, &b).await.is_err() {
                            break;
                        }
                    }
                    Message::Text(_) => {}
                    Message::Close(_) => break,
                    Message::Ping(p) => { let _ = out_tx.send(Message::Pong(p)); }
                    _ => {}
                }
            }
        }
    }

    state
        .execution_stream_pool
        .unregister_client(&execution_id, &client_id)
        .await;
    drop(out_tx);
    let _ = writer.await;
}
