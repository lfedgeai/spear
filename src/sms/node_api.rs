//! Shared node and resource API mapping helpers for SMS.
//! SMS 的节点与资源 API 共享映射辅助模块。
//!
//! This module keeps node/resource presentation rules in one place so public
//! handlers and admin endpoints do not duplicate field projection logic.
//! 此模块将节点/资源展示规则集中到一个位置，避免 public handler 与 admin 入口重复维护字段投影逻辑。

use serde::Serialize;
use std::collections::HashMap;

use crate::proto::sms::{Node, NodeResource};

#[derive(Debug, Clone, Serialize)]
pub struct NodeResponse {
    pub uuid: String,
    pub ip_address: String,
    pub port: i32,
    pub http_port: i32,
    pub status: String,
    pub last_heartbeat: i64,
    pub registered_at: i64,
    pub metadata: HashMap<String, String>,
}

pub(crate) fn node_to_response(node: Node) -> NodeResponse {
    NodeResponse {
        uuid: node.uuid,
        ip_address: node.ip_address,
        port: node.port,
        http_port: node.http_port,
        status: node.status,
        last_heartbeat: node.last_heartbeat,
        registered_at: node.registered_at,
        metadata: node.metadata,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeResourceResponse {
    pub node_uuid: String,
    pub cpu_usage_percent: f64,
    pub memory_usage_percent: f64,
    pub total_memory_bytes: i64,
    pub used_memory_bytes: i64,
    pub available_memory_bytes: i64,
    pub disk_usage_percent: f64,
    pub total_disk_bytes: i64,
    pub used_disk_bytes: i64,
    pub network_rx_bytes_per_sec: i64,
    pub network_tx_bytes_per_sec: i64,
    pub load_average_1m: f64,
    pub load_average_5m: f64,
    pub load_average_15m: f64,
    pub resource_metadata: HashMap<String, String>,
    pub updated_at: i64,
}

pub(crate) fn node_resource_to_response(resource: NodeResource) -> NodeResourceResponse {
    NodeResourceResponse {
        node_uuid: resource.node_uuid,
        cpu_usage_percent: resource.cpu_usage_percent,
        memory_usage_percent: resource.memory_usage_percent,
        total_memory_bytes: resource.total_memory_bytes,
        used_memory_bytes: resource.used_memory_bytes,
        available_memory_bytes: resource.available_memory_bytes,
        disk_usage_percent: resource.disk_usage_percent,
        total_disk_bytes: resource.total_disk_bytes,
        used_disk_bytes: resource.used_disk_bytes,
        network_rx_bytes_per_sec: resource.network_rx_bytes_per_sec,
        network_tx_bytes_per_sec: resource.network_tx_bytes_per_sec,
        load_average_1m: resource.load_average_1m,
        load_average_5m: resource.load_average_5m,
        load_average_15m: resource.load_average_15m,
        resource_metadata: resource.resource_metadata,
        updated_at: resource.updated_at,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PublicNodeRegistrationResponse {
    pub success: bool,
    pub message: String,
    pub node_uuid: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublicNodeActionResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublicNodeListResponse {
    pub success: bool,
    pub nodes: Vec<NodeResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublicNodeEnvelope {
    pub success: bool,
    pub node: NodeResponse,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublicNodeResourceListResponse {
    pub resources: Vec<NodeResourceResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublicNodeWithResourceResponse {
    pub uuid: String,
    pub ip_address: String,
    pub port: i32,
    pub status: String,
    pub metadata: HashMap<String, String>,
    pub registered_at: i64,
    pub last_heartbeat: i64,
    pub resource: Option<NodeResourceResponse>,
}

pub(crate) fn node_with_resource_to_response(
    node: Node,
    resource: Option<NodeResource>,
) -> PublicNodeWithResourceResponse {
    PublicNodeWithResourceResponse {
        uuid: node.uuid,
        ip_address: node.ip_address,
        port: node.port,
        status: node.status,
        metadata: node.metadata,
        registered_at: node.registered_at,
        last_heartbeat: node.last_heartbeat,
        resource: resource.map(node_resource_to_response),
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminNodeListItem {
    pub(crate) uuid: String,
    pub(crate) name: String,
    pub(crate) ip_address: String,
    pub(crate) port: i32,
    pub(crate) status: String,
    pub(crate) last_heartbeat: i64,
    pub(crate) registered_at: i64,
    pub(crate) metadata: HashMap<String, String>,
}

pub(crate) fn node_to_admin_list_item(node: Node) -> AdminNodeListItem {
    let name = node.metadata.get("name").cloned().unwrap_or_default();
    AdminNodeListItem {
        uuid: node.uuid,
        name,
        ip_address: node.ip_address,
        port: node.port,
        status: node.status,
        last_heartbeat: node.last_heartbeat,
        registered_at: node.registered_at,
        metadata: node.metadata,
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminNodeListResponse {
    pub(crate) nodes: Vec<AdminNodeListItem>,
    pub(crate) total_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminNodeDetailEnvelope {
    pub(crate) found: bool,
    pub(crate) node: Option<NodeResponse>,
    pub(crate) resource: Option<NodeResourceResponse>,
}

pub(crate) fn admin_node_detail_response(
    node: Option<Node>,
    resource: Option<NodeResource>,
) -> AdminNodeDetailEnvelope {
    AdminNodeDetailEnvelope {
        found: node.is_some(),
        node: node.map(node_to_response),
        resource: resource.map(node_resource_to_response),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_node() -> Node {
        Node {
            uuid: "node-1".to_string(),
            ip_address: "127.0.0.1".to_string(),
            port: 9000,
            http_port: 8081,
            status: "online".to_string(),
            last_heartbeat: 20,
            registered_at: 10,
            metadata: HashMap::from([("name".to_string(), "alpha".to_string())]),
        }
    }

    fn sample_resource() -> NodeResource {
        NodeResource {
            node_uuid: "node-1".to_string(),
            cpu_usage_percent: 10.0,
            memory_usage_percent: 20.0,
            total_memory_bytes: 100,
            used_memory_bytes: 50,
            available_memory_bytes: 50,
            disk_usage_percent: 30.0,
            total_disk_bytes: 200,
            used_disk_bytes: 60,
            network_rx_bytes_per_sec: 1,
            network_tx_bytes_per_sec: 2,
            load_average_1m: 0.1,
            load_average_5m: 0.2,
            load_average_15m: 0.3,
            resource_metadata: HashMap::from([("gpu".to_string(), "A10".to_string())]),
            updated_at: 30,
        }
    }

    #[test]
    fn node_response_keeps_public_fields() {
        let response = node_to_response(sample_node());
        assert_eq!(response.uuid, "node-1");
        assert_eq!(response.http_port, 8081);
        assert_eq!(
            response.metadata.get("name").map(String::as_str),
            Some("alpha")
        );
    }

    #[test]
    fn node_with_resource_response_nests_resource() {
        let response = node_with_resource_to_response(sample_node(), Some(sample_resource()));
        assert_eq!(response.uuid, "node-1");
        assert_eq!(
            response
                .resource
                .as_ref()
                .and_then(|resource| resource.resource_metadata.get("gpu"))
                .map(String::as_str),
            Some("A10")
        );
    }

    #[test]
    fn admin_list_item_derives_name_from_metadata() {
        let item = node_to_admin_list_item(sample_node());
        assert_eq!(item.name, "alpha");
        assert_eq!(item.ip_address, "127.0.0.1");
    }
}
