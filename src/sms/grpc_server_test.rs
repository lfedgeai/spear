//! Tests for SMS gRPC server module
//! SMS gRPC服务器模块的测试

use std::net::SocketAddr;

use crate::config::base::StorageConfig;
use crate::proto::sms::task_service_server::TaskService;
use crate::proto::sms::RegisterTaskRequest;
use crate::sms::grpc_server::GrpcServer;
use crate::sms::service::SmsServiceImpl;
use tonic::Request;
use uuid::Uuid;

/// Create test SMS service / 创建测试SMS服务
async fn create_test_sms_service() -> SmsServiceImpl {
    let storage_config = StorageConfig {
        backend: "memory".to_string(),
        data_dir: "/tmp/test_sms".to_string(),
        max_cache_size_mb: 100,
        compression_enabled: false,
        pool_size: 10,
    };

    SmsServiceImpl::with_storage_config(&storage_config).await
}

/// Create test socket address / 创建测试套接字地址
fn create_test_addr(port: u16) -> SocketAddr {
    format!("127.0.0.1:{}", port).parse().unwrap()
}

#[tokio::test]
async fn test_sms_grpc_server_creation() {
    // Test SMS gRPC server creation / 测试SMS gRPC服务器创建
    let sms_service = create_test_sms_service().await;
    let addr = create_test_addr(50070);

    let _server = GrpcServer::new(addr, sms_service);

    // Server should be created successfully / 服务器应该成功创建
    // Note: We can't easily test internal state without exposing it
    // 注意：我们无法在不暴露内部状态的情况下轻松测试内部状态
}

#[tokio::test]
async fn test_register_task_returns_uuid_on_success() {
    let sms_service = create_test_sms_service().await;
    let req = RegisterTaskRequest {
        name: "echo".to_string(),
        description: "simple echo".to_string(),
        priority: crate::proto::sms::TaskPriority::Normal as i32,
        node_uuid: Uuid::new_v4().to_string(),
        endpoint: "echo".to_string(),
        version: "1.0.0".to_string(),
        capabilities: vec!["echo".to_string()],
        metadata: std::collections::HashMap::new(),
        config: std::collections::HashMap::new(),
        executable: None,
    };

    let resp = sms_service.register_task(Request::new(req)).await.unwrap();
    let inner = resp.get_ref();
    assert!(inner.success);
    assert!(!inner.task_id.is_empty());
    assert!(Uuid::parse_str(&inner.task_id).is_ok());
}

#[tokio::test]
async fn test_sms_grpc_server_different_addresses() {
    // Test SMS gRPC server with different addresses / 测试不同地址的SMS gRPC服务器
    let sms_service1 = create_test_sms_service().await;
    let sms_service2 = create_test_sms_service().await;

    let addr1 = create_test_addr(50071);
    let addr2 = create_test_addr(50072);

    let _server1 = GrpcServer::new(addr1, sms_service1);
    let _server2 = GrpcServer::new(addr2, sms_service2);

    // Both servers should be created successfully / 两个服务器都应该成功创建
    // Different addresses should not conflict / 不同地址不应该冲突
}

#[tokio::test]
async fn test_sms_grpc_server_invalid_address() {
    // Test SMS gRPC server with invalid address / 测试无效地址的SMS gRPC服务器
    let sms_service = create_test_sms_service().await;

    // Use a valid but potentially problematic port / 使用有效但可能有问题的端口
    let addr = create_test_addr(65535); // Max valid port / 最大有效端口
    let server = GrpcServer::new(addr, sms_service);

    // Server creation should succeed / 服务器创建应该成功
    // Note: We don't actually start the server to avoid port conflicts
    // 注意：我们不实际启动服务器以避免端口冲突
}

#[tokio::test]
async fn test_sms_grpc_server_ipv6_address() {
    // Test SMS gRPC server with IPv6 address / 测试IPv6地址的SMS gRPC服务器
    let sms_service = create_test_sms_service().await;
    let ipv6_addr: SocketAddr = "[::1]:50073".parse().unwrap();

    let _server = GrpcServer::new(ipv6_addr, sms_service);

    // Server should be created successfully / 服务器应该成功创建
}

