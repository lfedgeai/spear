//! Tests for SMS HTTP Handlers
//! SMS HTTP处理器测试

use serde_urlencoded;
use std::collections::HashMap;

use crate::sms::handlers::{
    health_check, DeleteTaskParams, HttpHeartbeatRequest, HttpRegisterNodeRequest,
    HttpUpdateNodeRequest, HttpUpdateNodeResourceRequest, ListNodeResourcesQuery, ListNodesQuery,
    ListTasksParams, RegisterTaskParams,
};

// Mock tests for handlers that don't require gRPC clients / 不需要gRPC客户端的处理器模拟测试

#[tokio::test]
async fn test_health_check_handler() {
    // Test health check endpoint / 测试健康检查端点
    let response = health_check().await;

    // Verify response structure / 验证响应结构
    let value = response.0;
    assert!(value.get("status").is_some());
    assert!(value.get("timestamp").is_some());
    assert!(value.get("service").is_some());

    // Verify specific values / 验证具体值
    assert_eq!(value["status"], "healthy");
    assert_eq!(value["service"], "sms");
}

#[test]
fn test_http_register_node_request_serialization() {
    // Test HTTP register node request serialization / 测试HTTP注册节点请求序列化
    let mut metadata = HashMap::new();
    metadata.insert("region".to_string(), "us-west-1".to_string());
    metadata.insert("zone".to_string(), "a".to_string());

    let request = HttpRegisterNodeRequest {
        ip_address: "192.168.1.100".to_string(),
        port: 8080,
        http_port: Some(8081),
        metadata: Some(metadata.clone()),
    };

    // Test serialization / 测试序列化
    let json_str = serde_json::to_string(&request).unwrap();
    assert!(json_str.contains("192.168.1.100"));
    assert!(json_str.contains("8080"));
    assert!(json_str.contains("us-west-1"));

    // Test deserialization / 测试反序列化
    let deserialized: HttpRegisterNodeRequest = serde_json::from_str(&json_str).unwrap();
    assert_eq!(deserialized.ip_address, "192.168.1.100");
    assert_eq!(deserialized.port, 8080);
    assert_eq!(deserialized.metadata.unwrap(), metadata);
}

#[test]
fn test_http_update_node_request_serialization() {
    // Test HTTP update node request serialization / 测试HTTP更新节点请求序列化
    let mut metadata = HashMap::new();
    metadata.insert("updated".to_string(), "true".to_string());

    let request = HttpUpdateNodeRequest {
        ip_address: Some("192.168.1.101".to_string()),
        port: Some(8081),
        http_port: Some(8082),
        status: Some("inactive".to_string()),
        metadata: Some(metadata.clone()),
    };

    // Test serialization / 测试序列化
    let json_str = serde_json::to_string(&request).unwrap();
    assert!(json_str.contains("192.168.1.101"));
    assert!(json_str.contains("8081"));
    assert!(json_str.contains("inactive"));

    // Test deserialization / 测试反序列化
    let deserialized: HttpUpdateNodeRequest = serde_json::from_str(&json_str).unwrap();
    assert_eq!(deserialized.ip_address.unwrap(), "192.168.1.101");
    assert_eq!(deserialized.port.unwrap(), 8081);
    assert_eq!(deserialized.status.unwrap(), "inactive");
    assert_eq!(deserialized.metadata.unwrap(), metadata);
}

#[test]
fn test_http_heartbeat_request_serialization() {
    // Test HTTP heartbeat request serialization / 测试HTTP心跳请求序列化
    let mut health_info = HashMap::new();
    health_info.insert("cpu_usage".to_string(), "45.2".to_string());
    health_info.insert("memory_usage".to_string(), "67.8".to_string());

    let request = HttpHeartbeatRequest {
        health_info: Some(health_info.clone()),
    };

    // Test serialization / 测试序列化
    let json_str = serde_json::to_string(&request).unwrap();
    assert!(json_str.contains("cpu_usage"));
    assert!(json_str.contains("45.2"));

    // Test deserialization / 测试反序列化
    let deserialized: HttpHeartbeatRequest = serde_json::from_str(&json_str).unwrap();
    assert_eq!(deserialized.health_info.unwrap(), health_info);
}

