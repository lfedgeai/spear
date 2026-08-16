//! Test utilities for SPEAR Metadata Server service / SPEAR元数据服务器服务测试工具
//!
//! This module provides common test utilities, data generators, and helper functions
//! for testing various components of the SPEAR Metadata Server service.
//!
//! 此模块为SPEAR元数据服务器服务的各种组件测试提供通用测试工具、数据生成器和辅助函数。

use chrono::{DateTime, Utc};
use std::collections::HashMap;
use uuid::Uuid;

use crate::proto::sms::Node;
use crate::sms::config::SmsConfig;
use crate::sms::services::node_service::{NodeService, NodeStatus};
use crate::sms::services::resource_service::NodeResourceInfo;

/// Test data generator for creating sample nodes / 创建示例节点的测试数据生成器
pub struct TestDataGenerator;

impl TestDataGenerator {
    /// Create a sample node for testing / 创建测试用的示例节点
    pub fn create_sample_node() -> Node {
        Node {
            uuid: Uuid::new_v4().to_string(),
            ip_address: "127.0.0.1".to_string(),
            port: 8080,
            http_port: 0,
            status: "online".to_string(),
            last_heartbeat: Utc::now().timestamp(),
            registered_at: Utc::now().timestamp(),
            metadata: HashMap::new(),
        }
    }

    /// Create a node with specific address / 创建具有特定地址的节点
    pub fn create_node_with_address(ip: &str, port: u16) -> Node {
        Node {
            uuid: Uuid::new_v4().to_string(),
            ip_address: ip.to_string(),
            port: port as i32,
            http_port: 0,
            status: "online".to_string(),
            last_heartbeat: Utc::now().timestamp(),
            registered_at: Utc::now().timestamp(),
            metadata: HashMap::new(),
        }
    }

    /// Create multiple sample nodes / 创建多个示例节点
    pub fn create_sample_nodes(count: usize) -> Vec<Node> {
        (0..count)
            .map(|i| Node {
                uuid: Uuid::new_v4().to_string(),
                ip_address: format!("127.0.0.{}", i + 1),
                port: 8080 + i as i32,
                http_port: 0,
                status: "online".to_string(),
                last_heartbeat: Utc::now().timestamp(),
                registered_at: Utc::now().timestamp(),
                metadata: HashMap::new(),
            })
            .collect()
    }

    /// Create a node with specific status / 创建具有特定状态的节点
    pub fn create_node_with_status(status: NodeStatus) -> Node {
        let status_str = match status {
            NodeStatus::Online => "online",
            NodeStatus::Offline => "offline",
            NodeStatus::Maintenance => "maintenance",
        };

        Node {
            uuid: Uuid::new_v4().to_string(),
            ip_address: "127.0.0.1".to_string(),
            port: 8080,
            http_port: 0,
            status: status_str.to_string(),
            last_heartbeat: Utc::now().timestamp(),
            registered_at: Utc::now().timestamp(),
            metadata: HashMap::new(),
        }
    }

    /// Create an unhealthy node (old heartbeat) / 创建不健康的节点（旧心跳）
    pub fn create_unhealthy_node(age_seconds: i64) -> Node {
        Node {
            uuid: Uuid::new_v4().to_string(),
            ip_address: "127.0.0.1".to_string(),
            port: 8080,
            http_port: 0,
            status: "offline".to_string(),
            last_heartbeat: (Utc::now() - chrono::Duration::seconds(age_seconds)).timestamp(),
            registered_at: Utc::now().timestamp(),
            metadata: HashMap::new(),
        }
    }

    /// Create a node with metadata / 创建带有元数据的节点
    pub fn create_node_with_metadata(metadata: HashMap<String, String>) -> Node {
        Node {
            uuid: Uuid::new_v4().to_string(),
            ip_address: "127.0.0.1".to_string(),
            port: 8080,
            http_port: 0,
            status: "online".to_string(),
            last_heartbeat: Utc::now().timestamp(),
            registered_at: Utc::now().timestamp(),
            metadata,
        }
    }

    /// Create sample resource info / 创建示例资源信息
    pub fn create_sample_resource(node_uuid: Uuid) -> NodeResourceInfo {
        NodeResourceInfo::new(node_uuid)
    }

    /// Create resource info with custom values / 创建具有自定义值的资源信息
    pub fn create_resource_with_usage(
        node_uuid: Uuid,
        cpu_percent: f64,
        memory_percent: f64,
        disk_percent: f64,
    ) -> NodeResourceInfo {
        let mut resource = NodeResourceInfo::new(node_uuid);
        resource.cpu_usage_percent = cpu_percent;
        resource.memory_usage_percent = memory_percent;
        resource.disk_usage_percent = disk_percent;
        resource
    }

    /// Create high-load resource info / 创建高负载资源信息
    pub fn create_high_load_resource(node_uuid: Uuid) -> NodeResourceInfo {
        Self::create_resource_with_usage(node_uuid, 85.0, 90.0, 95.0)
    }

