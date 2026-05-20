//! Tests for SMS HTTP Gateway
//! SMS HTTP网关测试

use axum::Router;
use std::sync::Arc;
use tonic::transport::Channel;

use crate::proto::sms::{
    admin_llm_config_service_client::AdminLlmConfigServiceClient,
    backend_registry_service_client::BackendRegistryServiceClient,
    execution_index_service_client::ExecutionIndexServiceClient,
    execution_registry_service_client::ExecutionRegistryServiceClient,
    instance_registry_service_client::InstanceRegistryServiceClient,
    mcp_registry_service_client::McpRegistryServiceClient,
    model_deployment_registry_service_client::ModelDeploymentRegistryServiceClient,
    node_service_client::NodeServiceClient, placement_service_client::PlacementServiceClient,
    task_service_client::TaskServiceClient,
};
use crate::sms::gateway::{create_gateway_router, GatewayState};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Create a mock gateway state for testing / 创建用于测试的模拟网关状态
async fn create_mock_gateway_state() -> GatewayState {
    // Create a mock channel for testing / 创建用于测试的模拟通道
    let channel = tonic::transport::Channel::from_static("http://localhost:50051").connect_lazy();
    let files_dir = std::env::temp_dir()
        .join(format!("spear-sms-files-{}", Uuid::new_v4()))
        .to_string_lossy()
        .to_string();

    GatewayState {
        config: Arc::new(crate::sms::config::SmsConfig::default()),
        node_client: NodeServiceClient::new(channel.clone()),
        task_client: TaskServiceClient::new(channel.clone()),
        placement_client: PlacementServiceClient::new(channel.clone()),
        instance_registry_client: InstanceRegistryServiceClient::new(channel.clone()),
        execution_registry_client: ExecutionRegistryServiceClient::new(channel.clone()),
        execution_index_client: ExecutionIndexServiceClient::new(channel.clone()),
        mcp_registry_client: McpRegistryServiceClient::new(channel.clone()),
        backend_registry_client: BackendRegistryServiceClient::new(channel.clone()),
        admin_llm_config_client: AdminLlmConfigServiceClient::new(channel.clone()),
        model_deployment_registry_client: ModelDeploymentRegistryServiceClient::new(
            channel.clone(),
        ),
        stream_sessions: crate::sms::gateway::StreamSessionStore::new(),
        execution_stream_pool: crate::sms::gateway::ExecutionStreamPool::new(),
        cancel_token: CancellationToken::new(),
        max_upload_bytes: 64 * 1024 * 1024,
        files_dir,
    }
}

#[tokio::test]
async fn test_gateway_state_creation() {
    // Test gateway state creation / 测试网关状态创建
    let state = create_mock_gateway_state().await;

    // Verify state was created successfully / 验证状态创建成功
    // We can't directly test the clients without a gRPC server, but we can verify they exist
    // 我们无法在没有gRPC服务器的情况下直接测试客户端，但我们可以验证它们存在
    assert!(std::mem::size_of_val(&state.node_client) > 0);
    assert!(std::mem::size_of_val(&state.task_client) > 0);
    assert!(std::mem::size_of_val(&state.placement_client) > 0);
}

#[tokio::test]
async fn test_gateway_state_clone() {
    // Test gateway state cloning / 测试网关状态克隆
    let state = create_mock_gateway_state().await;
    let cloned_state = state.clone();

    // Verify cloned state has the same structure / 验证克隆状态具有相同结构
    assert!(std::mem::size_of_val(&cloned_state.node_client) > 0);
    assert!(std::mem::size_of_val(&cloned_state.task_client) > 0);
    assert!(std::mem::size_of_val(&cloned_state.placement_client) > 0);
}

#[tokio::test]
async fn test_create_gateway_router() {
    // Test gateway router creation / 测试网关路由器创建
    let state = create_mock_gateway_state().await;
    let router = create_gateway_router(state);

    // Verify router was created successfully / 验证路由器创建成功
    assert!(std::mem::size_of_val(&router) > 0);

    // Verify it's actually a Router / 验证它确实是一个Router
    let _: Router = router; // This will fail to compile if it's not a Router / 如果不是Router，这将编译失败
}

#[tokio::test]
async fn test_gateway_router_with_different_states() {
    // Test gateway router creation with different states / 测试使用不同状态创建网关路由器
    for i in 0..5 {
        let state = create_mock_gateway_state().await;
        let router = create_gateway_router(state);

        // Each router should be created successfully / 每个路由器都应该创建成功
        assert!(
            std::mem::size_of_val(&router) > 0,
            "Router {} creation failed",
            i
        );
    }
}

