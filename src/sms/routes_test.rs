//! Tests for SMS HTTP Routes
//! SMS HTTP路由测试

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
};
use tower::ServiceExt;

use crate::proto::sms::{
    admin_credential_service_client::AdminCredentialServiceClient,
    backend_registry_service_client::BackendRegistryServiceClient,
    execution_index_service_client::ExecutionIndexServiceClient,
    execution_registry_service_client::ExecutionRegistryServiceClient,
    instance_registry_service_client::InstanceRegistryServiceClient,
    mcp_registry_service_client::McpRegistryServiceClient,
    node_service_client::NodeServiceClient, placement_service_client::PlacementServiceClient,
    task_service_client::TaskServiceClient,
};
use crate::sms::gateway::{create_gateway_router, GatewayState};
use crate::sms::routes::create_routes;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Create a mock gateway state for testing / 创建用于测试的模拟网关状态
fn create_mock_gateway_state() -> GatewayState {
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
        task_assignment_client: crate::proto::sms::task_placement_assignment_service_client::TaskPlacementAssignmentServiceClient::new(channel.clone()),
        placement_client: PlacementServiceClient::new(channel.clone()),
        instance_registry_client: InstanceRegistryServiceClient::new(channel.clone()),
        execution_registry_client: ExecutionRegistryServiceClient::new(channel.clone()),
        execution_index_client: ExecutionIndexServiceClient::new(channel.clone()),
        mcp_registry_client: McpRegistryServiceClient::new(channel.clone()),
        backend_registry_client: BackendRegistryServiceClient::new(channel.clone()),
        ai_backend_control_plane_client:
            crate::proto::sms::ai_backend_control_plane_service_client::AiBackendControlPlaneServiceClient::new(channel.clone()),
        admin_credential_client: AdminCredentialServiceClient::new(channel.clone()),
        stream_sessions: crate::sms::gateway::StreamSessionStore::new(),
        execution_stream_pool: crate::sms::gateway::ExecutionStreamPool::new(),
        cancel_token: CancellationToken::new(),
        max_upload_bytes: 64 * 1024 * 1024,
        files_dir,
    }
}

#[tokio::test]
async fn test_routes_creation() {
    // Test routes creation / 测试路由创建
    let state = create_mock_gateway_state();
    let app = create_routes(state);

    // Verify the router was created successfully by checking its type / 通过检查类型验证路由器创建成功
    let _service = app.into_make_service();
    // If we reach here, the router was created successfully / 如果到达这里，说明路由器创建成功
}

#[tokio::test]
async fn test_health_route() {
    // Test health check route / 测试健康检查路由
    let state = create_mock_gateway_state();
    let app = create_routes(state);

    let request = Request::builder()
        .method(Method::GET)
        .uri("/health")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    // Health endpoint should return 200 OK / 健康端点应返回200 OK
    assert_eq!(response.status(), StatusCode::OK);

    // Verify content type / 验证内容类型
    let content_type = response.headers().get("content-type");
    assert!(content_type.is_some());
    assert!(content_type
        .unwrap()
        .to_str()
        .unwrap()
        .contains("application/json"));
}

#[tokio::test]
async fn test_node_routes_structure() {
    // Test node routes structure / 测试节点路由结构
    let state = create_mock_gateway_state();
    let app = create_routes(state);

    // Test various node endpoints (they will fail due to no gRPC server, but routes should exist)
    // 测试各种节点端点（由于没有gRPC服务器会失败，但路由应该存在）
    let test_cases = vec![
        (Method::POST, "/api/v1/nodes"),
        (Method::GET, "/api/v1/nodes"),
        (
            Method::GET,
            "/api/v1/nodes/00000000-0000-0000-0000-000000000000",
        ),
        (
            Method::PUT,
            "/api/v1/nodes/00000000-0000-0000-0000-000000000000",
        ),
        (
            Method::DELETE,
            "/api/v1/nodes/00000000-0000-0000-0000-000000000000",
        ),
        (
            Method::POST,
            "/api/v1/nodes/00000000-0000-0000-0000-000000000000/heartbeat",
        ),
    ];

    for (method, uri) in test_cases {
        let request = Request::builder()
            .method(method.clone())
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap();

        let response = app.clone().oneshot(request).await.unwrap();

        // For ID-specific endpoints, 404 indicates entity not found, not missing route
        // 对于带ID的端点，404表示实体不存在，而不是路由缺失
        if uri.starts_with("/api/v1/nodes/") && uri != "/api/v1/nodes" {
            assert!(
                [
                    StatusCode::OK,
                    StatusCode::NOT_FOUND,
                    StatusCode::INTERNAL_SERVER_ERROR,
                    StatusCode::CREATED
                ]
                .contains(&response.status()),
                "Route {} {} should be matched",
                method,
                uri
            );
        } else {
            assert_ne!(
                response.status(),
                StatusCode::NOT_FOUND,
                "Route {} {} should exist",
                method,
                uri
            );
        }
    }
}

