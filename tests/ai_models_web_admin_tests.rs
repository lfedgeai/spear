use axum_test::TestServer;
use spear_next::proto::sms::{
    ai_backend_control_plane_service_client::AiBackendControlPlaneServiceClient,
    ai_backend_control_plane_service_server::AiBackendControlPlaneServiceServer,
    backend_registry_service_client::BackendRegistryServiceClient,
    backend_registry_service_server::BackendRegistryServiceServer,
};
use spear_next::sms::config::SmsConfig;
use spear_next::sms::gateway::GatewayState;
use spear_next::sms::service::SmsServiceImpl;
use spear_next::sms::web_admin::create_admin_router;
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
            .add_service(BackendRegistryServiceServer::new(sms_service.clone()))
            .add_service(AiBackendControlPlaneServiceServer::new(sms_service.clone()))
            .add_service(spear_next::proto::sms::admin_credential_service_server::AdminCredentialServiceServer::new(sms_service))
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
    BackendRegistryServiceClient<tonic::transport::Channel>,
) {
    let channel = tonic::transport::Channel::from_shared(grpc_url.to_string())
        .unwrap()
        .connect()
        .await
        .unwrap();

    let state = GatewayState {
        config: Arc::new(SmsConfig::default()),
        node_client: spear_next::proto::sms::node_service_client::NodeServiceClient::new(
            channel.clone(),
        ),
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

    (server, BackendRegistryServiceClient::new(channel.clone()))
}

#[tokio::test]
async fn test_admin_ai_model_views_lists_local_and_remote_models() {
    let (handle, grpc_url) = start_sms_grpc().await;
    let (server, _backend_registry) = create_admin_test_server(&grpc_url).await;

    let local_backend: serde_json::Value = server
        .post("/admin/api/ai-backends")
        .json(&serde_json::json!({
            "display_name": "Local llama.cpp",
            "provider": "llamacpp",
            "model": "llama3",
            "hosting": "local",
            "backend_kind": "llamacpp",
            "local": {
                "provider_family": "llama_cpp",
                "config": {
                    "model_path": "/models/llama3.gguf"
                }
            },
            "spec": {
                "base_url": "http://127.0.0.1:8080/v1",
                "operations": ["chat_completions"],
                "features": [],
                "transports": ["http"],
                "weight": 100,
                "priority": 0
            }
        }))
        .await
        .json();
    assert!(local_backend["success"].as_bool().unwrap());
    assert_eq!(local_backend["backend"]["local"]["provider_family"], "llama_cpp");
    assert_eq!(
        local_backend["backend"]["local"]["config"]["model_path"],
        "/models/llama3.gguf"
    );
    let local_backend_id = local_backend["backend"]["backend_id"].as_str().unwrap();

    let remote_backend: serde_json::Value = server
        .post("/admin/api/ai-backends")
        .json(&serde_json::json!({
            "display_name": "Remote OpenAI",
            "provider": "openai",
            "model": "gpt-4o",
            "hosting": "remote",
            "backend_kind": "openai_chat_completion",
            "remote": {
                "provider_family": "open_ai_compatible",
                "config": {
                    "base_url": "https://api.openai.com/v1",
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
            }
        }))
        .await
        .json();
    assert!(remote_backend["success"].as_bool().unwrap());
    assert_eq!(
        remote_backend["backend"]["remote"]["provider_family"],
        "open_ai_compatible"
    );
    assert_eq!(
        remote_backend["backend"]["remote"]["config"]["base_url"],
        "https://api.openai.com/v1"
    );
    let remote_backend_id = remote_backend["backend"]["backend_id"].as_str().unwrap();

    let local_placement: serde_json::Value = server
        .post("/admin/api/ai-backend-placements")
        .json(&serde_json::json!({
            "backend_id": local_backend_id,
            "node_uuid": "node-1"
        }))
        .await
        .json();
    assert!(local_placement["success"].as_bool().unwrap());

    let remote_placement: serde_json::Value = server
        .post("/admin/api/ai-backend-placements")
        .json(&serde_json::json!({
            "backend_id": remote_backend_id,
            "node_uuid": "node-2"
        }))
        .await
        .json();
    assert!(remote_placement["success"].as_bool().unwrap());

    let body: serde_json::Value = server.get("/admin/api/ai-model-views").await.json();
    assert!(body["success"].as_bool().unwrap());
    let views = body["views"].as_array().unwrap();
    assert!(views
        .iter()
        .any(|m| m["provider"] == "llamacpp" && m["model"] == "llama3" && m["hosting"] == "local"));
    assert!(views
        .iter()
        .any(|m| m["provider"] == "openai" && m["model"] == "gpt-4o" && m["hosting"] == "remote"));

    handle.abort();
}

#[tokio::test]
async fn test_admin_ai_backend_create_normalizes_local_metadata_shape() {
    let (handle, grpc_url) = start_sms_grpc().await;
    let (server, _backend_registry) = create_admin_test_server(&grpc_url).await;

    let body: serde_json::Value = server
        .post("/admin/api/ai-backends")
        .json(&serde_json::json!({
            "display_name": "Local llama.cpp",
            "provider": "llamacpp",
            "model": "llama3",
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
                    "model_url": " https://models.example.com/llama3.gguf ",
                    "skip_download": false,
                    "download_timeout_s": 60,
                    "threads": 8
                }
            },
            "metadata": {
                "extra_key": "preserve-me"
            }
        }))
        .await
        .json();

    assert!(body["success"].as_bool().unwrap());
    assert_eq!(
        body["backend"]["metadata"]["model_url"],
        "https://models.example.com/llama3.gguf"
    );
    assert_eq!(body["backend"]["metadata"]["skip_download"], "false");
    assert_eq!(body["backend"]["metadata"]["download_timeout_s"], "60");
    assert_eq!(body["backend"]["metadata"]["threads"], "8");
    assert_eq!(body["backend"]["metadata"]["extra_key"], "preserve-me");
    assert_eq!(body["backend"]["local"]["provider_family"], "llama_cpp");
    assert_eq!(
        body["backend"]["local"]["config"]["model_url"],
        "https://models.example.com/llama3.gguf"
    );
    assert_eq!(body["backend"]["local"]["config"]["skip_download"], false);
    assert_eq!(body["backend"]["local"]["config"]["download_timeout_s"], 60);
    assert_eq!(body["backend"]["local"]["config"]["threads"], 8);

    handle.abort();
}