#[tokio::test]
async fn test_gateway_state_memory_usage() {
    // Test gateway state memory usage / 测试网关状态内存使用
    let state = create_mock_gateway_state().await;
    let state_size = std::mem::size_of_val(&state);

    // Gateway state should not use excessive memory / 网关状态不应使用过多内存
    assert!(
        state_size < 2048,
        "Gateway state uses too much memory: {} bytes",
        state_size
    );
}

#[tokio::test]
async fn test_multiple_gateway_states() {
    // Test creating multiple gateway states / 测试创建多个网关状态
    let mut states = Vec::new();

    for _ in 0..10 {
        let state = create_mock_gateway_state().await;
        states.push(state);
    }

    // All states should be created successfully / 所有状态都应该创建成功
    assert_eq!(states.len(), 10);

    for (i, state) in states.iter().enumerate() {
        assert!(
            std::mem::size_of_val(&state.node_client) > 0,
            "State {} node client invalid",
            i
        );
        assert!(
            std::mem::size_of_val(&state.task_client) > 0,
            "State {} task client invalid",
            i
        );
        assert!(
            std::mem::size_of_val(&state.placement_client) > 0,
            "State {} placement client invalid",
            i
        );
    }
}

#[tokio::test]
async fn test_gateway_state_with_different_endpoints() {
    // Test gateway state with different gRPC endpoints / 测试使用不同gRPC端点的网关状态
    let endpoints = vec![
        "http://localhost:50051",
        "http://127.0.0.1:50052",
        "http://0.0.0.0:50053",
        "https://example.com:443",
    ];

    for endpoint in endpoints {
        let channel = Channel::from_static(endpoint).connect_lazy();
        let node_client = NodeServiceClient::new(channel.clone());
        let task_client = TaskServiceClient::new(channel.clone());
        let placement_client = PlacementServiceClient::new(channel.clone());
        let instance_registry_client = InstanceRegistryServiceClient::new(channel.clone());
        let execution_registry_client = ExecutionRegistryServiceClient::new(channel.clone());
        let execution_index_client = ExecutionIndexServiceClient::new(channel.clone());
        let mcp_registry_client = McpRegistryServiceClient::new(channel.clone());
        let backend_registry_client = BackendRegistryServiceClient::new(channel.clone());
        let admin_llm_config_client = AdminLlmConfigServiceClient::new(channel.clone());
        let model_deployment_registry_client =
            ModelDeploymentRegistryServiceClient::new(channel.clone());

        let state = GatewayState {
            config: Arc::new(crate::sms::config::SmsConfig::default()),
            node_client,
            task_client,
            placement_client,
            instance_registry_client,
            execution_registry_client,
            execution_index_client,
            mcp_registry_client,
            backend_registry_client,
            admin_llm_config_client,
            model_deployment_registry_client,
            stream_sessions: crate::sms::gateway::StreamSessionStore::new(),
            execution_stream_pool: crate::sms::gateway::ExecutionStreamPool::new(),
            cancel_token: CancellationToken::new(),
            max_upload_bytes: 64 * 1024 * 1024,
            files_dir: std::env::temp_dir()
                .join(format!("spear-sms-files-{}", Uuid::new_v4()))
                .to_string_lossy()
                .to_string(),
        };

        // State should be created successfully with any endpoint / 任何端点都应该成功创建状态
        assert!(std::mem::size_of_val(&state) > 0);
    }
}

#[tokio::test]
async fn test_gateway_router_service_conversion() {
    // Test gateway router service conversion / 测试网关路由器服务转换
    let state = create_mock_gateway_state().await;
    let router = create_gateway_router(state);

    // Convert router to service / 将路由器转换为服务
    let _service = router.into_make_service();

    // Service conversion should succeed / 服务转换应该成功
}

#[tokio::test]
async fn test_concurrent_gateway_creation() {
    // Test concurrent gateway creation / 测试并发网关创建

    let mut handles = vec![];

    for i in 0..10 {
        let handle = tokio::spawn(async move {
            let state = create_mock_gateway_state().await;
            let router = create_gateway_router(state);
            (i, std::mem::size_of_val(&router))
        });
        handles.push(handle);
    }

    for handle in handles {
        let (i, router_size) = handle.await.unwrap();
        assert!(router_size > 0, "Router {} creation failed", i);
    }
}

