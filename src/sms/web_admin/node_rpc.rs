use axum::Json;
use serde_json::json;
use tonic::transport::Channel;

use crate::sms::gateway::GatewayState;

pub(super) struct ResolvedNodeTarget {
    pub node_uuid: String,
    pub channel: Channel,
}

pub(super) struct ResolvedNodeHttpTarget {
    pub node_uuid: String,
    pub base_url: String,
}

/// Resolve one SMS node uuid into a lazy gRPC channel for Spearlet-side RPCs.
/// 将一个 SMS 节点 uuid 解析为用于 Spearlet 侧 RPC 的 lazy gRPC channel。
pub(super) async fn resolve_node_channel(
    state: &GatewayState,
    node_uuid: &str,
) -> Result<ResolvedNodeTarget, Json<serde_json::Value>> {
    use crate::proto::sms::GetNodeRequest;

    let mut node_client = state.node_client.clone();
    let node_resp = match node_client
        .get_node(GetNodeRequest {
            uuid: node_uuid.to_string(),
        })
        .await
    {
        Ok(r) => r.into_inner(),
        Err(e) => return Err(Json(json!({"success": false, "message": e.to_string()}))),
    };
    if !node_resp.found {
        return Err(Json(json!({"success": false, "message": "node not found"})));
    }
    let Some(node) = node_resp.node else {
        return Err(Json(json!({"success": false, "message": "node not found"})));
    };

    let Some(channel) = connect_lazy_node_channel(&node.ip_address, node.port) else {
        return Err(Json(json!({"success": false, "message": "invalid node url"})));
    };

    Ok(ResolvedNodeTarget {
        node_uuid: node.uuid,
        channel,
    })
}

/// Resolve one SMS node uuid into a Spearlet HTTP base URL.
/// 将一个 SMS 节点 uuid 解析为 Spearlet HTTP 基础地址。
pub(super) async fn resolve_node_http_target(
    state: &GatewayState,
    node_uuid: &str,
) -> Result<ResolvedNodeHttpTarget, Json<serde_json::Value>> {
    use crate::proto::sms::GetNodeRequest;

    let mut node_client = state.node_client.clone();
    let node_resp = match node_client
        .get_node(GetNodeRequest {
            uuid: node_uuid.to_string(),
        })
        .await
    {
        Ok(r) => r.into_inner(),
        Err(e) => return Err(Json(json!({"success": false, "message": e.to_string()}))),
    };
    if !node_resp.found {
        return Err(Json(json!({"success": false, "message": "node not found"})));
    }
    let Some(node) = node_resp.node else {
        return Err(Json(json!({"success": false, "message": "node not found"})));
    };

    let http_port = if node.http_port > 0 {
        node.http_port as u16
    } else {
        node.metadata
            .get("http_port")
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(8081)
    };
    let base_url = format!("http://{}:{}", node.ip_address, http_port);
    Ok(ResolvedNodeHttpTarget {
        node_uuid: node.uuid,
        base_url,
    })
}

/// Build a lazy gRPC channel from one node endpoint.
/// 从节点 endpoint 构建 lazy gRPC channel。
pub(super) fn connect_lazy_node_channel(ip_address: &str, port: i32) -> Option<Channel> {
    let url = format!("http://{}:{}", ip_address, port);
    Channel::from_shared(url).ok().map(|ch| ch.connect_lazy())
}