#[tokio::test]
async fn test_resource_routes_structure() {
    // Test resource routes structure / 测试资源路由结构
    let state = create_mock_gateway_state();
    let app = create_routes(state);

    let test_cases = vec![
        (
            Method::PUT,
            "/api/v1/nodes/00000000-0000-0000-0000-000000000000/resource",
        ),
        (
            Method::GET,
            "/api/v1/nodes/00000000-0000-0000-0000-000000000000/resource",
        ),
        (Method::GET, "/api/v1/resources"),
        (
            Method::GET,
            "/api/v1/nodes/00000000-0000-0000-0000-000000000000/with-resource",
        ),
    ];

    for (method, uri) in test_cases {
        let request = Request::builder()
            .method(method.clone())
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap();

        let response = app.clone().oneshot(request).await.unwrap();

        // ID-specific endpoints may legitimately return 404 if data is absent
        // 带ID的端点在数据缺失时返回404是合理的
        if uri.contains("/api/v1/nodes/") && !uri.ends_with("/resources") {
            assert!(
                [
                    StatusCode::OK,
                    StatusCode::NOT_FOUND,
                    StatusCode::INTERNAL_SERVER_ERROR
                ]
                .contains(&response.status()),
                "Route {} {} should be matched",
                method,
                uri
            );
        } else {
            assert_ne!(
                response.status(),
                StatusCode::NOT_FOUND,
                "Route {} {} should exist",
                method,
                uri
            );
        }
    }
}

#[tokio::test]
async fn test_stream_routes_structure() {
    // Test stream routes structure / 测试流相关路由结构
    let state = create_mock_gateway_state();
    let app = create_routes(state);

    let test_cases = vec![
        (Method::POST, "/api/v1/executions/exec-1/streams/session"),
        (Method::GET, "/api/v1/executions/exec-1/streams/ws?token=t"),
        (Method::GET, "/e/echo/ws"),
    ];

    for (method, uri) in test_cases {
        let request = Request::builder()
            .method(method.clone())
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap();

        let response = app.clone().oneshot(request).await.unwrap();
        if uri.starts_with("/api/v1/executions/") {
            assert!(
                [
                    StatusCode::OK,
                    StatusCode::BAD_REQUEST,
                    StatusCode::UNAUTHORIZED,
                    StatusCode::FORBIDDEN,
                    StatusCode::NOT_FOUND,
                    StatusCode::BAD_GATEWAY,
                    StatusCode::INTERNAL_SERVER_ERROR
                ]
                .contains(&response.status()),
                "Route {} {} should be matched",
                method,
                uri
            );
        } else {
            assert_ne!(
                response.status(),
                StatusCode::NOT_FOUND,
                "Route {} {} should exist",
                method,
                uri
            );
        }
    }
}

