//! Tests for HTTP gateway module
//! HTTP网关模块的测试

use axum::Router;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tonic::transport::Channel;

use crate::config::base::ServerConfig;
use crate::spearlet::config::{HttpConfig, SpearletConfig, StorageConfig};
use crate::spearlet::function_service::FunctionServiceImpl;
use crate::spearlet::grpc_server::HealthService;
use crate::spearlet::http_gateway::HttpGateway;
use crate::spearlet::object_service::ObjectServiceImpl;

/// Create test configuration / 创建测试配置
fn create_test_config() -> SpearletConfig {
    SpearletConfig {
        http: HttpConfig {
            server: ServerConfig {
                addr: "127.0.0.1:0".parse().unwrap(),
                ..Default::default()
            },
            cors_enabled: true,
            swagger_enabled: true,
        },
        grpc: ServerConfig {
            addr: "127.0.0.1:0".parse().unwrap(),
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

fn create_dummy_grpc_clients(
    addr: std::net::SocketAddr,
) -> (
    crate::proto::spearlet::object_service_client::ObjectServiceClient<Channel>,
    crate::proto::spearlet::invocation_service_client::InvocationServiceClient<Channel>,
    crate::proto::spearlet::execution_service_client::ExecutionServiceClient<Channel>,
) {
    let channel = Channel::from_shared(format!("http://{}", addr))
        .unwrap()
        .connect_lazy();
    (
        crate::proto::spearlet::object_service_client::ObjectServiceClient::new(channel.clone()),
        crate::proto::spearlet::invocation_service_client::InvocationServiceClient::new(
            channel.clone(),
        ),
        crate::proto::spearlet::execution_service_client::ExecutionServiceClient::new(channel),
    )
}

/// Create test HTTP gateway / 创建测试HTTP网关
async fn create_test_gateway() -> HttpGateway {
    let config = Arc::new(create_test_config());
    let object_service = Arc::new(ObjectServiceImpl::new_with_memory(1024 * 1024));
    let function_service = Arc::new(
        FunctionServiceImpl::new(Arc::new(create_test_config()), None)
            .await
            .unwrap(),
    );
    let health_service = Arc::new(HealthService::new(object_service, function_service.clone()));
    let (object_client, invocation_client, execution_client) =
        create_dummy_grpc_clients(config.grpc.addr);
    HttpGateway::new(
        config,
        health_service,
        function_service,
        object_client,
        invocation_client,
        execution_client,
    )
}

#[tokio::test]
async fn test_http_gateway_creation() {
    // Test HTTP gateway creation / 测试HTTP网关创建
    let _gateway = create_test_gateway().await;

    // Gateway should be created successfully / 网关应该成功创建
    // Note: We can't easily test the internal state without exposing it
    // 注意：我们无法在不暴露内部状态的情况下轻松测试内部状态
}

#[tokio::test]
async fn test_gateway_config() {
    // Test HTTP gateway configuration / 测试HTTP网关配置
    let mut config = create_test_config();
    config.http.server.addr = "0.0.0.0:8080".parse().unwrap();
    config.http.swagger_enabled = false;

    let object_service = Arc::new(ObjectServiceImpl::new_with_memory(1024 * 1024));
    let function_service = Arc::new(
        FunctionServiceImpl::new(Arc::new(create_test_config()), None)
            .await
            .unwrap(),
    );
    let health_service = Arc::new(HealthService::new(object_service, function_service.clone()));
    let config = Arc::new(config);
    let (object_client, invocation_client, execution_client) =
        create_dummy_grpc_clients(config.grpc.addr);
    let _gateway = HttpGateway::new(
        config,
        health_service,
        function_service,
        object_client,
        invocation_client,
        execution_client,
    );

    // Gateway should be created with custom config / 网关应该使用自定义配置创建
}

#[tokio::test]
async fn test_gateway_with_different_storage_sizes() {
    // Test HTTP gateway with different storage sizes / 测试不同存储大小的HTTP网关
    let sizes = vec![1024, 1024 * 1024, 10 * 1024 * 1024]; // 1KB, 1MB, 10MB

    for size in sizes {
        let mut config = create_test_config();
        config.storage.max_object_size = size;

        let object_service = Arc::new(ObjectServiceImpl::new_with_memory(size));
        let function_service = Arc::new(
            FunctionServiceImpl::new(Arc::new(create_test_config()), None)
                .await
                .unwrap(),
        );
        let health_service = Arc::new(HealthService::new(object_service, function_service.clone()));
        let config = Arc::new(config);
        let (object_client, invocation_client, execution_client) =
            create_dummy_grpc_clients(config.grpc.addr);
        let _gateway = HttpGateway::new(
            config,
            health_service,
            function_service,
            object_client,
            invocation_client,
            execution_client,
        );

        // Gateway should be created with different storage sizes / 网关应该使用不同存储大小创建
    }
}

#[tokio::test]
async fn test_gateway_swagger_enabled() {
    // Test HTTP gateway with Swagger enabled / 测试启用Swagger的HTTP网关
    let mut config = create_test_config();
    config.http.swagger_enabled = true;

    let object_service = Arc::new(ObjectServiceImpl::new_with_memory(1024 * 1024));
    let function_service = Arc::new(
        FunctionServiceImpl::new(Arc::new(create_test_config()), None)
            .await
            .unwrap(),
    );
    let health_service = Arc::new(HealthService::new(object_service, function_service.clone()));
    let config = Arc::new(config);
    let (object_client, invocation_client, execution_client) =
        create_dummy_grpc_clients(config.grpc.addr);
    let _gateway = HttpGateway::new(
        config,
        health_service,
        function_service,
        object_client,
        invocation_client,
        execution_client,
    );

    // Gateway should be created with Swagger enabled / 网关应该启用Swagger创建
}

#[tokio::test]
async fn test_gateway_swagger_disabled() {
    // Test HTTP gateway with Swagger disabled / 测试禁用Swagger的HTTP网关
    let mut config = create_test_config();
    config.http.swagger_enabled = false;

    let object_service = Arc::new(ObjectServiceImpl::new_with_memory(1024 * 1024));
    let function_service = Arc::new(
        FunctionServiceImpl::new(Arc::new(create_test_config()), None)
            .await
            .unwrap(),
    );
    let health_service = Arc::new(HealthService::new(object_service, function_service.clone()));
    let config = Arc::new(config);
    let (object_client, invocation_client, execution_client) =
        create_dummy_grpc_clients(config.grpc.addr);
    let _gateway = HttpGateway::new(
        config,
        health_service,
        function_service,
        object_client,
        invocation_client,
        execution_client,
    );

    // Gateway should be created with Swagger disabled / 网关应该禁用Swagger创建
}

#[tokio::test]
async fn test_invalid_http_address() {
    // Test HTTP gateway with invalid address / 测试无效地址的HTTP网关
    let mut config = create_test_config();
    config.http.server.addr = "0.0.0.0:8080".parse().unwrap();

    let object_service = Arc::new(ObjectServiceImpl::new_with_memory(1024 * 1024));
    let function_service = Arc::new(
        FunctionServiceImpl::new(Arc::new(create_test_config()), None)
            .await
            .unwrap(),
    );
    let health_service = Arc::new(HealthService::new(object_service, function_service.clone()));
    let config = Arc::new(config);
    let (object_client, invocation_client, execution_client) =
        create_dummy_grpc_clients(config.grpc.addr);
    let _gateway = HttpGateway::new(
        config,
        health_service,
        function_service,
        object_client,
        invocation_client,
        execution_client,
    );

    // Gateway creation should succeed, but start() would fail
    // 网关创建应该成功，但start()会失败
    // Note: We don't test start() here to avoid actual network binding
    // 注意：我们这里不测试start()以避免实际的网络绑定
}

#[tokio::test]
async fn test_multiple_gateways() {
    // Test creating multiple HTTP gateways / 测试创建多个HTTP网关
    let config1 = Arc::new(create_test_config());
    let config2 = Arc::new(create_test_config());

    let object_service1 = Arc::new(ObjectServiceImpl::new_with_memory(1024 * 1024));
    let object_service2 = Arc::new(ObjectServiceImpl::new_with_memory(1024 * 1024));

    let function_service1 = Arc::new(
        FunctionServiceImpl::new(Arc::new(create_test_config()), None)
            .await
            .unwrap(),
    );
    let function_service2 = Arc::new(
        FunctionServiceImpl::new(Arc::new(create_test_config()), None)
            .await
            .unwrap(),
    );

    let health_service1 = Arc::new(HealthService::new(
        object_service1,
        function_service1.clone(),
    ));
    let health_service2 = Arc::new(HealthService::new(
        object_service2,
        function_service2.clone(),
    ));

    let (object_client1, invocation_client1, execution_client1) =
        create_dummy_grpc_clients(config1.grpc.addr);
    let (object_client2, invocation_client2, execution_client2) =
        create_dummy_grpc_clients(config2.grpc.addr);
    let _gateway1 = HttpGateway::new(
        config1,
        health_service1,
        function_service1,
        object_client1,
        invocation_client1,
        execution_client1,
    );
    let _gateway2 = HttpGateway::new(
        config2,
        health_service2,
        function_service2,
        object_client2,
        invocation_client2,
        execution_client2,
    );

    // Both gateways should be created successfully / 两个网关都应该成功创建
}

#[cfg(test)]
mod request_body_tests {
    use super::*;
    use base64::{engine::general_purpose, Engine as _};

    #[test]
    fn test_put_object_body_serialization() {
        // Test PutObjectBody serialization / 测试PutObjectBody序列化
        let test_data = b"test data";
        let encoded_data = general_purpose::STANDARD.encode(test_data);

        let mut metadata = HashMap::new();
        metadata.insert("type".to_string(), "test".to_string());

        let body = json!({
            "value": encoded_data,
            "metadata": metadata,
            "overwrite": true
        });

        // Should be valid JSON / 应该是有效的JSON
        assert!(body.is_object());
        assert!(body["value"].is_string());
        assert!(body["metadata"].is_object());
        assert!(body["overwrite"].is_boolean());
    }

    #[test]
    fn test_list_objects_query_params() {
        // Test ListObjectsQuery parameters / 测试ListObjectsQuery参数
        let query_params = json!({
            "prefix": "test-",
            "limit": 10,
            "continuation_token": "token123"
        });

        // Should be valid query parameters / 应该是有效的查询参数
        assert!(query_params.is_object());
        assert_eq!(query_params["prefix"], "test-");
        assert_eq!(query_params["limit"], 10);
        assert_eq!(query_params["continuation_token"], "token123");
    }

    #[test]
    fn test_ref_count_body() {
        // Test RefCountBody structure / 测试RefCountBody结构
        let body = json!({
            "count": 5
        });

        // Should be valid ref count body / 应该是有效的引用计数体
        assert!(body.is_object());
        assert_eq!(body["count"], 5);
    }

    #[test]
    fn test_delete_object_query() {
        // Test DeleteObjectQuery parameters / 测试DeleteObjectQuery参数
        let query = json!({
            "force": true
        });

        // Should be valid delete query / 应该是有效的删除查询
        assert!(query.is_object());
        assert_eq!(query["force"], true);
    }

    #[test]
    fn test_base64_encoding_decoding() {
        // Test Base64 encoding/decoding for object values / 测试对象值的Base64编码/解码
        let original_data = b"Hello, World! This is test data.";
        let encoded = general_purpose::STANDARD.encode(original_data);
        let decoded = general_purpose::STANDARD.decode(&encoded).unwrap();

        assert_eq!(original_data, decoded.as_slice());
    }

    #[test]
    fn test_empty_metadata() {
        // Test empty metadata handling / 测试空元数据处理
        let body = json!({
            "value": general_purpose::STANDARD.encode(b"test"),
            "metadata": {},
            "overwrite": false
        });

        // Should handle empty metadata / 应该处理空元数据
        assert!(body["metadata"].is_object());
        assert_eq!(body["metadata"].as_object().unwrap().len(), 0);
    }

    #[test]
    fn test_optional_fields() {
        // Test optional fields in request bodies / 测试请求体中的可选字段
        let minimal_body = json!({
            "value": general_purpose::STANDARD.encode(b"test")
        });

        // Should work with minimal required fields / 应该使用最少必需字段工作
        assert!(minimal_body.is_object());
        assert!(minimal_body["value"].is_string());
        assert!(minimal_body["metadata"].is_null());
        assert!(minimal_body["overwrite"].is_null());
    }
}

// Tests for new function, task, and monitoring endpoints / 新的function、task和monitoring端点的测试
#[cfg(test)]
mod new_endpoints_tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::{Method, Request, StatusCode};
    use base64::{engine::general_purpose, Engine as _};
    use tonic::transport::{Endpoint, Server};
    use tonic::{Request as TonicRequest, Response as TonicResponse, Status};
    use tower::ServiceExt;

    use crate::proto::spearlet::execution_service_server::{
        ExecutionService, ExecutionServiceServer,
    };
    use crate::proto::spearlet::invocation_service_server::{
        InvocationService, InvocationServiceServer,
    };
    use crate::proto::spearlet::{
        Execution, ExecutionStatus, GetExecutionRequest, InvokeRequest, InvokeResponse,
        ListExecutionsRequest, ListExecutionsResponse, Payload, TerminateExecutionRequest,
        TerminateExecutionResponse,
    };
    use crate::spearlet::execution::artifact::{InvocationType, ResourceLimits};
    use crate::spearlet::execution::task::{
        HealthCheckConfig, ScalingConfig, TaskSpec, TaskType, TimeoutConfig,
    };
    use crate::spearlet::execution::RuntimeType;

    async fn create_router_for_task_monitoring() -> Router {
        let config = Arc::new(create_test_config());
        let object_service = Arc::new(ObjectServiceImpl::new_with_memory(1024 * 1024));
        let function_service = Arc::new(
            FunctionServiceImpl::new(config.clone(), None)
                .await
                .unwrap(),
        );
        let health_service = Arc::new(HealthService::new(object_service, function_service.clone()));

        let mgr = function_service.get_execution_manager();
        let artifact = mgr
            .ensure_artifact_with_id(
                "artifact-1".to_string(),
                crate::spearlet::execution::artifact::ArtifactSpec {
                    name: "a1".to_string(),
                    version: "v1".to_string(),
                    description: None,
                    runtime_type: RuntimeType::Process,
                    runtime_config: HashMap::new(),
                    location: None,
                    checksum_sha256: None,
                    environment: HashMap::new(),
                    resource_limits: ResourceLimits::default(),
                    invocation_type: InvocationType::ExistingTask,
                    max_execution_timeout_ms: 30_000,
                    labels: HashMap::new(),
                },
            )
            .unwrap();

        let task_spec = TaskSpec {
            name: "fn1".to_string(),
            task_type: TaskType::HttpHandler,
            runtime_type: RuntimeType::Process,
            entry_point: "main".to_string(),
            handler_config: HashMap::new(),
            task_config: HashMap::new(),
            environment: HashMap::new(),
            invocation_type: InvocationType::ExistingTask,
            min_instances: 0,
            max_instances: 1,
            target_concurrency: 1,
            scaling_config: ScalingConfig::default(),
            health_check: HealthCheckConfig::default(),
            timeout_config: TimeoutConfig::default(),
        };

        let _ = mgr
            .ensure_task_with_id("task-1".to_string(), &artifact, task_spec)
            .unwrap();

        let channel = Endpoint::from_static("http://127.0.0.1:50051").connect_lazy();
        let state = crate::spearlet::http_gateway::new_app_state(
            crate::proto::spearlet::object_service_client::ObjectServiceClient::new(
                channel.clone(),
            ),
            crate::proto::spearlet::invocation_service_client::InvocationServiceClient::new(
                channel.clone(),
            ),
            crate::proto::spearlet::execution_service_client::ExecutionServiceClient::new(channel),
            health_service,
            function_service,
            config,
        );
        crate::spearlet::http_gateway::build_router(state, true)
    }

    #[derive(Clone)]
    struct FakeFunctionGrpc;

    #[tonic::async_trait]
    impl InvocationService for FakeFunctionGrpc {
        async fn invoke(
            &self,
            request: TonicRequest<InvokeRequest>,
        ) -> Result<TonicResponse<InvokeResponse>, Status> {
            let req = request.into_inner();
            Ok(TonicResponse::new(InvokeResponse {
                invocation_id: if req.invocation_id.is_empty() {
                    "inv-1".to_string()
                } else {
                    req.invocation_id
                },
                execution_id: if req.execution_id.is_empty() {
                    "exec-1".to_string()
                } else {
                    req.execution_id
                },
                instance_id: "inst-1".to_string(),
                status: ExecutionStatus::Completed as i32,
                output: Some(Payload {
                    content_type: "application/octet-stream".to_string(),
                    data: b"ok".to_vec(),
                }),
                error: None,
                started_at: None,
                completed_at: None,
            }))
        }
    }

    #[tonic::async_trait]
    impl ExecutionService for FakeFunctionGrpc {
        async fn get_execution(
            &self,
            request: TonicRequest<GetExecutionRequest>,
        ) -> Result<TonicResponse<Execution>, Status> {
            let req = request.into_inner();
            if req.execution_id == "missing" {
                return Err(Status::not_found("execution not found"));
            }
            Ok(TonicResponse::new(Execution {
                invocation_id: "inv-1".to_string(),
                execution_id: req.execution_id,
                task_id: "task-1".to_string(),
                function_name: "fn1".to_string(),
                instance_id: "inst-1".to_string(),
                status: ExecutionStatus::Running as i32,
                output: Some(Payload {
                    content_type: "application/octet-stream".to_string(),
                    data: if req.include_output {
                        b"out".to_vec()
                    } else {
                        Vec::new()
                    },
                }),
                error: None,
                started_at: None,
                completed_at: None,
            }))
        }

        async fn terminate_execution(
            &self,
            request: TonicRequest<TerminateExecutionRequest>,
        ) -> Result<TonicResponse<TerminateExecutionResponse>, Status> {
            let req = request.into_inner();
            Ok(TonicResponse::new(TerminateExecutionResponse {
                success: true,
                final_status: ExecutionStatus::Terminated as i32,
                message: format!("terminated {}", req.execution_id),
            }))
        }

        async fn list_executions(
            &self,
            _request: TonicRequest<ListExecutionsRequest>,
        ) -> Result<TonicResponse<ListExecutionsResponse>, Status> {
            Ok(TonicResponse::new(ListExecutionsResponse {
                executions: Vec::new(),
                next_page_token: String::new(),
            }))
        }
    }

    pub(super) async fn create_router_with_fake_grpc() -> Router {
        let config = Arc::new(create_test_config());
        let object_service = Arc::new(ObjectServiceImpl::new_with_memory(1024 * 1024));
        let function_service = Arc::new(
            FunctionServiceImpl::new(config.clone(), None)
                .await
                .unwrap(),
        );
        let health_service = Arc::new(HealthService::new(object_service, function_service.clone()));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            Server::builder()
                .add_service(InvocationServiceServer::new(FakeFunctionGrpc))
                .add_service(ExecutionServiceServer::new(FakeFunctionGrpc))
                .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
                .await
                .unwrap();
        });

        let channel = Endpoint::from_shared(format!("http://{}", addr))
            .unwrap()
            .connect()
            .await
            .unwrap();

        let state = crate::spearlet::http_gateway::new_app_state(
            crate::proto::spearlet::object_service_client::ObjectServiceClient::new(
                channel.clone(),
            ),
            crate::proto::spearlet::invocation_service_client::InvocationServiceClient::new(
                channel.clone(),
            ),
            crate::proto::spearlet::execution_service_client::ExecutionServiceClient::new(channel),
            health_service,
            function_service,
            config,
        );
        crate::spearlet::http_gateway::build_router(state, true)
    }

    #[tokio::test]
    async fn test_execute_function_endpoint_success() {
        let router = create_router_with_fake_grpc().await;

        let request = Request::builder()
            .method(Method::POST)
            .uri("/functions/execute")
            .header("Content-Type", "application/json")
            .body(Body::from(
                r#"{"task_id":"task-1","mode":"sync","input_base64":"aGVsbG8="}"#,
            ))
            .unwrap();

        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert!(json["success"].as_bool().unwrap());
        assert_eq!(json["status"], "COMPLETED");
        assert_eq!(
            json["output_base64"],
            general_purpose::STANDARD.encode(b"ok")
        );
    }

    #[tokio::test]
    async fn test_execute_function_endpoint_invalid_json() {
        let router = create_router_with_fake_grpc().await;

        let request = Request::builder()
            .method(Method::POST)
            .uri("/functions/execute")
            .header("Content-Type", "application/json")
            .body(Body::from("invalid json"))
            .unwrap();

        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_execute_function_endpoint_missing_task_id() {
        let router = create_router_with_fake_grpc().await;

        let request = Request::builder()
            .method(Method::POST)
            .uri("/functions/execute")
            .header("Content-Type", "application/json")
            .body(Body::from(r#"{"mode":"sync"}"#))
            .unwrap();

        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_get_execution_status_endpoint_success() {
        let router = create_router_with_fake_grpc().await;

        let request = Request::builder()
            .method(Method::GET)
            .uri("/functions/executions/exec-123?include_output=true")
            .body(Body::empty())
            .unwrap();

        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["execution_id"], "exec-123");
        assert_eq!(json["status"], "RUNNING");
        assert_eq!(
            json["output_base64"],
            general_purpose::STANDARD.encode(b"out")
        );
    }

    #[tokio::test]
    async fn test_get_execution_status_endpoint_not_found() {
        let router = create_router_with_fake_grpc().await;

        let request = Request::builder()
            .method(Method::GET)
            .uri("/functions/executions/missing")
            .body(Body::empty())
            .unwrap();

        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_cancel_execution_endpoint_success() {
        let router = create_router_with_fake_grpc().await;

        let request = Request::builder()
            .method(Method::POST)
            .uri("/functions/executions/exec-123/cancel")
            .header("Content-Type", "application/json")
            .body(Body::from(r#"{"reason":"test"}"#))
            .unwrap();

        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert!(json["success"].as_bool().unwrap());
        assert_eq!(json["final_status"], "TERMINATED");
    }

    #[tokio::test]
    async fn test_list_tasks_endpoint_includes_created_task() {
        let router = create_router_for_task_monitoring().await;

        let request = Request::builder()
            .method(Method::GET)
            .uri("/tasks")
            .body(Body::empty())
            .unwrap();

        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert!(json["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["task_id"] == "task-1"));
        assert_eq!(json["total"], 1);
    }

    #[tokio::test]
    async fn test_get_task_endpoint_not_found() {
        let router = create_router_for_task_monitoring().await;

        let request = Request::builder()
            .method(Method::GET)
            .uri("/tasks/does-not-exist")
            .body(Body::empty())
            .unwrap();

        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_get_task_executions_endpoint_empty() {
        let router = create_router_for_task_monitoring().await;

        let request = Request::builder()
            .method(Method::GET)
            .uri("/tasks/task-1/executions")
            .body(Body::empty())
            .unwrap();

        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["task_id"], "task-1");
        assert!(json["executions"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_get_stats_and_health_endpoints() {
        let router = create_router_for_task_monitoring().await;

        let resp = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/monitoring/stats")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["task_count"], 1);

        let resp = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/monitoring/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert!(json["timestamp"].is_string());
        assert_eq!(json["details"]["task_count"], 1);
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::spearlet::execution::hostcall::fd_table::EP_CTL_ADD;
    use crate::spearlet::execution::hostcall::types::PollEvents;
    use futures::{SinkExt, StreamExt};

    #[tokio::test]
    async fn test_gateway_lifecycle() {
        // Test complete HTTP gateway lifecycle / 测试完整的HTTP网关生命周期
        let config = Arc::new(create_test_config());
        let object_service = Arc::new(ObjectServiceImpl::new_with_memory(1024 * 1024));
        let function_service = Arc::new(
            FunctionServiceImpl::new(Arc::new(create_test_config()), None)
                .await
                .unwrap(),
        );
        let health_service = Arc::new(HealthService::new(
            object_service.clone(),
            function_service.clone(),
        ));

        // Create gateway / 创建网关
        let (object_client, invocation_client, execution_client) =
            create_dummy_grpc_clients(config.grpc.addr);
        let _gateway = HttpGateway::new(
            config.clone(),
            health_service.clone(),
            function_service,
            object_client,
            invocation_client,
            execution_client,
        );

        // Verify initial state / 验证初始状态
        let stats = object_service.get_stats().await;
        assert_eq!(stats.object_count, 0);
        assert_eq!(stats.total_size, 0);
        assert_eq!(stats.pinned_count, 0);

        // Verify health service / 验证健康服务
        let health_status = health_service.get_health_status().await;
        assert_eq!(health_status.status, "healthy");
        assert_eq!(health_status.object_count, 0);

        // Note: We don't start the actual server to avoid port binding issues
        // 注意：我们不启动实际服务器以避免端口绑定问题
    }

    #[tokio::test]
    async fn test_gateway_with_different_configs() {
        // Test gateway with various configurations / 测试各种配置的网关
        let configs = vec![
            SpearletConfig {
                http: HttpConfig {
                    server: ServerConfig {
                        addr: "127.0.0.1:8080".parse().unwrap(),
                        ..Default::default()
                    },
                    cors_enabled: true,
                    swagger_enabled: true,
                },
                grpc: ServerConfig {
                    addr: "127.0.0.1:9090".parse().unwrap(),
                    ..Default::default()
                },
                storage: StorageConfig {
                    backend: "memory".to_string(),
                    data_dir: "/tmp/test".to_string(),
                    max_cache_size_mb: 100,
                    compression_enabled: false,
                    max_object_size: 1024 * 1024,
                },
                ..Default::default()
            },
            SpearletConfig {
                http: HttpConfig {
                    server: ServerConfig {
                        addr: "0.0.0.0:3000".parse().unwrap(),
                        ..Default::default()
                    },
                    cors_enabled: false,
                    swagger_enabled: false,
                },
                grpc: ServerConfig {
                    addr: "0.0.0.0:3001".parse().unwrap(),
                    ..Default::default()
                },
                storage: StorageConfig {
                    backend: "memory".to_string(),
                    data_dir: "/tmp/test".to_string(),
                    max_cache_size_mb: 100,
                    compression_enabled: false,
                    max_object_size: 10 * 1024 * 1024,
                },
                ..Default::default()
            },
        ];

        for config in configs {
            let object_service = Arc::new(ObjectServiceImpl::new_with_memory(
                config.storage.max_object_size,
            ));
            let function_service = Arc::new(
                FunctionServiceImpl::new(Arc::new(create_test_config()), None)
                    .await
                    .unwrap(),
            );
            let health_service =
                Arc::new(HealthService::new(object_service, function_service.clone()));
            let config = Arc::new(config);
            let (object_client, invocation_client, execution_client) =
                create_dummy_grpc_clients(config.grpc.addr);
            let _gateway = HttpGateway::new(
                config,
                health_service,
                function_service,
                object_client,
                invocation_client,
                execution_client,
            );

            // Each gateway should be created successfully / 每个网关都应该成功创建
        }
    }

    #[tokio::test]
    async fn test_user_stream_ws_end_to_end() {
        let router = super::new_endpoints_tests::create_router_with_fake_grpc().await;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });

        let exec_id = "exec-ws-e2e";
        let url = format!("ws://{}/api/v1/executions/{}/streams/ws", addr, exec_id);
        let (mut ws, _) = tokio_tungstenite::connect_async(url).await.unwrap();

        let api = crate::spearlet::execution::host_api::DefaultHostApi::new(
            crate::spearlet::execution::runtime::RuntimeConfig {
                runtime_type: crate::spearlet::execution::runtime::RuntimeType::Wasm,
                settings: HashMap::new(),
                global_environment: HashMap::new(),
                spearlet_config: None,
                resource_pool: crate::spearlet::execution::runtime::ResourcePoolConfig::default(),
            },
        );

        crate::spearlet::execution::host_api::set_current_wasm_execution_id(Some(
            exec_id.to_string(),
        ));

        let epfd = api.spear_ep_create();
        let in_fd = api.user_stream_open(1, 1);
        assert_eq!(
            api.spear_ep_ctl(epfd, EP_CTL_ADD, in_fd, PollEvents::IN.bits() as i32),
            0
        );

        let inbound =
            crate::spearlet::execution::host_api::ssf::build_ssf_v1_frame(1, 2, b"{}", b"hello");
        ws.send(tokio_tungstenite::tungstenite::Message::Binary(
            inbound.clone(),
        ))
        .await
        .unwrap();

        let api2 = api.clone();
        let ready = tokio::task::spawn_blocking(move || api2.spear_ep_wait_ready(epfd, 500))
            .await
            .unwrap()
            .unwrap();
        assert!(ready
            .iter()
            .any(|(rfd, ev)| *rfd == in_fd && ((*ev as u32) & PollEvents::IN.bits()) != 0));

        let got = api.user_stream_read(in_fd).unwrap();
        assert_eq!(got, inbound);

        let out_fd = api.user_stream_open(1, 2);
        let outbound =
            crate::spearlet::execution::host_api::ssf::build_ssf_v1_frame(1, 2, b"{}", b"world");
        assert_eq!(api.user_stream_write(out_fd, &outbound), 0);

        let msg = ws.next().await.unwrap().unwrap();
        match msg {
            tokio_tungstenite::tungstenite::Message::Binary(b) => {
                assert_eq!(b, outbound);
            }
            other => panic!("unexpected ws message: {other:?}"),
        }

        crate::spearlet::execution::host_api::set_current_wasm_execution_id(None);
    }
}