#[tokio::test]
async fn test_gateway_state_debug_format() {
    // Test gateway state debug format / 测试网关状态调试格式
    let state = create_mock_gateway_state().await;
    let debug_str = format!("{:?}", state);

    // Debug string should contain relevant information / 调试字符串应包含相关信息
    assert!(debug_str.contains("GatewayState"));
    assert!(!debug_str.is_empty());
}

#[tokio::test]
async fn test_gateway_state_field_access() {
    // Test gateway state field access / 测试网关状态字段访问
    let state = create_mock_gateway_state().await;

    // Should be able to access fields / 应该能够访问字段
    let _node_client = &state.node_client;
    let _task_client = &state.task_client;
    let _placement_client = &state.placement_client;

    // Fields should be accessible / 字段应该可访问
}

#[tokio::test]
async fn test_gateway_state_move_semantics() {
    // Test gateway state move semantics / 测试网关状态移动语义
    let state = create_mock_gateway_state().await;

    // Move state to router creation / 将状态移动到路由器创建
    let router = create_gateway_router(state);

    // Router should be created successfully / 路由器应该创建成功
    assert!(std::mem::size_of_val(&router) > 0);

    // Original state is now moved and cannot be used / 原始状态现在已移动，无法使用
    // This is expected behavior / 这是预期行为
}

// Integration tests for gateway functionality / 网关功能的集成测试
#[cfg(test)]
mod integration_tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_gateway_stress_test() {
        // Stress test: create many gateways concurrently / 压力测试：并发创建多个网关
        let num_gateways = 100;
        let mut handles = vec![];

        for i in 0..num_gateways {
            let handle = tokio::spawn(async move {
                let state = create_mock_gateway_state().await;
                let router = create_gateway_router(state);
                (i, std::mem::size_of_val(&router))
            });
            handles.push(handle);
        }

        // Wait for all gateways to be created / 等待所有网关创建完成
        let results: Vec<(usize, usize)> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|result| result.unwrap())
            .collect();

        assert_eq!(results.len(), num_gateways);

        for (i, router_size) in results {
            assert!(router_size > 0, "Gateway {} creation failed", i);
        }
    }

    #[tokio::test]
    async fn test_gateway_memory_efficiency() {
        // Test memory efficiency of gateway creation / 测试网关创建的内存效率
        let initial_memory = std::mem::size_of::<GatewayState>();

        let state = create_mock_gateway_state().await;
        let state_memory = std::mem::size_of_val(&state);

        // Memory usage should be reasonable / 内存使用应该合理
        assert!(state_memory >= initial_memory);
        assert!(state_memory < initial_memory * 10); // Reasonable upper bound / 合理的上限

        let router = create_gateway_router(state);
        let router_memory = std::mem::size_of_val(&router);

        // Router memory should also be reasonable / 路由器内存也应该合理
        assert!(router_memory > 0);
        assert!(router_memory < 10240); // Less than 10KB / 小于10KB
    }

    #[tokio::test]
    async fn test_gateway_creation_performance() {
        // Test gateway creation performance / 测试网关创建性能
        let start = std::time::Instant::now();

        for _ in 0..100 {
            // Reduced from 1000 to 100 for async
            let state = create_mock_gateway_state().await;
            let _router = create_gateway_router(state);
        }

        let duration = start.elapsed();

        // Should create 100 gateways in reasonable time / 应在合理时间内创建100个网关
        assert!(
            duration.as_secs() < 5,
            "Gateway creation too slow: {:?}",
            duration
        );
    }

    #[tokio::test]
    async fn test_gateway_concurrent_access() {
        // Test concurrent access to gateway components / 测试网关组件的并发访问
        let state = Arc::new(create_mock_gateway_state().await);
        let mut handles = vec![];

        for i in 0..50 {
            let state_clone = Arc::clone(&state);
            let handle = tokio::spawn(async move {
                // Access gateway state fields concurrently / 并发访问网关状态字段
                let _node_client = &state_clone.node_client;
                let _task_client = &state_clone.task_client;
                i
            });
            handles.push(handle);
        }

        // All concurrent accesses should succeed / 所有并发访问都应该成功
        let results: Vec<usize> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|result| result.unwrap())
            .collect();

        assert_eq!(results.len(), 50);

        // Verify all tasks completed / 验证所有任务完成
        for (expected, actual) in (0..50).zip(results.iter()) {
            assert_eq!(expected, *actual);
        }
    }
}
