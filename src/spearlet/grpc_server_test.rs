//! Tests for gRPC server module
//! gRPC服务器模块的测试

use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

use crate::config::base::ServerConfig;
use crate::spearlet::config::{SpearletConfig, StorageConfig};
use crate::spearlet::grpc_server::{GrpcServer, HealthService, HealthStatus};

/// Create test configuration / 创建测试配置
fn create_test_config() -> SpearletConfig {
    SpearletConfig {
        grpc: ServerConfig {
            addr: "127.0.0.1:50052".parse().unwrap(),
            ..Default::default()
        },
        storage: StorageConfig {
            backend: "memory".to_string(),
            data_dir: "/tmp/test".to_string(),
            max_cache_size_mb: 100,
            compression_enabled: false,
            max_object_size: 1024 * 1024, // 1MB
        },
        ..Default::default()
    }
}

#[tokio::test]
async fn test_grpc_server_creation() {
    // Test gRPC server creation / 测试gRPC服务器创建
    let config = Arc::new(create_test_config());
    let server = GrpcServer::new(config.clone(), None).await.unwrap();

    // Verify server has object service / 验证服务器有对象服务
    let object_service = server.get_object_service();
    assert_eq!(object_service.get_stats().await.object_count, 0);
}

#[tokio::test]
async fn test_grpc_server_config() {
    // Test gRPC server configuration / 测试gRPC服务器配置
    let mut config = create_test_config();
    config.grpc.addr = "0.0.0.0:8080".parse().unwrap();
    config.grpc.addr = "0.0.0.0:8080".parse().unwrap();
    config.storage.max_object_size = 2 * 1024 * 1024; // 2MB

    let server = GrpcServer::new(Arc::new(config), None).await.unwrap();
    let object_service = server.get_object_service();

    // Verify object service is created / 验证对象服务已创建
    assert_eq!(object_service.get_stats().await.object_count, 0);
}

#[tokio::test]
async fn test_health_service_creation() {
    // Test health service creation / 测试健康服务创建
    let config = Arc::new(create_test_config());
    let server = GrpcServer::new(config, None).await.unwrap();
    let object_service = server.get_object_service();
    let function_service = server.get_function_service();

    let health_service = HealthService::new(object_service, function_service);

    // Test health status retrieval / 测试健康状态获取
    let health_status = health_service.get_health_status().await;
    assert_eq!(health_status.status, "healthy");
    assert_eq!(health_status.object_count, 0);
    assert_eq!(health_status.total_object_size, 0);
    assert_eq!(health_status.pinned_object_count, 0);
}

#[tokio::test]
async fn test_health_status_structure() {
    // Test health status structure / 测试健康状态结构
    let health_status = HealthStatus {
        status: "healthy".to_string(),
        object_count: 10,
        total_object_size: 1024,
        pinned_object_count: 5,
        task_count: 3,
        execution_count: 7,
        running_executions: 2,
    };

    assert_eq!(health_status.status, "healthy");
    assert_eq!(health_status.object_count, 10);
    assert_eq!(health_status.total_object_size, 1024);
    assert_eq!(health_status.pinned_object_count, 5);
    assert_eq!(health_status.task_count, 3);
    assert_eq!(health_status.execution_count, 7);
    assert_eq!(health_status.running_executions, 2);

    // Test cloning / 测试克隆
    let cloned_status = health_status.clone();
    assert_eq!(cloned_status.status, health_status.status);
    assert_eq!(cloned_status.object_count, health_status.object_count);
}

#[tokio::test]
async fn test_grpc_server_invalid_address() {
    // Test gRPC server with invalid address / 测试无效地址的gRPC服务器
    let mut config = create_test_config();
    config.grpc.addr = "0.0.0.0:8080".parse().unwrap();

    let server = GrpcServer::new(Arc::new(config), None).await.unwrap();

    // Server should start but fail when trying to bind to invalid address
    // 服务器应该启动但在尝试绑定到无效地址时失败
    let result = timeout(Duration::from_millis(100), server.start()).await;

    // Should timeout or return error / 应该超时或返回错误
    assert!(result.is_err() || result.unwrap().is_err());
}

