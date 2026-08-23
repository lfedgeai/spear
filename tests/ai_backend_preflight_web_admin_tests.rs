use axum::{routing::post, Json, Router};
use axum_test::TestServer;
use serde_json::json;
use spear_next::proto::sms::{
    admin_credential_service_server::AdminCredentialServiceServer,
    ai_backend_control_plane_service_client::AiBackendControlPlaneServiceClient,
    ai_backend_control_plane_service_server::AiBackendControlPlaneServiceServer,
    backend_registry_service_client::BackendRegistryServiceClient,
    backend_registry_service_server::BackendRegistryServiceServer,
    node_service_client::NodeServiceClient, node_service_server::NodeServiceServer, Node,
    RegisterNodeRequest,
};
use spear_next::sms::config::SmsConfig;
use spear_next::sms::gateway::GatewayState;
use spear_next::sms::service::SmsServiceImpl;
use spear_next::sms::web_admin::create_admin_router;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tonic::transport::Server;
use uuid::Uuid;

async fn start_sms_grpc() -> (tokio::task::JoinHandle<()>, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let sms_service =
        SmsServiceImpl::with_storage_config(&spear_next::config::base::StorageConfig {
            backend: "memory".to_string(),
            ..Default::default()
        })
        .await;

    let handle = tokio::spawn(async move {
        Server::builder()
            .add_service(NodeServiceServer::new(sms_service.clone()))
            .add_service(BackendRegistryServiceServer::new(sms_service.clone()))
            .add_service(AiBackendControlPlaneServiceServer::new(sms_service.clone()))
            .add_service(AdminCredentialServiceServer::new(sms_service))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });

    (handle, format!("http://{}", addr))
}

async fn create_admin_test_server(
    grpc_url: &str,
) -> (
    TestServer,
    NodeServiceClient<tonic::transport::Channel>,
    BackendRegistryServiceClient<tonic::transport::Channel>,
) {
    let channel = tonic::transport::Channel::from_shared(grpc_url.to_string())
        .unwrap()
        .connect()
        .await
        .unwrap();

    let state = GatewayState {
        config: Arc::new(SmsConfig::default()),
        node_client: NodeServiceClient::new(channel.clone()),
        task_client: spear_next::proto::sms::task_service_client::TaskServiceClient::new(
            channel.clone(),
        ),
        task_assignment_client: spear_next::proto::sms::task_placement_assignment_service_client::TaskPlacementAssignmentServiceClient::new(channel.clone()),
        placement_client: spear_next::proto::sms::placement_service_client::PlacementServiceClient::new(
            channel.clone(),
        ),
        instance_registry_client: spear_next::proto::sms::instance_registry_service_client::InstanceRegistryServiceClient::new(
            channel.clone(),
        ),
        execution_registry_client: spear_next::proto::sms::execution_registry_service_client::ExecutionRegistryServiceClient::new(
            channel.clone(),
        ),
        execution_index_client: spear_next::proto::sms::execution_index_service_client::ExecutionIndexServiceClient::new(
            channel.clone(),
        ),
        mcp_registry_client: spear_next::proto::sms::mcp_registry_service_client::McpRegistryServiceClient::new(
            channel.clone(),
        ),
        backend_registry_client: BackendRegistryServiceClient::new(channel.clone()),
        ai_backend_control_plane_client: AiBackendControlPlaneServiceClient::new(channel.clone()),
        admin_credential_client:
            spear_next::proto::sms::admin_credential_service_client::AdminCredentialServiceClient::new(
                channel.clone(),
            ),
        stream_sessions: spear_next::sms::gateway::StreamSessionStore::new(),
        execution_stream_pool: spear_next::sms::gateway::ExecutionStreamPool::new(),
        cancel_token: CancellationToken::new(),
        max_upload_bytes: 64 * 1024 * 1024,
        files_dir: std::env::temp_dir()
            .join(format!("spear-sms-files-{}", Uuid::new_v4()))
            .to_string_lossy()
            .to_string(),
    };

    let app = create_admin_router(state);
    let server = TestServer::new(app.into_make_service()).unwrap();

    (
        server,
        NodeServiceClient::new(channel.clone()),
        BackendRegistryServiceClient::new(channel.clone()),
    )
}

fn remote_openai_model_access_verified_payload() -> serde_json::Value {
    json!({
        "success": true,
        "result": {
            "provider_family": "open_ai_compatible",
            "outcome": {
                "outcome_kind": "model_access_verified",
                "resolved_endpoint": "https://api.openai.com/v1/models",
                "http_status": 200,
                "checks": [
                    {"name": "connectivity", "ok": true},
                    {"name": "auth", "ok": true},
                    {"name": "model_access", "ok": true}
                ],
                "auth_valid": true,
                "model_accessible": true
            }
        },
        "message": "authentication and model access verified",
        "latency_ms": 42,
    })
}