#[test]
fn test_list_nodes_query_deserialization() {
    // Test list nodes query deserialization / 测试列出节点查询反序列化
    let query_str = "status=active";
    let query: ListNodesQuery = serde_urlencoded::from_str(query_str).unwrap();
    assert_eq!(query.status.unwrap(), "active");

    // Test empty query / 测试空查询
    let empty_query: ListNodesQuery = serde_urlencoded::from_str("").unwrap();
    assert!(empty_query.status.is_none());
}

#[test]
fn test_http_update_node_resource_request_serialization() {
    // Test HTTP update node resource request serialization / 测试HTTP更新节点资源请求序列化
    let mut resource_metadata = HashMap::new();
    resource_metadata.insert("gpu_count".to_string(), "4".to_string());
    resource_metadata.insert("gpu_type".to_string(), "RTX4090".to_string());

    let request = HttpUpdateNodeResourceRequest {
        cpu_usage_percent: Some(75.5),
        memory_usage_percent: Some(82.3),
        total_memory_bytes: Some(16_000_000_000),
        used_memory_bytes: Some(13_168_000_000),
        available_memory_bytes: Some(2_832_000_000),
        disk_usage_percent: Some(45.2),
        total_disk_bytes: Some(1_000_000_000_000),
        used_disk_bytes: Some(452_000_000_000),
        network_rx_bytes_per_sec: Some(1_048_576),
        network_tx_bytes_per_sec: Some(524_288),
        load_average_1m: Some(2.5),
        load_average_5m: Some(2.1),
        load_average_15m: Some(1.8),
        resource_metadata: Some(resource_metadata.clone()),
    };

    // Test serialization / 测试序列化
    let json_str = serde_json::to_string(&request).unwrap();
    assert!(json_str.contains("75.5"));
    assert!(json_str.contains("RTX4090"));

    // Test deserialization / 测试反序列化
    let deserialized: HttpUpdateNodeResourceRequest = serde_json::from_str(&json_str).unwrap();
    assert_eq!(deserialized.cpu_usage_percent.unwrap(), 75.5);
    assert_eq!(deserialized.resource_metadata.unwrap(), resource_metadata);
}

#[test]
fn test_list_node_resources_query_deserialization() {
    // Test list node resources query deserialization / 测试列出节点资源查询反序列化
    let query_str = "node_uuids=uuid1,uuid2,uuid3";
    let query: ListNodeResourcesQuery = serde_urlencoded::from_str(query_str).unwrap();
    assert_eq!(query.node_uuids.unwrap(), "uuid1,uuid2,uuid3");

    // Test empty query / 测试空查询
    let empty_query: ListNodeResourcesQuery = serde_urlencoded::from_str("").unwrap();
    assert!(empty_query.node_uuids.is_none());
}

#[test]
fn test_register_task_params_serialization() {
    // Test register task params serialization / 测试注册任务参数序列化
    let mut metadata = HashMap::new();
    metadata.insert("author".to_string(), "test_user".to_string());
    metadata.insert("version".to_string(), "1.0.0".to_string());

    let mut config = HashMap::new();
    config.insert("timeout".to_string(), "30".to_string());
    config.insert("retry_count".to_string(), "3".to_string());

    let params = RegisterTaskParams {
        name: "test_task".to_string(),
        description: Some("A test task".to_string()),
        priority: Some("high".to_string()),
        desired_replicas: Some(2),
        scheduling_strategy: Some("spread".to_string()),
        endpoint: "http://localhost:8080/task".to_string(),
        version: "1.0.0".to_string(),
        capabilities: Some(vec!["cpu".to_string(), "memory".to_string()]),
        metadata: Some(metadata.clone()),
        config: Some(config.clone()),
        executable: None,
    };

    // Test serialization / 测试序列化
    let json_str = serde_json::to_string(&params).unwrap();
    assert!(json_str.contains("test_task"));
    assert!(json_str.contains("high"));
    assert!(json_str.contains("spread"));

    // Test deserialization / 测试反序列化
    let deserialized: RegisterTaskParams = serde_json::from_str(&json_str).unwrap();
    assert_eq!(deserialized.name, "test_task");
    assert_eq!(deserialized.priority.unwrap(), "high");
    assert_eq!(deserialized.metadata.unwrap(), metadata);
    assert_eq!(deserialized.config.unwrap(), config);
}