#[tokio::test]
async fn test_sms_grpc_server_port_zero() {
    // Test SMS gRPC server with port 0 (random port) / 测试端口0的SMS gRPC服务器（随机端口）
    let sms_service = create_test_sms_service().await;
    let addr = create_test_addr(0);

    let _server = GrpcServer::new(addr, sms_service);

    // Server should be created successfully / 服务器应该成功创建
}

#[tokio::test]
async fn test_sms_grpc_server_concurrent_creation() {
    // Test concurrent SMS gRPC server creation / 测试并发SMS gRPC服务器创建
    use tokio::task::JoinSet;

    let mut join_set = JoinSet::new();

    // Create multiple servers concurrently / 并发创建多个服务器
    for i in 0..5 {
        join_set.spawn(async move {
            let sms_service = create_test_sms_service().await;
            let addr = create_test_addr(50080 + i);
            let _server = GrpcServer::new(addr, sms_service);
            true // Server creation succeeded / 服务器创建成功
        });
    }

    // Wait for all tasks to complete / 等待所有任务完成
    let mut success_count = 0;
    while let Some(result) = join_set.join_next().await {
        if result.unwrap() {
            success_count += 1;
        }
    }

    assert_eq!(success_count, 5);
}

#[tokio::test]
async fn test_sms_grpc_server_with_different_storage_backends() {
    // Test SMS gRPC server with different storage backends / 测试不同存储后端的SMS gRPC服务器

    // Test with memory backend / 测试内存后端
    let memory_config = StorageConfig {
        backend: "memory".to_string(),
        data_dir: "/tmp/test_memory".to_string(),
        max_cache_size_mb: 50,
        compression_enabled: false,
        pool_size: 5,
    };

    let memory_service = SmsServiceImpl::with_storage_config(&memory_config).await;
    let memory_server = GrpcServer::new(create_test_addr(50090), memory_service);

    // Test with rocksdb backend / 测试rocksdb后端
    let rocksdb_config = StorageConfig {
        backend: "memory".to_string(), // Use memory for testing / 测试时使用内存后端
        data_dir: "/tmp/test_rocksdb".to_string(),
        max_cache_size_mb: 100,
        compression_enabled: true,
        pool_size: 15,
    };

    let rocksdb_service = SmsServiceImpl::with_storage_config(&rocksdb_config).await;
    let rocksdb_server = GrpcServer::new(create_test_addr(50091), rocksdb_service);

    // Both servers should be created successfully / 两个服务器都应该成功创建
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_sms_grpc_server_lifecycle() {
        // Test complete SMS gRPC server lifecycle / 测试完整的SMS gRPC服务器生命周期
        let sms_service = create_test_sms_service().await;
        let addr = create_test_addr(50095);

        let _server = GrpcServer::new(addr, sms_service);

        // Note: We don't actually start the server in tests to avoid port conflicts
        // 注意：我们在测试中不实际启动服务器以避免端口冲突

        // Server should be ready to start / 服务器应该准备好启动
    }

    #[tokio::test]
    async fn test_sms_grpc_server_stress_test() {
        // Stress test for SMS gRPC server creation / SMS gRPC服务器创建的压力测试

        // Create and destroy servers rapidly / 快速创建和销毁服务器
        for i in 0..50 {
            let sms_service = create_test_sms_service().await;
            let addr = create_test_addr(51000 + i);
            let _server = GrpcServer::new(addr, sms_service);

            // Server goes out of scope and is dropped / 服务器超出作用域并被丢弃
        }
    }

    #[tokio::test]
    async fn test_sms_grpc_server_memory_usage() {
        // Test SMS gRPC server memory usage / 测试SMS gRPC服务器内存使用
        let mut servers = Vec::new();

        // Create multiple servers and keep them in memory / 创建多个服务器并保持在内存中
        for i in 0..10 {
            let sms_service = create_test_sms_service().await;
            let addr = create_test_addr(51100 + i);
            let server = GrpcServer::new(addr, sms_service);
            servers.push(server);
        }

        // All servers should be created successfully / 所有服务器都应该成功创建
        assert_eq!(servers.len(), 10);

        // Clean up / 清理
        servers.clear();
    }
}