#[tokio::test]
async fn test_admin_ai_model_views_include_backend_ids_and_instances() {
    let (handle, grpc_url) = start_sms_grpc().await;
    let (server, _backend_registry) = create_admin_test_server(&grpc_url).await;

    let backend: serde_json::Value = server
        .post("/admin/api/ai-backends")
        .json(&serde_json::json!({
            "display_name": "Remote OpenAI",
            "provider": "openai",
            "model": "gpt-4o",
            "hosting": "remote",
            "backend_kind": "openai_chat_completion",
            "remote": {
                "provider_family": "open_ai_compatible",
                "config": {
                    "base_url": "https://api.openai.com/v1",
                    "operations": ["chat_completions"],
                    "features": ["streaming"],
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
            }
        }))
        .await
        .json();
    assert!(backend["success"].as_bool().unwrap());
    let backend_id = backend["backend"]["backend_id"].as_str().unwrap();

    let placement: serde_json::Value = server
        .post("/admin/api/ai-backend-placements")
        .json(&serde_json::json!({
            "backend_id": backend_id,
            "node_uuid": "node-9",
            "weight_override": 80,
            "priority_override": 3
        }))
        .await
        .json();
    assert!(placement["success"].as_bool().unwrap());

    let body: serde_json::Value = server.get("/admin/api/ai-model-views").await.json();
    assert!(body["success"].as_bool().unwrap());
    let view = body["views"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["provider"] == "openai" && item["model"] == "gpt-4o")
        .cloned()
        .expect("view");
    assert_eq!(view["hosting"], "remote");
    assert!(view["backend_ids"]
        .as_array()
        .unwrap()
        .iter()
        .any(|id| id == backend_id));
    assert!(view["instances"]
        .as_array()
        .unwrap()
        .iter()
        .any(|instance| instance["node_uuid"] == "node-9"));

    handle.abort();
}

#[tokio::test]
async fn test_admin_ai_backend_create_requires_structured_local_input_for_llamacpp() {
    let (handle, grpc_url) = start_sms_grpc().await;
    let (server, _backend_registry) = create_admin_test_server(&grpc_url).await;

    let body: serde_json::Value = server
        .post("/admin/api/ai-backends")
        .json(&serde_json::json!({
            "display_name": "Local llama.cpp",
            "provider": "llamacpp",
            "model": "llama3",
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
                "model_path": "/models/llama3.gguf"
            }
        }))
        .await
        .json();

    assert!(!body["success"].as_bool().unwrap());
    assert_eq!(
        body["message"],
        "local provider-specific input is required for backend provider llamacpp"
    );

    handle.abort();
}