#[test]
fn test_list_tasks_params_deserialization() {
    // Test list tasks params deserialization / 测试列出任务参数反序列化
    let query_str = "status=active&priority=high&limit=10&offset=0";
    let params: ListTasksParams = serde_urlencoded::from_str(query_str).unwrap();

    assert_eq!(params.status.unwrap(), "active");
    assert_eq!(params.priority.unwrap(), "high");
    assert_eq!(params.limit.unwrap(), 10);
    assert_eq!(params.offset.unwrap(), 0);

    // Test partial query / 测试部分查询
    let partial_query_str = "status=inactive&limit=5";
    let partial_params: ListTasksParams = serde_urlencoded::from_str(partial_query_str).unwrap();
    assert_eq!(partial_params.status.unwrap(), "inactive");
    assert!(partial_params.priority.is_none());
    assert_eq!(partial_params.limit.unwrap(), 5);
    assert!(partial_params.offset.is_none());
}

#[test]
fn test_delete_task_params_serialization() {
    // Test delete task params serialization / 测试删除任务参数序列化
    let params = DeleteTaskParams {
        reason: Some("Task completed successfully".to_string()),
        force: Some(true),
    };

    // Test serialization / 测试序列化
    let json_str = serde_json::to_string(&params).unwrap();
    assert!(json_str.contains("Task completed successfully"));

    // Test deserialization / 测试反序列化
    let deserialized: DeleteTaskParams = serde_json::from_str(&json_str).unwrap();
    assert_eq!(deserialized.reason.unwrap(), "Task completed successfully");
    assert_eq!(deserialized.force, Some(true));

    // Test with no reason / 测试无原因情况
    let no_reason_params = DeleteTaskParams {
        reason: None,
        force: None,
    };
    let no_reason_json = serde_json::to_string(&no_reason_params).unwrap();
    let no_reason_deserialized: DeleteTaskParams = serde_json::from_str(&no_reason_json).unwrap();
    assert!(no_reason_deserialized.reason.is_none());
    assert!(no_reason_deserialized.force.is_none());
}

#[test]
fn test_request_validation() {
    // Test request validation for various edge cases / 测试各种边界情况的请求验证

    // Test empty node registration / 测试空节点注册
    let empty_node_request = HttpRegisterNodeRequest {
        ip_address: "".to_string(),
        port: 0,
        http_port: None,
        metadata: None,
    };
    let json_str = serde_json::to_string(&empty_node_request).unwrap();
    let deserialized: HttpRegisterNodeRequest = serde_json::from_str(&json_str).unwrap();
    assert_eq!(deserialized.ip_address, "");
    assert_eq!(deserialized.port, 0);

    // Test invalid port numbers / 测试无效端口号
    let invalid_port_request = HttpRegisterNodeRequest {
        ip_address: "127.0.0.1".to_string(),
        port: -1,
        http_port: None,
        metadata: None,
    };
    let json_str = serde_json::to_string(&invalid_port_request).unwrap();
    let deserialized: HttpRegisterNodeRequest = serde_json::from_str(&json_str).unwrap();
    assert_eq!(deserialized.port, -1);

    // Test large port numbers / 测试大端口号
    let large_port_request = HttpRegisterNodeRequest {
        ip_address: "127.0.0.1".to_string(),
        port: 65535,
        http_port: None,
        metadata: None,
    };
    let json_str = serde_json::to_string(&large_port_request).unwrap();
    let deserialized: HttpRegisterNodeRequest = serde_json::from_str(&json_str).unwrap();
    assert_eq!(deserialized.port, 65535);
}