    /// Create test configuration / 创建测试配置
    pub fn create_test_config() -> SmsConfig {
        use crate::config::base::{LogConfig, ServerConfig};
        use crate::sms::config::DatabaseConfig;

        SmsConfig {
            grpc: ServerConfig {
                addr: "127.0.0.1:50051".parse().unwrap(),
                ..Default::default()
            },
            http: ServerConfig {
                addr: "127.0.0.1:8080".parse().unwrap(),
                ..Default::default()
            },
            logging: LogConfig::default(),
            enable_swagger: true,
            database: DatabaseConfig {
                db_type: "memory".to_string(),
                path: "./test_data".to_string(),
                pool_size: Some(5),
            },
            enable_web_admin: false,
            enable_console: true,
            web_admin: ServerConfig {
                addr: "127.0.0.1:8081".parse().unwrap(),
                ..Default::default()
            },
            heartbeat_timeout: 90,
            cleanup_interval: 30,
            assignment_reconcile_interval: 60,
            max_upload_bytes: 64 * 1024 * 1024,
            files_dir: "./test_data/files".to_string(),
            execution_logs_dir: "./test_data/files/execution_logs".to_string(),
            event_kv: None,
            mcp: crate::sms::config::McpConfig::default(),
        }
    }

    /// Create test configuration with custom values / 创建具有自定义值的测试配置
    pub fn create_config_with_values(
        grpc_addr: &str,
        http_addr: &str,
        _heartbeat_timeout: u64,
    ) -> SmsConfig {
        use crate::config::base::{LogConfig, ServerConfig};
        use crate::sms::config::DatabaseConfig;

        SmsConfig {
            grpc: ServerConfig {
                addr: grpc_addr.parse().unwrap(),
                ..Default::default()
            },
            http: ServerConfig {
                addr: http_addr.parse().unwrap(),
                ..Default::default()
            },
            logging: LogConfig::default(),
            enable_swagger: false,
            database: DatabaseConfig {
                db_type: "memory".to_string(),
                path: "./test_data".to_string(),
                pool_size: Some(5),
            },
            enable_web_admin: false,
            enable_console: true,
            web_admin: ServerConfig {
                addr: "127.0.0.1:8081".parse().unwrap(),
                ..Default::default()
            },
            heartbeat_timeout: _heartbeat_timeout,
            cleanup_interval: 30,
            assignment_reconcile_interval: 60,
            max_upload_bytes: 64 * 1024 * 1024,
            files_dir: "./test_data/files".to_string(),
            execution_logs_dir: "./test_data/files/execution_logs".to_string(),
            event_kv: None,
            mcp: crate::sms::config::McpConfig::default(),
        }
    }
}

/// Test helper functions / 测试辅助函数
pub struct TestHelpers;

impl TestHelpers {
    /// Setup a test handler with sample nodes / 设置包含示例节点的测试处理器
    pub async fn setup_test_registry() -> NodeService {
        let mut service = NodeService::new();
        let nodes = TestDataGenerator::create_sample_nodes(3);

        for node in nodes {
            service.register_node(node).await.unwrap();
        }

        service
    }

    /// Setup a service with nodes and resources / 设置包含节点和资源的服务
    /// Note: NodeService doesn't handle resources directly, this just returns a service with nodes
    pub async fn setup_registry_with_resources() -> NodeService {
        Self::setup_test_registry().await
    }

    /// Setup a service with mixed health nodes / 设置包含混合健康状态节点的服务
    /// Note: NodeService doesn't have update_node method, this just returns a service with nodes
    pub async fn setup_mixed_health_registry() -> NodeService {
        Self::setup_test_registry().await
    }

    /// Assert that two timestamps are close (within tolerance) / 断言两个时间戳接近（在容差范围内）
    pub fn assert_timestamps_close(
        actual: DateTime<Utc>,
        expected: DateTime<Utc>,
        tolerance_seconds: i64,
    ) {
        let diff = (actual - expected).num_seconds().abs();
        assert!(
            diff <= tolerance_seconds,
            "Timestamps differ by {} seconds, expected within {} seconds",
            diff,
            tolerance_seconds
        );
    }

    /// Assert that a timestamp is recent (within last few seconds) / 断言时间戳是最近的（在最近几秒内）
    pub fn assert_timestamp_recent(timestamp: DateTime<Utc>, tolerance_seconds: i64) {
        Self::assert_timestamps_close(timestamp, Utc::now(), tolerance_seconds);
    }

    /// Create a temporary test file with content / 创建包含内容的临时测试文件
    pub fn create_temp_config_file(content: &str) -> tempfile::NamedTempFile {
        use std::io::Write;
        let mut temp_file = tempfile::NamedTempFile::new().unwrap();
        temp_file.write_all(content.as_bytes()).unwrap();
        temp_file.flush().unwrap();
        temp_file
    }

    /// Generate random UUID for testing / 生成用于测试的随机UUID
    pub fn generate_test_uuid() -> Uuid {
        Uuid::new_v4()
    }

    /// Create test metadata map / 创建测试元数据映射
    pub fn create_test_metadata() -> HashMap<String, String> {
        let mut metadata = HashMap::new();
        metadata.insert("environment".to_string(), "test".to_string());
        metadata.insert("version".to_string(), "1.0.0".to_string());
        metadata.insert("region".to_string(), "us-west-1".to_string());
        metadata
    }

