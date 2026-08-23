//! Resource HTTP handlers for SPEAR Metadata Server
//! SPEAR元数据服务器的资源HTTP处理器
//!
//! This module contains HTTP handlers for node resource management operations
//! 此模块包含节点资源管理操作的HTTP处理器

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{info, warn};

use crate::proto::sms::{
    GetNodeResourceRequest, GetNodeWithResourceRequest, ListNodeResourcesRequest, NodeResource,
    UpdateNodeResourceRequest,
};
use crate::sms::gateway::GatewayState;
use crate::sms::node_api::{
    node_resource_to_response, node_with_resource_to_response, NodeResourceResponse,
    PublicNodeActionResponse, PublicNodeResourceListResponse, PublicNodeWithResourceResponse,
};

/// Node resource update request for HTTP API / HTTP API的节点资源更新请求
#[derive(Debug, Serialize, Deserialize)]
pub struct HttpUpdateNodeResourceRequest {
    pub cpu_usage_percent: Option<f32>,
    pub memory_usage_percent: Option<f32>,
    pub total_memory_bytes: Option<u64>,
    pub used_memory_bytes: Option<u64>,
    pub available_memory_bytes: Option<u64>,
    pub disk_usage_percent: Option<f32>,
    pub total_disk_bytes: Option<u64>,
    pub used_disk_bytes: Option<u64>,
    pub network_rx_bytes_per_sec: Option<u64>,
    pub network_tx_bytes_per_sec: Option<u64>,
    pub load_average_1m: Option<f32>,
    pub load_average_5m: Option<f32>,
    pub load_average_15m: Option<f32>,
    pub resource_metadata: Option<HashMap<String, String>>,
}

/// Query parameters for listing node resources / 列出节点资源的查询参数
#[derive(Debug, Deserialize)]
pub struct ListNodeResourcesQuery {
    pub node_uuids: Option<String>, // Comma-separated UUIDs / 逗号分隔的UUID
}

/// Update node resource information / 更新节点资源信息
pub async fn update_node_resource(
    State(state): State<GatewayState>,
    Path(uuid): Path<String>,
    Json(req): Json<HttpUpdateNodeResourceRequest>,
) -> Result<Json<PublicNodeActionResponse>, StatusCode> {
    let mut client = state.node_client.clone();

    // Convert HTTP request to gRPC request / 将HTTP请求转换为gRPC请求
    let resource = NodeResource {
        node_uuid: uuid.clone(),
        cpu_usage_percent: req.cpu_usage_percent.unwrap_or(0.0) as f64,
        memory_usage_percent: req.memory_usage_percent.unwrap_or(0.0) as f64,
        total_memory_bytes: req.total_memory_bytes.unwrap_or(0) as i64,
        used_memory_bytes: req.used_memory_bytes.unwrap_or(0) as i64,
        available_memory_bytes: req.available_memory_bytes.unwrap_or(0) as i64,
        disk_usage_percent: req.disk_usage_percent.unwrap_or(0.0) as f64,
        total_disk_bytes: req.total_disk_bytes.unwrap_or(0) as i64,
        used_disk_bytes: req.used_disk_bytes.unwrap_or(0) as i64,
        network_rx_bytes_per_sec: req.network_rx_bytes_per_sec.unwrap_or(0) as i64,
        network_tx_bytes_per_sec: req.network_tx_bytes_per_sec.unwrap_or(0) as i64,
        load_average_1m: req.load_average_1m.unwrap_or(0.0) as f64,
        load_average_5m: req.load_average_5m.unwrap_or(0.0) as f64,
        load_average_15m: req.load_average_15m.unwrap_or(0.0) as f64,
        resource_metadata: req.resource_metadata.unwrap_or_default(),
        updated_at: chrono::Utc::now().timestamp(),
    };

    let grpc_req = UpdateNodeResourceRequest {
        resource: Some(resource),
    };

    match client.update_node_resource(grpc_req).await {
        Ok(response) => {
            let resp = response.into_inner();
            Ok(Json(PublicNodeActionResponse {
                success: resp.success,
                message: resp.message,
            }))
        }
        Err(e) => {
            warn!("Failed to update node resource: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get node resource information / 获取节点资源信息
pub async fn get_node_resource(
    State(state): State<GatewayState>,
    Path(uuid): Path<String>,
) -> Result<Json<NodeResourceResponse>, StatusCode> {
    let mut client = state.node_client.clone();

    let grpc_req = GetNodeResourceRequest { node_uuid: uuid };

    match client.get_node_resource(grpc_req).await {
        Ok(response) => {
            let resp = response.into_inner();
            if let Some(resource) = resp.resource {
                Ok(Json(node_resource_to_response(resource)))
            } else {
                Err(StatusCode::NOT_FOUND)
            }
        }
        Err(e) => {
            warn!("Failed to get node resource: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// List node resources / 列出节点资源
pub async fn list_node_resources(
    State(state): State<GatewayState>,
    Query(query): Query<ListNodeResourcesQuery>,
) -> Result<Json<PublicNodeResourceListResponse>, StatusCode> {
    let mut client = state.node_client.clone();

    let node_uuids = if let Some(uuids_str) = &query.node_uuids {
        info!(
            "HTTP list_node_resources called with node_uuids: '{}'",
            uuids_str
        );
        uuids_str
            .split(',')
            .map(|s| s.trim().to_string())
            .collect::<Vec<_>>()
    } else {
        info!("HTTP list_node_resources called with no node_uuids filter");
        Vec::<String>::new()
    };

    let grpc_req = ListNodeResourcesRequest { node_uuids };

    match client.list_node_resources(grpc_req).await {
        Ok(response) => {
            let resp = response.into_inner();
            let resources = resp
                .resources
                .into_iter()
                .map(node_resource_to_response)
                .collect();

            Ok(Json(PublicNodeResourceListResponse { resources }))
        }
        Err(e) => {
            warn!("Failed to list node resources: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get node with its resource information / 获取节点及其资源信息
pub async fn get_node_with_resource(
    State(state): State<GatewayState>,
    Path(uuid): Path<String>,
) -> Result<Json<PublicNodeWithResourceResponse>, StatusCode> {
    let mut client = state.node_client.clone();

    let grpc_req = GetNodeWithResourceRequest { uuid };

    match client.get_node_with_resource(grpc_req).await {
        Ok(response) => {
            let resp = response.into_inner();
            if let Some(node) = resp.node {
                Ok(Json(node_with_resource_to_response(node, resp.resource)))
            } else {
                Err(StatusCode::NOT_FOUND)
            }
        }
        Err(e) => {
            warn!("Failed to get node with resource: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
