use axum_test::TestServer;
use spear_next::proto::sms::{
    backend_registry_service_client::BackendRegistryServiceClient,
    backend_registry_service_server::BackendRegistryServiceServer,
    model_deployment_registry_service_client::ModelDeploymentRegistryServiceClient,
    model_deployment_registry_service_server::ModelDeploymentRegistryServiceServer, BackendHosting,
    BackendInfo, BackendOrigin, BackendSpec, BackendStatus, NodeBackendSnapshot,
    ReportNodeBackendsRequest,
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
            .add_service(ModelDeploymentRegistryServiceServer::new(sms_service))
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
    ModelDeploymentRegistryServiceClient<tonic::transport::Channel>,
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
        admin_credential_client:
            spear_next::proto::sms::admin_credential_service_client::AdminCredentialServiceClient::new(
                channel.clone(),
            ),
        admin_ai_config_client:
            spear_next::proto::sms::admin_ai_config_service_client::AdminAiConfigServiceClient::new(
                channel.clone(),
            ),
        model_deployment_registry_client: ModelDeploymentRegistryServiceClient::new(channel.clone()),
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
        BackendRegistryServiceClient::new(channel.clone()),
        ModelDeploymentRegistryServiceClient::new(channel),
    )
}

#[tokio::test]
async fn test_admin_ai_models_list_filters_hosting() {
    let (handle, grpc_url) = start_sms_grpc().await;
    let (server, mut backend_registry, _model_deployments) =
        create_admin_test_server(&grpc_url).await;

    backend_registry
        .report_node_backends(tonic::Request::new(ReportNodeBackendsRequest {
            snapshot: Some(NodeBackendSnapshot {
                node_uuid: "node-1".to_string(),
                revision: 1,
                reported_at_ms: chrono::Utc::now().timestamp_millis(),
                backends: vec![
                    BackendInfo {
                        spec: Some(BackendSpec {
                            name: "local-ollama".to_string(),
                            kind: "ollama_chat".to_string(),
                            operations: vec!["chat_completions".to_string()],
                            features: Vec::new(),
                            transports: vec!["http".to_string()],
                            weight: 100,
                            priority: 0,
                            base_url: "http://127.0.0.1:11434".to_string(),
                            provider: "ollama".to_string(),
                            model: "llama3".to_string(),
                            hosting: BackendHosting::NodeLocal as i32,
                            credential_ref: String::new(),
                            origin: BackendOrigin::StaticConfig as i32,
                            deployment_id: String::new(),
                        }),
                        status: BackendStatus::Available as i32,
                        status_reason: String::new(),
                    },
                    BackendInfo {
                        spec: Some(BackendSpec {
                            name: "remote-openai".to_string(),
                            kind: "openai_chat".to_string(),
                            operations: vec!["chat_completions".to_string()],
                            features: Vec::new(),
                            transports: vec!["http".to_string()],
                            weight: 100,
                            priority: 0,
                            base_url: "https://api.openai.com/v1".to_string(),
                            provider: "openai".to_string(),
                            model: "gpt-4o".to_string(),
                            hosting: BackendHosting::Remote as i32,
                            credential_ref: String::new(),
                            origin: BackendOrigin::StaticConfig as i32,
                            deployment_id: String::new(),
                        }),
                        status: BackendStatus::Available as i32,
                        status_reason: String::new(),
                    },
                ],
            }),
        }))
        .await
        .unwrap();

    let body: serde_json::Value = server
        .get("/admin/api/ai-models?hosting=local")
        .await
        .json();
    assert!(body["success"].as_bool().unwrap());
    let models = body["models"].as_array().unwrap();
    assert!(models
        .iter()
        .any(|m| m["provider"] == "ollama" && m["model"] == "llama3" && m["hosting"] == "local"));
    assert!(!models.iter().any(|m| m["hosting"] == "remote"));

    handle.abort();
}