#[test]
fn test_resource_request_edge_cases() {
    // Test resource request edge cases / 测试资源请求边界情况

    // Test with zero values / 测试零值
    let zero_resource_request = HttpUpdateNodeResourceRequest {
        cpu_usage_percent: Some(0.0),
        memory_usage_percent: Some(0.0),
        total_memory_bytes: Some(0),
        used_memory_bytes: Some(0),
        available_memory_bytes: Some(0),
        disk_usage_percent: Some(0.0),
        total_disk_bytes: Some(0),
        used_disk_bytes: Some(0),
        network_rx_bytes_per_sec: Some(0),
        network_tx_bytes_per_sec: Some(0),
        load_average_1m: Some(0.0),
        load_average_5m: Some(0.0),
        load_average_15m: Some(0.0),
        resource_metadata: None,
    };

    let json_str = serde_json::to_string(&zero_resource_request).unwrap();
    let deserialized: HttpUpdateNodeResourceRequest = serde_json::from_str(&json_str).unwrap();
    assert_eq!(deserialized.cpu_usage_percent.unwrap(), 0.0);
    assert_eq!(deserialized.total_memory_bytes.unwrap(), 0);

    // Test with maximum values / 测试最大值
    let max_resource_request = HttpUpdateNodeResourceRequest {
        cpu_usage_percent: Some(100.0),
        memory_usage_percent: Some(100.0),
        total_memory_bytes: Some(u64::MAX),
        used_memory_bytes: Some(u64::MAX),
        available_memory_bytes: Some(u64::MAX),
        disk_usage_percent: Some(100.0),
        total_disk_bytes: Some(u64::MAX),
        used_disk_bytes: Some(u64::MAX),
        network_rx_bytes_per_sec: Some(u64::MAX),
        network_tx_bytes_per_sec: Some(u64::MAX),
        load_average_1m: Some(f32::MAX),
        load_average_5m: Some(f32::MAX),
        load_average_15m: Some(f32::MAX),
        resource_metadata: None,
    };

    let json_str = serde_json::to_string(&max_resource_request).unwrap();
    let deserialized: HttpUpdateNodeResourceRequest = serde_json::from_str(&json_str).unwrap();
    assert_eq!(deserialized.cpu_usage_percent.unwrap(), 100.0);
    assert_eq!(deserialized.total_memory_bytes.unwrap(), u64::MAX);
}

#[test]
fn test_task_params_edge_cases() {
    // Test task params edge cases / 测试任务参数边界情况

    // Test with empty strings / 测试空字符串
    let empty_task_params = RegisterTaskParams {
        name: "".to_string(),
        description: Some("".to_string()),
        priority: Some("".to_string()),
        desired_replicas: Some(1),
        scheduling_strategy: Some("spread".to_string()),
        endpoint: "".to_string(),
        version: "".to_string(),
        capabilities: Some(vec![]),
        metadata: Some(HashMap::new()),
        config: Some(HashMap::new()),
        executable: None,
    };

    let json_str = serde_json::to_string(&empty_task_params).unwrap();
    let deserialized: RegisterTaskParams = serde_json::from_str(&json_str).unwrap();
    assert_eq!(deserialized.name, "");
    assert_eq!(deserialized.endpoint, "");
    assert!(deserialized.capabilities.unwrap().is_empty());

    // Test with very long strings / 测试很长的字符串
    let long_string = "a".repeat(1000);
    let long_task_params = RegisterTaskParams {
        name: long_string.clone(),
        description: Some(long_string.clone()),
        priority: Some("normal".to_string()),
        desired_replicas: Some(3),
        scheduling_strategy: Some("spread".to_string()),
        endpoint: format!("http://localhost:8080/{}", long_string),
        version: long_string.clone(),
        capabilities: Some(vec![long_string.clone()]),
        metadata: Some(HashMap::from([(long_string.clone(), long_string.clone())])),
        config: Some(HashMap::from([(long_string.clone(), long_string.clone())])),
        executable: None,
    };

    let json_str = serde_json::to_string(&long_task_params).unwrap();
    let deserialized: RegisterTaskParams = serde_json::from_str(&json_str).unwrap();
    assert_eq!(deserialized.name.len(), 1000);
    assert_eq!(deserialized.description.unwrap().len(), 1000);
}