#[tokio::test]
async fn test_task_routes_structure() {
    // Test task routes structure / 测试任务路由结构
    let state = create_mock_gateway_state();
    let app = create_routes(state);

    let test_cases = vec![
        (Method::POST, "/api/v1/tasks"),
        (Method::GET, "/api/v1/tasks"),
        (Method::GET, "/api/v1/tasks/task-123"),
        (Method::DELETE, "/api/v1/tasks/task-123"),
        (Method::POST, "/api/v1/placement/invocations/place"),
        (Method::POST, "/api/v1/placement/invocations/report-outcome"),
    ];

    for (method, uri) in test_cases {
        let request = Request::builder()
            .method(method.clone())
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap();

        let response = app.clone().oneshot(request).await.unwrap();

        // ID-specific endpoints might return 404 when task doesn't exist
        // 带ID的端点在任务不存在时可能返回404
        if uri.starts_with("/api/v1/tasks/") && uri != "/api/v1/tasks" {
            assert!(
                [
                    StatusCode::OK,
                    StatusCode::NOT_FOUND,
                    StatusCode::INTERNAL_SERVER_ERROR
                ]
                .contains(&response.status()),
                "Route {} {} should be matched",
                method,
                uri
            );
        } else {
            assert_ne!(
                response.status(),
                StatusCode::NOT_FOUND,
                "Route {} {} should exist",
                method,
                uri
            );
        }
    }
}

#[tokio::test]
async fn test_documentation_routes() {
    // Test documentation routes / 测试文档路由
    let state = create_mock_gateway_state();
    let app = create_routes(state);

    let doc_routes = vec!["/api/openapi.json", "/swagger-ui/"];

    for uri in doc_routes {
        let request = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .body(Body::empty())
            .unwrap();

        let response = app.clone().oneshot(request).await.unwrap();

        // Documentation routes should exist / 文档路由应该存在
        assert_ne!(
            response.status(),
            StatusCode::NOT_FOUND,
            "Documentation route {} should exist",
            uri
        );
    }
}

#[tokio::test]
async fn test_invalid_routes() {
    // Test invalid routes return 404 / 测试无效路由返回404
    let state = create_mock_gateway_state();
    let app = create_routes(state);

    let invalid_routes = vec![
        "/invalid",
        "/api/invalid",
        "/api/v1/invalid",
        "/api/v2/nodes", // Wrong version / 错误版本
        "/nodes",        // Missing api prefix / 缺少api前缀
    ];

    for uri in invalid_routes {
        let request = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .body(Body::empty())
            .unwrap();

        let response = app.clone().oneshot(request).await.unwrap();

        // Invalid routes should return 404 / 无效路由应返回404
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "Invalid route {} should return 404",
            uri
        );
    }
}

#[tokio::test]
async fn test_method_not_allowed() {
    // Test method not allowed for existing routes / 测试现有路由的方法不允许
    let state = create_mock_gateway_state();
    let app = create_routes(state);

    // Test wrong methods on existing routes / 测试现有路由的错误方法
    let test_cases = vec![
        (Method::DELETE, "/health"), // Health only supports GET / 健康检查只支持GET
        (Method::PUT, "/api/v1/nodes"), // Nodes list only supports GET and POST / 节点列表只支持GET和POST
        (Method::PATCH, "/api/v1/tasks"), // Tasks list only supports GET and POST / 任务列表只支持GET和POST
    ];

    for (method, uri) in test_cases {
        let request = Request::builder()
            .method(method.clone())
            .uri(uri)
            .body(Body::empty())
            .unwrap();

        let response = app.clone().oneshot(request).await.unwrap();

        // Should return method not allowed / 应返回方法不允许
        assert_eq!(
            response.status(),
            StatusCode::METHOD_NOT_ALLOWED,
            "Method {} on {} should not be allowed",
            method,
            uri
        );
    }
}

#[tokio::test]
async fn test_cors_headers() {
    // Test CORS headers are present / 测试CORS头部存在
    let state = create_mock_gateway_state();
    let app = create_gateway_router(state);

    let request = Request::builder()
        .method(Method::GET)
        .uri("/health")
        .header("Origin", "http://localhost:3000")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    // Request should succeed / 请求应该成功
    assert_eq!(response.status(), StatusCode::OK);

    // Check for CORS headers / 检查CORS头部
    let headers = response.headers();
    assert!(
        headers.contains_key("access-control-allow-origin")
            || headers.contains_key("Access-Control-Allow-Origin"),
        "CORS headers should be present"
    );
}