#[tokio::test]
async fn test_admin_ai_backend_create_rejects_duplicate_remote_openai_sources() {
    let (handle, grpc_url) = start_sms_grpc().await;
    let (server, _backend_registry) = create_admin_test_server(&grpc_url).await;

    let body: serde_json::Value = server
        .post("/admin/api/ai-backends")
        .json(&serde_json::json!({
            "display_name": "Remote OpenAI",
            "provider": "openai",
            "model": "gpt-4o",
            "hosting": "remote",
            "backend_kind": "openai_chat_completion",
            "credential_ref": "legacy-openai",
            "remote": {
                "provider_family": "open_ai_compatible",
                "config": {
                    "base_url": "https://api.openai.com/v1",
                    "credential_ref": "openai-prod",
                    "operations": ["chat_completions"],
                    "features": ["stream"],
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
            }
        }))
        .await
        .json();

    assert!(!body["success"].as_bool().unwrap());
    assert_eq!(
        body["message"],
        "remote openai structured input cannot be combined with legacy fields: spec.base_url, credential_ref, spec.operations, spec.features, spec.transports"
    );

    handle.abort();
}

#[tokio::test]
async fn test_admin_ai_backend_create_rejects_unsupported_provider() {
    let (handle, grpc_url) = start_sms_grpc().await;
    let (server, _backend_registry) = create_admin_test_server(&grpc_url).await;

    let body: serde_json::Value = server
        .post("/admin/api/ai-backends")
        .json(&serde_json::json!({
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
        }))
        .await
        .json();

    assert!(!body["success"].as_bool().unwrap());
    assert_eq!(
        body["message"],
        "unsupported backend provider: anthropic-compatible"
    );

    handle.abort();
}

#[tokio::test]
async fn test_admin_ai_backend_update_requires_structured_remote_input_for_openai() {
    let (handle, grpc_url) = start_sms_grpc().await;
    let (server, _backend_registry) = create_admin_test_server(&grpc_url).await;

    let created: serde_json::Value = server
        .post("/admin/api/ai-backends")
        .json(&serde_json::json!({
            "display_name": "Remote OpenAI",
            "provider": "openai",
            "model": "gpt-4o",
            "hosting": "remote",
            "backend_kind": "openai_chat_completion",
            "remote": {
                "provider_family": "open_ai_compatible",
                "config": {
                    "base_url": "https://api.openai.com/v1",
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
            }
        }))
        .await
        .json();
    assert!(created["success"].as_bool().unwrap());
    let backend_id = created["backend"]["backend_id"]
        .as_str()
        .expect("backend id should exist");

    let body: serde_json::Value = server
        .put(&format!("/admin/api/ai-backends/{backend_id}"))
        .json(&serde_json::json!({
            "display_name": "Remote OpenAI Updated",
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
        }))
        .await
        .json();

    assert!(!body["success"].as_bool().unwrap());
    assert_eq!(
        body["message"],
        "remote provider-specific input is required for backend provider openai"
    );

    handle.abort();
}

#[tokio::test]
async fn test_admin_ai_backend_detail_returns_typed_remote_read_view() {
    let (handle, grpc_url) = start_sms_grpc().await;
    let (server, _backend_registry) = create_admin_test_server(&grpc_url).await;

    let created: serde_json::Value = server
        .post("/admin/api/ai-backends")
        .json(&serde_json::json!({
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
                    "operations": ["chat_completions", "embeddings"],
                    "features": ["stream"],
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
            }
        }))
        .await
        .json();
    assert!(created["success"].as_bool().unwrap());
    let backend_id = created["backend"]["backend_id"]
        .as_str()
        .expect("backend id should exist");

    let body: serde_json::Value = server
        .get(&format!("/admin/api/ai-backends/{backend_id}"))
        .await
        .json();

    assert!(body["success"].as_bool().unwrap());
    assert!(body["found"].as_bool().unwrap());
    assert_eq!(
        body["backend"]["remote"]["provider_family"],
        "open_ai_compatible"
    );
    assert_eq!(
        body["backend"]["remote"]["config"]["base_url"],
        "https://api.openai.com/v1"
    );
    assert_eq!(
        body["backend"]["remote"]["config"]["credential_ref"],
        "openai-prod"
    );
    assert_eq!(
        body["backend"]["remote"]["config"]["operations"],
        serde_json::json!(["chat_completions", "embeddings"])
    );
    assert_eq!(
        body["backend"]["remote"]["config"]["features"],
        serde_json::json!(["stream"])
    );
    assert_eq!(
        body["backend"]["remote"]["config"]["transports"],
        serde_json::json!(["http"])
    );

    handle.abort();
}

#[tokio::test]
async fn test_admin_ai_backend_list_returns_typed_local_read_view() {
    let (handle, grpc_url) = start_sms_grpc().await;
    let (server, _backend_registry) = create_admin_test_server(&grpc_url).await;

    let created: serde_json::Value = server
        .post("/admin/api/ai-backends")
        .json(&serde_json::json!({
            "display_name": "Local llama.cpp",
            "provider": "llamacpp",
            "model": "llama3.1",
            "hosting": "local",
            "backend_kind": "llamacpp",
            "local": {
                "provider_family": "llama_cpp",
                "config": {
                    "model_url": "https://models.example.com/llama3.1.gguf",
                    "model_path": "/models/llama3.1.gguf",
                    "skip_download": false,
                    "download_timeout_s": 90,
                    "threads": 8,
                    "ctx_size": 4096
                }
            },
            "spec": {
                "base_url": "http://127.0.0.1:8080/v1",
                "operations": ["chat_completions"],
                "features": [],
                "transports": ["http"],
                "weight": 100,
                "priority": 0
            }
        }))
        .await
        .json();
    assert!(created["success"].as_bool().unwrap());

    let body: serde_json::Value = server.get("/admin/api/ai-backends").await.json();

    assert!(body["success"].as_bool().unwrap());
    let backend = body["backends"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["provider"] == "llamacpp" && item["model"] == "llama3.1")
        .cloned()
        .expect("local backend should appear in list");
    assert_eq!(backend["local"]["provider_family"], "llama_cpp");
    assert_eq!(
        backend["local"]["config"]["model_url"],
        "https://models.example.com/llama3.1.gguf"
    );
    assert_eq!(
        backend["local"]["config"]["model_path"],
        "/models/llama3.1.gguf"
    );
    assert_eq!(backend["local"]["config"]["skip_download"], false);
    assert_eq!(backend["local"]["config"]["download_timeout_s"], 90);
    assert_eq!(backend["local"]["config"]["threads"], 8);
    assert_eq!(backend["local"]["config"]["ctx_size"], 4096);

    handle.abort();
}