// Integration tests for handler behavior / 处理器行为的集成测试
#[cfg(test)]
mod integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_health_check_response_format() {
        // Test health check response format / 测试健康检查响应格式
        let response = health_check().await;
        let value = response.0;

        // Verify all required fields are present / 验证所有必需字段都存在
        assert!(value.get("status").is_some());
        assert!(value.get("timestamp").is_some());
        assert!(value.get("service").is_some());

        // Verify field types / 验证字段类型
        assert!(value["status"].is_string());
        assert!(value["timestamp"].is_string());
        assert!(value["service"].is_string());

        // Verify timestamp format (should be RFC3339) / 验证时间戳格式（应为RFC3339）
        let timestamp_str = value["timestamp"].as_str().unwrap();
        assert!(chrono::DateTime::parse_from_rfc3339(timestamp_str).is_ok());
    }

    #[test]
    fn test_concurrent_serialization() {
        // Test concurrent serialization of requests / 测试请求的并发序列化
        use std::sync::Arc;
        use std::thread;

        let request = Arc::new(HttpRegisterNodeRequest {
            ip_address: "192.168.1.100".to_string(),
            port: 8080,
            http_port: Some(8081),
            metadata: Some(HashMap::from([
                ("region".to_string(), "us-west-1".to_string()),
                ("zone".to_string(), "a".to_string()),
            ])),
        });

        let mut handles = vec![];

        for _ in 0..10 {
            let request_clone = Arc::clone(&request);
            let handle = thread::spawn(move || {
                let json_str = serde_json::to_string(&*request_clone).unwrap();
                let deserialized: HttpRegisterNodeRequest =
                    serde_json::from_str(&json_str).unwrap();
                deserialized
            });
            handles.push(handle);
        }

        for handle in handles {
            let result = handle.join().unwrap();
            assert_eq!(result.ip_address, "192.168.1.100");
            assert_eq!(result.port, 8080);
        }
    }

    #[test]
    fn test_memory_usage() {
        // Test memory usage of request structures / 测试请求结构的内存使用
        let node_request = HttpRegisterNodeRequest {
            ip_address: "192.168.1.100".to_string(),
            port: 8080,
            http_port: Some(8081),
            metadata: Some(HashMap::new()),
        };

        let resource_request = HttpUpdateNodeResourceRequest {
            cpu_usage_percent: Some(50.0),
            memory_usage_percent: Some(60.0),
            total_memory_bytes: Some(8_000_000_000),
            used_memory_bytes: Some(4_800_000_000),
            available_memory_bytes: Some(3_200_000_000),
            disk_usage_percent: Some(30.0),
            total_disk_bytes: Some(500_000_000_000),
            used_disk_bytes: Some(150_000_000_000),
            network_rx_bytes_per_sec: Some(1_048_576),
            network_tx_bytes_per_sec: Some(524_288),
            load_average_1m: Some(1.5),
            load_average_5m: Some(1.2),
            load_average_15m: Some(1.0),
            resource_metadata: Some(HashMap::new()),
        };

        // Verify structures don't use excessive memory / 验证结构不使用过多内存
        let node_size = std::mem::size_of_val(&node_request);
        let resource_size = std::mem::size_of_val(&resource_request);

        // These are reasonable upper bounds for the structures / 这些是结构的合理上限
        assert!(node_size < 1024); // Less than 1KB / 小于1KB
        assert!(resource_size < 2048); // Less than 2KB / 小于2KB
    }
}