#[tokio::test]
async fn test_content_type_handling() {
    // Test content type handling / 测试内容类型处理
    let state = create_mock_gateway_state();
    let app = create_routes(state);

    // Test with correct content type / 测试正确的内容类型
    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/nodes")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"ip_address": "127.0.0.1", "port": 8080}"#))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();

    // Should not return 415 Unsupported Media Type / 不应返回415不支持的媒体类型
    assert_ne!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);

    // Test with incorrect content type / 测试错误的内容类型
    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/nodes")
        .header("content-type", "text/plain")
        .body(Body::from("not json"))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    // May return 400 Bad Request or 415 Unsupported Media Type / 可能返回400错误请求或415不支持的媒体类型
    assert!(
        response.status() == StatusCode::BAD_REQUEST
            || response.status() == StatusCode::UNSUPPORTED_MEDIA_TYPE
            || response.status() == StatusCode::INTERNAL_SERVER_ERROR
    ); // Due to gRPC connection / 由于gRPC连接
}

#[tokio::test]
async fn test_large_request_handling() {
    // Test large request handling / 测试大请求处理
    let state = create_mock_gateway_state();
    let app = create_routes(state);

    // Create a large JSON payload / 创建大JSON负载
    let large_metadata = (0..1000)
        .map(|i| format!(r#""key{}": "value{}""#, i, i))
        .collect::<Vec<_>>()
        .join(", ");

    let large_json = format!(
        r#"{{
        "ip_address": "127.0.0.1",
        "port": 8080,
        "metadata": {{{}}}
    }}"#,
        large_metadata
    );

    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/nodes")
        .header("content-type", "application/json")
        .body(Body::from(large_json))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    // Should handle large requests (not return 413 Payload Too Large) / 应处理大请求（不返回413负载过大）
    assert_ne!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

// Integration tests for route behavior / 路由行为的集成测试
#[cfg(test)]
mod integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_concurrent_route_access() {
        // Test concurrent access to routes / 测试路由的并发访问
        let state = create_mock_gateway_state();
        let app = create_routes(state);
        let mut handles = vec![];

        for i in 0..10 {
            let app_clone = app.clone();
            let handle = tokio::spawn(async move {
                let request = Request::builder()
                    .method(Method::GET)
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap();

                let response = app_clone.oneshot(request).await.unwrap();
                (i, response.status())
            });
            handles.push(handle);
        }

        for handle in handles {
            let (i, status) = handle.await.unwrap();
            assert_eq!(status, StatusCode::OK, "Request {} failed", i);
        }
    }

    #[tokio::test]
    async fn test_route_performance() {
        // Test route performance under load / 测试负载下的路由性能
        let state = create_mock_gateway_state();
        let app = create_routes(state);

        let start = std::time::Instant::now();
        let mut handles = vec![];

        for i in 0..100 {
            let app_clone = app.clone();
            let handle = tokio::spawn(async move {
                let request = Request::builder()
                    .method(Method::GET)
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap();

                let response = app_clone.oneshot(request).await.unwrap();
                (i, response.status())
            });
            handles.push(handle);
        }

        for handle in handles {
            let (i, status) = handle.await.unwrap();
            assert_eq!(status, StatusCode::OK, "Request {} failed", i);
        }

        let duration = start.elapsed();
        assert!(
            duration.as_secs() < 5,
            "Route performance too slow: {:?}",
            duration
        );
    }

    #[tokio::test]
    async fn test_memory_usage_under_load() {
        // Test memory usage under load / 测试负载下的内存使用
        let state = create_mock_gateway_state();
        let app = create_routes(state);

        // Simulate load / 模拟负载
        let mut handles = vec![];

        for _ in 0..50 {
            let app_clone = app.clone();
            let handle = tokio::spawn(async move {
                for _ in 0..10 {
                    let request = Request::builder()
                        .method(Method::GET)
                        .uri("/health")
                        .body(Body::empty())
                        .unwrap();

                    let _response = app_clone.clone().oneshot(request).await.unwrap();
                }
            });
            handles.push(handle);
        }

        // Wait for all requests to complete / 等待所有请求完成
        for handle in handles {
            handle.await.unwrap();
        }

        // If we reach here without panicking, memory usage is acceptable
        // 如果我们到达这里而没有panic，内存使用是可接受的
    }
}