fn remote_openai_auth_failed_payload() -> serde_json::Value {
    json!({
        "success": false,
        "result": {
            "provider_family": "open_ai_compatible",
            "outcome": {
                "outcome_kind": "auth_failed",
                "error_code": "auth_invalid",
                "resolved_endpoint": "https://api.openai.com/v1/models",
                "http_status": 401,
                "provider_code": "invalid_api_key",
                "checks": [
                    {"name": "connectivity", "ok": true},
                    {"name": "auth", "ok": false},
                    {"name": "model_access", "ok": false}
                ],
                "auth_valid": false
            }
        },
        "message": "invalid api key",
        "latency_ms": 17,
    })
}

async fn start_spearlet_preflight_server(
    remote_payload: serde_json::Value,
) -> (tokio::task::JoinHandle<()>, u16) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let app = Router::new()
        .route(
            "/internal/ai/local-models/preflight",
            post(|| async {
                Json(json!({
                    "success": true,
                    "details": {
                        "source_kind": "model_url",
                        "effective_model_path": "/var/lib/spear/local_models/llamacpp/models/qwen.gguf",
                        "final_url": "https://models.example.com/qwen.gguf",
                        "http_status": 200,
                        "content_length": 1024
                    },
                    "message": "model_url is reachable from the node"
                }))
            }),
        )
        .route(
            "/internal/ai/backends/preflight",
            post(move || {
                let remote_payload = remote_payload.clone();
                async move { Json(remote_payload) }
            }),
        );
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (handle, port)
}