#[tokio::test]
async fn test_admin_ai_model_detail_respects_hosting_query() {
    let (handle, grpc_url) = start_sms_grpc().await;
    let (server, mut backend_registry, _model_deployments) =
        create_admin_test_server(&grpc_url).await;

    backend_registry
        .report_node_backends(tonic::Request::new(ReportNodeBackendsRequest {
            snapshot: Some(NodeBackendSnapshot {
                node_uuid: "node-1".to_string(),
                revision: 1,
                reported_at_ms: chrono::Utc::now().timestamp_millis(),
                backends: vec![BackendInfo {
                    spec: Some(BackendSpec {
                        name: "local-openai".to_string(),
                        kind: "openai_chat".to_string(),
                        operations: vec!["chat_completions".to_string()],
                        features: Vec::new(),
                        transports: vec!["http".to_string()],
                        weight: 100,
                        priority: 0,
                        base_url: "http://127.0.0.1:8000/v1".to_string(),
                        provider: "openai".to_string(),
                        model: "gpt-4o".to_string(),
                        hosting: BackendHosting::NodeLocal as i32,
                        credential_ref: String::new(),
                        origin: BackendOrigin::StaticConfig as i32,
                        deployment_id: String::new(),
                    }),
                    status: BackendStatus::Available as i32,
                    status_reason: String::new(),
                }],
            }),
        }))
        .await
        .unwrap();
    backend_registry
        .report_node_backends(tonic::Request::new(ReportNodeBackendsRequest {
            snapshot: Some(NodeBackendSnapshot {
                node_uuid: "node-2".to_string(),
                revision: 1,
                reported_at_ms: chrono::Utc::now().timestamp_millis(),
                backends: vec![BackendInfo {
                    spec: Some(BackendSpec {
                        name: "remote-openai".to_string(),
                        kind: "openai_chat".to_string(),
                        operations: vec!["chat_completions".to_string()],
                        features: Vec::new(),
                        transports: vec!["http".to_string()],
                        weight: 100,
                        priority: 0,
                        base_url: "https://api.openai.com/v1".to_string(),
                        provider: "openai".to_string(),
                        model: "gpt-4o".to_string(),
                        hosting: BackendHosting::Remote as i32,
                        credential_ref: String::new(),
                        origin: BackendOrigin::StaticConfig as i32,
                        deployment_id: String::new(),
                    }),
                    status: BackendStatus::Available as i32,
                    status_reason: String::new(),
                }],
            }),
        }))
        .await
        .unwrap();

    let body: serde_json::Value = server
        .get("/admin/api/ai-models/openai/gpt-4o?hosting=local")
        .await
        .json();
    assert!(body["success"].as_bool().unwrap());
    assert!(body["found"].as_bool().unwrap());
    assert_eq!(body["model"]["hosting"], "local");
    assert!(body["model"]["instances"]
        .as_array()
        .unwrap()
        .iter()
        .all(|i| i["hosting"] == "local"));

    handle.abort();
}

#[tokio::test]
async fn test_admin_create_and_list_node_model_deployments() {
    let (handle, grpc_url) = start_sms_grpc().await;
    let (server, _backend_registry, _model_deployments) = create_admin_test_server(&grpc_url).await;

    let create_body = serde_json::json!({
        "provider": "vllm",
        "model": "meta-llama/Llama-3.1-8B",
        "params": { "base_url": "http://127.0.0.1:8000" }
    });
    let resp: serde_json::Value = server
        .post("/admin/api/nodes/node-1/ai-models")
        .json(&create_body)
        .await
        .json();
    assert!(resp["success"].as_bool().unwrap());
    assert!(resp["deployment_id"].as_str().unwrap().len() > 0);

    let list: serde_json::Value = server
        .get("/admin/api/nodes/node-1/ai-models/deployments")
        .await
        .json();
    assert!(list["success"].as_bool().unwrap());
    let deployments = list["deployments"].as_array().unwrap();
    assert!(deployments.iter().any(|r| {
        r["spec"]["provider"] == "vllm" && r["spec"]["model"] == "meta-llama/Llama-3.1-8B"
    }));

    handle.abort();
}