    /// Create test health info map / 创建测试健康信息映射
    pub fn create_test_health_info() -> HashMap<String, String> {
        let mut health_info = HashMap::new();
        health_info.insert("status".to_string(), "healthy".to_string());
        health_info.insert("uptime".to_string(), "3600".to_string());
        health_info.insert("load".to_string(), "0.5".to_string());
        health_info
    }
}

/// Constants for testing / 测试常量
pub mod test_constants {
    /// Default test timeout in seconds / 默认测试超时时间（秒）
    pub const DEFAULT_TIMEOUT: u64 = 60;

    /// Test gRPC address / 测试gRPC地址
    pub const TEST_GRPC_ADDR: &str = "127.0.0.1:50051";

    /// Test HTTP address / 测试HTTP地址
    pub const TEST_HTTP_ADDR: &str = "127.0.0.1:8080";

    /// Test node IP addresses / 测试节点IP地址
    pub const TEST_NODE_IPS: &[&str] = &["127.0.0.1", "127.0.0.2", "127.0.0.3"];

    /// Test node ports / 测试节点端口
    pub const TEST_NODE_PORTS: &[u16] = &[8080, 8081, 8082];

    /// High load thresholds / 高负载阈值
    pub const HIGH_CPU_THRESHOLD: f64 = 80.0;
    pub const HIGH_MEMORY_THRESHOLD: f64 = 85.0;
    pub const HIGH_DISK_THRESHOLD: f64 = 90.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_generator_create_sample_node() {
        // Test creating a sample node / 测试创建示例节点
        let node = TestDataGenerator::create_sample_node();
        assert_eq!(node.ip_address, "127.0.0.1");
        assert_eq!(node.port, 8080);
        assert_eq!(node.status, "online");
    }

    #[test]
    fn test_data_generator_create_multiple_nodes() {
        // Test creating multiple nodes / 测试创建多个节点
        let nodes = TestDataGenerator::create_sample_nodes(3);
        assert_eq!(nodes.len(), 3);

        for (i, node) in nodes.iter().enumerate() {
            assert_eq!(node.ip_address, format!("127.0.0.{}", i + 1));
            assert_eq!(node.port, 8080 + i as i32);
        }
    }

    #[test]
    fn test_data_generator_create_unhealthy_node() {
        // Test creating an unhealthy node / 测试创建不健康节点
        let node = TestDataGenerator::create_unhealthy_node(120);
        assert_eq!(node.status, "offline");
    }

    #[test]
    fn test_data_generator_create_high_load_resource() {
        // Test creating high load resource / 测试创建高负载资源
        let uuid = TestHelpers::generate_test_uuid();
        let resource = TestDataGenerator::create_high_load_resource(uuid);
        assert_eq!(resource.node_uuid, uuid);
        assert!(resource.cpu_usage_percent > 0.0);
    }

    #[tokio::test]
    async fn test_helpers_setup_test_registry() {
        // Test setting up a test registry / 测试设置测试注册表
        let registry = TestHelpers::setup_test_registry().await;
        assert_eq!(registry.node_count().await.unwrap(), 3);
        // Verify we have nodes / 验证我们有节点
        let nodes = registry.list_nodes().await.unwrap();
        assert!(!nodes.is_empty());
    }

    #[tokio::test]
    async fn test_helpers_setup_registry_with_resources() {
        // Test setting up registry with resources / 测试设置包含资源的注册表
        let registry = TestHelpers::setup_registry_with_resources().await;
        assert_eq!(registry.node_count().await.unwrap(), 3);
        // Note: NodeService doesn't handle resources directly
    }

    #[tokio::test]
    async fn test_helpers_setup_mixed_health_registry() {
        // Test setting up mixed health registry / 测试设置混合健康状态注册表
        let registry = TestHelpers::setup_mixed_health_registry().await;
        assert_eq!(registry.node_count().await.unwrap(), 3);

        // Just verify we have nodes / 只验证我们有节点
        let all_nodes = registry.list_nodes().await.unwrap();
        assert_eq!(all_nodes.len(), 3);
    }

    #[test]
    fn test_helpers_timestamp_assertions() {
        // Test timestamp assertion helpers / 测试时间戳断言辅助函数
        let now = Utc::now();
        let recent = now - chrono::Duration::seconds(5);

        TestHelpers::assert_timestamps_close(now, recent, 10);
        TestHelpers::assert_timestamp_recent(now, 5);
    }

    #[test]
    fn test_create_test_metadata() {
        // Test creating test metadata / 测试创建测试元数据
        let metadata = TestHelpers::create_test_metadata();
        assert!(metadata.contains_key("environment"));
        assert!(metadata.contains_key("version"));
        assert!(metadata.contains_key("region"));
    }

    #[test]
    fn test_create_test_health_info() {
        // Test creating test health info / 测试创建测试健康信息
        let health_info = TestHelpers::create_test_health_info();
        assert!(health_info.contains_key("status"));
        assert!(health_info.contains_key("uptime"));
        assert!(health_info.contains_key("load"));
    }
}