#[tokio::test]
async fn test_admin_ai_backend_preflight_proxies_to_target_node() {
    let (grpc_handle, grpc_url) = start_sms_grpc().await;
    let (server, mut node_client, _backend_registry) = create_admin_test_server(&grpc_url).await;
    let (spearlet_handle, http_port) =
        start_spearlet_preflight_server(remote_openai_model_access_verified_payload()).await;

    let response = node_client
        .register_node(RegisterNodeRequest {
            node: Some(Node {
                uuid: "node-preflight-1".to_string(),
                ip_address: "127.0.0.1".to_string(),
                port: 50052,
                http_port: http_port as i32,
                status: "online".to_string(),
                last_heartbeat: 0,
                registered_at: 0,
                metadata: HashMap::new(),
            }),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(response.success);

    let body: serde_json::Value = server
        .post("/admin/api/ai-backends/preflight")
        .json(&json!({
            "backend": {
                "display_name": "Local llama.cpp",
                "provider": "llamacpp",
                "model": "qwen",
                "hosting": "local",
                "backend_kind": "llamacpp",
                "spec": {
                    "base_url": "",
                    "operations": ["chat_completions"],
                    "features": [],
                    "transports": ["http"],
                    "weight": 100,
                    "priority": 0
                },
                "local": {
                    "provider_family": "llama_cpp",
                    "config": {
                        "model_url": "https://models.example.com/qwen.gguf",
                        "skip_download": false,
                        "download_timeout_s": 30
                    }
                }
            },
            "node_uuids": ["node-preflight-1"]
        }))
        .await
        .json();

    assert!(body["success"].as_bool().unwrap());
    let results = body["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["node_uuid"], "node-preflight-1");
    assert_eq!(results[0]["success"], true);
    assert_eq!(results[0]["details"]["kind"], "local_model");
    assert_eq!(results[0]["details"]["source"]["source_kind"], "model_url");
    assert_eq!(body["message"], serde_json::Value::Null);

    spearlet_handle.abort();
    grpc_handle.abort();
}

#[tokio::test]
async fn test_admin_ai_backend_preflight_supports_remote_backends() {
    let (grpc_handle, grpc_url) = start_sms_grpc().await;
    let (server, mut node_client, _backend_registry) = create_admin_test_server(&grpc_url).await;
    let (spearlet_handle, http_port) =
        start_spearlet_preflight_server(remote_openai_model_access_verified_payload()).await;

    let response = node_client
        .register_node(RegisterNodeRequest {
            node: Some(Node {
                uuid: "node-preflight-remote-1".to_string(),
                ip_address: "127.0.0.1".to_string(),
                port: 50052,
                http_port: http_port as i32,
                status: "online".to_string(),
                last_heartbeat: 0,
                registered_at: 0,
                metadata: HashMap::new(),
            }),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(response.success);

    let body: serde_json::Value = server
        .post("/admin/api/ai-backends/preflight")
        .json(&json!({
            "backend": {
                "display_name": "Remote OpenAI",
                "provider": "openai",
                "model": "gpt-4o-mini",
                "hosting": "remote",
                "backend_kind": "openai_chat_completion",
                "remote": {
                    "provider_family": "open_ai_compatible",
                    "config": {
                        "base_url": "https://api.openai.com/v1",
                        "credential_ref": "openai-prod",
                        "operations": ["chat_completions"],
                        "features": [],
                        "transports": ["http"]
                    }
                },
                "spec": {
                    "base_url": "",
                    "operations": [],
                    "features": [],
                    "transports": [],
                    "weight": 100,
                    "priority": 0
                },
                "metadata": {}
            },
            "node_uuids": ["node-preflight-remote-1"],
            "verification_policy": "single_node_strict",
            "requested_checks": ["connectivity", "auth", "model_access"]
        }))
        .await
        .json();

    assert!(body["success"].as_bool().unwrap());
    let results = body["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["node_uuid"], "node-preflight-remote-1");
    assert_eq!(results[0]["success"], true);
    assert_eq!(results[0]["details"]["kind"], "remote_provider");
    assert_eq!(
        results[0]["details"]["result"]["provider_family"],
        "open_ai_compatible"
    );
    assert_eq!(
        results[0]["details"]["result"]["outcome"]["outcome_kind"],
        "model_access_verified"
    );
    assert_eq!(results[0]["details"]["result"]["outcome"]["auth_valid"], true);
    assert_eq!(
        results[0]["details"]["result"]["outcome"]["model_accessible"],
        true
    );

    spearlet_handle.abort();
    grpc_handle.abort();
}

#[tokio::test]
async fn test_admin_ai_backend_preflight_surfaces_openai_auth_failure_shape() {
    let (grpc_handle, grpc_url) = start_sms_grpc().await;
    let (server, mut node_client, _backend_registry) = create_admin_test_server(&grpc_url).await;
    let (spearlet_handle, http_port) =
        start_spearlet_preflight_server(remote_openai_auth_failed_payload()).await;

    let response = node_client
        .register_node(RegisterNodeRequest {
            node: Some(Node {
                uuid: "node-preflight-remote-failure-1".to_string(),
                ip_address: "127.0.0.1".to_string(),
                port: 50052,
                http_port: http_port as i32,
                status: "online".to_string(),
                last_heartbeat: 0,
                registered_at: 0,
                metadata: HashMap::new(),
            }),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(response.success);

    let body: serde_json::Value = server
        .post("/admin/api/ai-backends/preflight")
        .json(&json!({
            "backend": {
                "display_name": "Remote OpenAI",
                "provider": "openai",
                "model": "gpt-4o-mini",
                "hosting": "remote",
                "backend_kind": "openai_chat_completion",
                "remote": {
                    "provider_family": "open_ai_compatible",
                    "config": {
                        "base_url": "https://api.openai.com/v1",
                        "credential_ref": "openai-prod",
                        "operations": ["chat_completions"],
                        "features": [],
                        "transports": ["http"]
                    }
                },
                "spec": {
                    "base_url": "",
                    "operations": [],
                    "features": [],
                    "transports": [],
                    "weight": 100,
                    "priority": 0
                },
                "metadata": {}
            },
            "node_uuids": ["node-preflight-remote-failure-1"],
            "verification_policy": "single_node_strict",
            "requested_checks": ["connectivity", "auth", "model_access"]
        }))
        .await
        .json();

    assert!(!body["success"].as_bool().unwrap());
    let results = body["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["node_uuid"], "node-preflight-remote-failure-1");
    assert_eq!(results[0]["success"], false);
    assert_eq!(results[0]["details"]["kind"], "remote_provider");
    assert_eq!(
        results[0]["details"]["result"]["provider_family"],
        "open_ai_compatible"
    );
    assert_eq!(
        results[0]["details"]["result"]["outcome"]["outcome_kind"],
        "auth_failed"
    );
    assert_eq!(results[0]["details"]["result"]["outcome"]["error_code"], "auth_invalid");
    assert_eq!(results[0]["details"]["result"]["outcome"]["auth_valid"], false);

    spearlet_handle.abort();
    grpc_handle.abort();
}

#[tokio::test]
async fn test_admin_ai_backend_preflight_requires_structured_local_input_for_llamacpp() {
    let (grpc_handle, grpc_url) = start_sms_grpc().await;
    let (server, _node_client, _backend_registry) = create_admin_test_server(&grpc_url).await;

    let body: serde_json::Value = server
        .post("/admin/api/ai-backends/preflight")
        .json(&json!({
            "backend": {
                "display_name": "Local llama.cpp",
                "provider": "llamacpp",
                "model": "qwen",
                "hosting": "local",
                "backend_kind": "llamacpp",
                "spec": {
                    "base_url": "http://127.0.0.1:8080/v1",
                    "operations": ["chat_completions"],
                    "features": [],
                    "transports": ["http"],
                    "weight": 100,
                    "priority": 0
                },
                "metadata": {
                    "model_path": "/models/qwen.gguf"
                }
            },
            "node_uuids": ["node-preflight-local-required-1"]
        }))
        .await
        .json();

    assert!(!body["success"].as_bool().unwrap());
    assert_eq!(body["results"].as_array().unwrap().len(), 0);
    assert_eq!(
        body["message"],
        "local provider-specific input is required for backend provider llamacpp"
    );

    grpc_handle.abort();
}

#[tokio::test]
async fn test_admin_ai_backend_preflight_requires_structured_remote_input_for_openai() {
    let (grpc_handle, grpc_url) = start_sms_grpc().await;
    let (server, _node_client, _backend_registry) = create_admin_test_server(&grpc_url).await;

    let body: serde_json::Value = server
        .post("/admin/api/ai-backends/preflight")
        .json(&json!({
            "backend": {
                "display_name": "Remote OpenAI",
                "provider": "openai",
                "model": "gpt-4o-mini",
                "hosting": "remote",
                "backend_kind": "openai_chat_completion",
                "credential_ref": "openai-prod",
                "spec": {
                    "base_url": "https://api.openai.com/v1",
                    "operations": ["chat_completions"],
                    "features": [],
                    "transports": ["http"],
                    "weight": 100,
                    "priority": 0
                },
                "metadata": {}
            },
            "node_uuids": ["node-preflight-remote-required-1"]
        }))
        .await
        .json();

    assert!(!body["success"].as_bool().unwrap());
    assert_eq!(body["results"].as_array().unwrap().len(), 0);
    assert_eq!(
        body["message"],
        "remote provider-specific input is required for backend provider openai"
    );

    grpc_handle.abort();
}

#[tokio::test]
async fn test_admin_ai_backend_preflight_rejects_duplicate_remote_openai_sources() {
    let (grpc_handle, grpc_url) = start_sms_grpc().await;
    let (server, _node_client, _backend_registry) = create_admin_test_server(&grpc_url).await;

    let body: serde_json::Value = server
        .post("/admin/api/ai-backends/preflight")
        .json(&json!({
            "backend": {
                "display_name": "Remote OpenAI",
                "provider": "openai",
                "model": "gpt-4o-mini",
                "hosting": "remote",
                "backend_kind": "openai_chat_completion",
                "credential_ref": "legacy-openai",
                "remote": {
                    "provider_family": "open_ai_compatible",
                    "config": {
                        "base_url": "https://api.openai.com/v1",
                        "credential_ref": "openai-prod",
                        "operations": ["chat_completions"],
                        "features": [],
                        "transports": ["http"]
                    }
                },
                "spec": {
                    "base_url": "https://legacy.example.com/v1",
                    "operations": ["chat_completions"],
                    "features": ["stream"],
                    "transports": ["http"],
                    "weight": 100,
                    "priority": 0
                },
                "metadata": {}
            },
            "node_uuids": ["node-preflight-remote-dup-1"]
        }))
        .await
        .json();

    assert!(!body["success"].as_bool().unwrap());
    assert_eq!(body["results"].as_array().unwrap().len(), 0);
    assert_eq!(
        body["message"],
        "remote openai structured input cannot be combined with legacy fields: spec.base_url, credential_ref, spec.operations, spec.features, spec.transports"
    );

    grpc_handle.abort();
}

#[tokio::test]
async fn test_admin_ai_backend_preflight_rejects_unsupported_provider() {
    let (grpc_handle, grpc_url) = start_sms_grpc().await;
    let (server, _node_client, _backend_registry) = create_admin_test_server(&grpc_url).await;

    let body: serde_json::Value = server
        .post("/admin/api/ai-backends/preflight")
        .json(&json!({
            "backend": {
                "display_name": "Custom Remote",
                "provider": "anthropic-compatible",
                "model": "claude-compatible",
                "hosting": "remote",
                "backend_kind": "custom_http",
                "credential_ref": "custom-secret",
                "spec": {
                    "base_url": "https://custom.example.com/v1",
                    "operations": ["chat_completions"],
                    "features": ["supports_tools"],
                    "transports": ["http"],
                    "weight": 100,
                    "priority": 0
                },
                "metadata": {
                    "tenant": "lab"
                }
            },
            "node_uuids": ["node-preflight-custom-1"]
        }))
        .await
        .json();

    assert!(!body["success"].as_bool().unwrap());
    assert_eq!(body["results"].as_array().unwrap().len(), 0);
    assert_eq!(
        body["message"],
        "unsupported backend provider: anthropic-compatible"
    );

    grpc_handle.abort();
}