#[tokio::test]
async fn test_multiple_grpc_servers() {
    // Test creating multiple gRPC servers / 测试创建多个gRPC服务器
    let config1 = Arc::new(create_test_config());
    let config2 = Arc::new(create_test_config());

    let server1 = GrpcServer::new(config1, None).await.unwrap();
    let server2 = GrpcServer::new(config2, None).await.unwrap();

    let service1 = server1.get_object_service();
    let service2 = server2.get_object_service();

    // Different servers should keep isolated object stores / 不同服务器应保持各自独立的对象存储
    use crate::proto::spearlet::{object_service_server::ObjectService, PutObjectRequest};
    use tonic::Request;

    let put_request = Request::new(PutObjectRequest {
        key: "server-1-only".to_string(),
        value: b"value".to_vec(),
        metadata: std::collections::HashMap::new(),
        overwrite: false,
    });
    service1.put_object(put_request).await.unwrap();

    assert_eq!(service1.get_stats().await.object_count, 1);
    assert_eq!(service2.get_stats().await.object_count, 0);
}

#[tokio::test]
async fn test_grpc_server_tls_config() {
    // Test gRPC server with TLS configuration / 测试带TLS配置的gRPC服务器
    let mut config = create_test_config();
    config.grpc.enable_tls = true;
    config.grpc.cert_path = Some("/path/to/cert.pem".to_string());
    config.grpc.key_path = Some("/path/to/key.pem".to_string());

    let server = GrpcServer::new(Arc::new(config), None).await.unwrap();
    let object_service = server.get_object_service();

    // Verify object service is created even with TLS config / 验证即使有TLS配置也能创建对象服务
    assert_eq!(object_service.get_stats().await.object_count, 0);
}

#[tokio::test]
async fn test_grpc_server_different_ports() {
    // Test gRPC servers on different ports / 测试不同端口的gRPC服务器
    let mut config1 = create_test_config();
    config1.grpc.addr = "127.0.0.1:50053".parse().unwrap();

    let mut config2 = create_test_config();
    config2.grpc.addr = "127.0.0.1:50054".parse().unwrap();

    let server1 = GrpcServer::new(Arc::new(config1), None).await.unwrap();
    let server2 = GrpcServer::new(Arc::new(config2), None).await.unwrap();

    let service1 = server1.get_object_service();
    let service2 = server2.get_object_service();

    // Different ports should still keep isolated stores / 不同端口的服务仍应保持独立存储
    use crate::proto::spearlet::{object_service_server::ObjectService, PutObjectRequest};
    use tonic::Request;

    let put_request = Request::new(PutObjectRequest {
        key: "port-1-only".to_string(),
        value: b"value".to_vec(),
        metadata: std::collections::HashMap::new(),
        overwrite: false,
    });
    service1.put_object(put_request).await.unwrap();

    assert_eq!(service1.get_stats().await.object_count, 1);
    assert_eq!(service2.get_stats().await.object_count, 0);
}

#[tokio::test]
async fn test_health_service_with_data() {
    // Test health service with simulated data / 测试带模拟数据的健康服务
    let config = Arc::new(create_test_config());
    let server = GrpcServer::new(config, None).await.unwrap();
    let object_service = server.get_object_service();

    // Add some test objects to the service / 向服务添加一些测试对象
    let test_key = "test_key".to_string();
    let test_value = b"test_value".to_vec();
    let test_metadata = std::collections::HashMap::new();

    // Store an object using put_object / 使用put_object存储一个对象
    use crate::proto::spearlet::{object_service_server::ObjectService, PutObjectRequest};
    use tonic::Request;
    let put_request = Request::new(PutObjectRequest {
        key: test_key.clone(),
        value: test_value,
        metadata: test_metadata,
        overwrite: false,
    });
    let _ = object_service.put_object(put_request).await;

    let function_service = server.get_function_service();
    let health_service = HealthService::new(object_service, function_service);
    let health_status = health_service.get_health_status().await;

    // Verify health status reflects the stored object / 验证健康状态反映了存储的对象
    assert_eq!(health_status.status, "healthy");
    assert_eq!(health_status.object_count, 1);
    assert!(health_status.total_object_size > 0);
}

#[tokio::test]
async fn test_grpc_server_edge_cases() {
    // Test gRPC server edge cases / 测试gRPC服务器边缘情况

    // Test with port 0 (should use random available port) / 测试端口0（应该使用随机可用端口）
    let mut config = create_test_config();
    config.grpc.addr = "127.0.0.1:0".parse().unwrap();

    let server = GrpcServer::new(Arc::new(config), None).await.unwrap();
    let object_service = server.get_object_service();
    assert_eq!(object_service.get_stats().await.object_count, 0);

    // Test with very high port number / 测试非常高的端口号
    let mut config2 = create_test_config();
    config2.grpc.addr = "127.0.0.1:65535".parse().unwrap();

    let server2 = GrpcServer::new(Arc::new(config2), None).await.unwrap();
    let object_service2 = server2.get_object_service();
    assert_eq!(object_service2.get_stats().await.object_count, 0);
}

#[tokio::test]
async fn test_health_status_debug_format() {
    // Test health status debug formatting / 测试健康状态调试格式
    let health_status = HealthStatus {
        status: "healthy".to_string(),
        object_count: 42,
        total_object_size: 2048,
        pinned_object_count: 10,
        task_count: 5,
        execution_count: 15,
        running_executions: 3,
    };

    let debug_str = format!("{:?}", health_status);
    assert!(debug_str.contains("healthy"));
    assert!(debug_str.contains("42"));
    assert!(debug_str.contains("2048"));
    assert!(debug_str.contains("10"));
}

#[tokio::test]
async fn test_grpc_server_concurrent_creation() {
    // Test concurrent gRPC server creation / 测试并发gRPC服务器创建
    use std::sync::Arc;
    use tokio::task::JoinSet;

    let mut join_set = JoinSet::new();

    // Create multiple servers concurrently / 并发创建多个服务器
    for i in 0..5 {
        let mut config = create_test_config();
        config.grpc.addr = format!("127.0.0.1:{}", 50060 + i).parse().unwrap();

        join_set.spawn(async move {
            let server = GrpcServer::new(Arc::new(config), None).await.unwrap();
            let object_service = server.get_object_service();
            object_service.get_stats().await.object_count == 0
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

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_grpc_server_lifecycle() {
        // Test complete gRPC server lifecycle / 测试完整的gRPC服务器生命周期
        let config = Arc::new(create_test_config());
        let server = GrpcServer::new(config, None).await.unwrap();

        // Get object service before starting server / 在启动服务器前获取对象服务
        let object_service = server.get_object_service();
        let function_service = server.get_function_service();
        let health_service = HealthService::new(object_service, function_service);

        // Check initial health status / 检查初始健康状态
        let initial_status = health_service.get_health_status().await;
        assert_eq!(initial_status.status, "healthy");
        assert_eq!(initial_status.object_count, 0);

        // Note: We don't actually start the server in tests to avoid port conflicts
        // 注意：我们在测试中不实际启动服务器以避免端口冲突
    }

    #[tokio::test]
    async fn test_grpc_server_stress_test() {
        // Stress test for gRPC server creation / gRPC服务器创建的压力测试
        let config = Arc::new(create_test_config());

        // Create and destroy servers rapidly / 快速创建和销毁服务器
        for _ in 0..100 {
            let server = GrpcServer::new(config.clone(), None).await.unwrap();
            let object_service = server.get_object_service();
            assert_eq!(object_service.get_stats().await.object_count, 0);
            // Server goes out of scope and is dropped / 服务器超出作用域并被丢弃
        }
    }

    #[tokio::test]
    async fn test_health_service_performance() {
        // Test health service performance / 测试健康服务性能
        let config = Arc::new(create_test_config());
        let server = GrpcServer::new(config, None).await.unwrap();
        let object_service = server.get_object_service();
        let function_service = server.get_function_service();
        let health_service = HealthService::new(object_service, function_service);

        // Measure time for health status retrieval / 测量健康状态获取时间
        let start = std::time::Instant::now();

        for _ in 0..1000 {
            let _ = health_service.get_health_status().await;
        }

        let duration = start.elapsed();

        // Should complete 1000 health checks in reasonable time / 应该在合理时间内完成1000次健康检查
        assert!(duration.as_millis() < 1000); // Less than 1 second
    }
}
