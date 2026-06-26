use crate::proto::sms::{
    admin_credential_service_client::AdminCredentialServiceClient,
    admin_ai_config_service_client::AdminAiConfigServiceClient,
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
use axum::body;
use axum::http::{Request, StatusCode};
use axum::{body::Body, Router};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;
use uuid::Uuid;

async fn make_router_with_limit(limit: usize) -> Router {
    let channel = tonic::transport::Channel::from_static("http://localhost:50051").connect_lazy();
    let files_dir = std::env::temp_dir()
        .join(format!("spear-sms-files-{}", Uuid::new_v4()))
        .to_string_lossy()
        .to_string();
    let state = GatewayState {
        config: Arc::new(crate::sms::config::SmsConfig::default()),
        node_client: NodeServiceClient::new(channel.clone()),
        task_client: TaskServiceClient::new(channel.clone()),
        placement_client: PlacementServiceClient::new(channel.clone()),
        instance_registry_client: InstanceRegistryServiceClient::new(channel.clone()),
        execution_registry_client: ExecutionRegistryServiceClient::new(channel.clone()),
        execution_index_client: ExecutionIndexServiceClient::new(channel.clone()),
        mcp_registry_client: McpRegistryServiceClient::new(channel.clone()),
        backend_registry_client: BackendRegistryServiceClient::new(channel.clone()),
        admin_credential_client: AdminCredentialServiceClient::new(channel.clone()),
        admin_ai_config_client: AdminAiConfigServiceClient::new(channel.clone()),
        model_deployment_registry_client: ModelDeploymentRegistryServiceClient::new(
            channel.clone(),
        ),
        stream_sessions: crate::sms::gateway::StreamSessionStore::new(),
        execution_stream_pool: crate::sms::gateway::ExecutionStreamPool::new(),
        cancel_token: CancellationToken::new(),
        max_upload_bytes: limit,
        files_dir,
    };
    create_gateway_router(state)
}

#[tokio::test]
async fn upload_small_file_succeeds() {
    let app = make_router_with_limit(1024 * 1024).await;
    let data = vec![1u8; 1024];
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/files")
        .body(Body::from(data.clone()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let body_bytes = body::to_bytes(resp.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let id = json.get("id").and_then(|v| v.as_str()).unwrap().to_string();

    let get_req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/files/{}", id))
        .body(Body::empty())
        .unwrap();
    let get_resp = app.clone().oneshot(get_req).await.unwrap();
    assert!(get_resp.status().is_success());
    let downloaded = body::to_bytes(get_resp.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    assert_eq!(downloaded.len(), data.len());
}

#[tokio::test]
async fn upload_exceeds_limit_fails() {
    let app = make_router_with_limit(1024).await;
    let data = vec![0u8; 2048];
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/files")
        .body(Body::from(data))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn meta_endpoint_reports_size() {
    let app = make_router_with_limit(1024 * 1024).await;
    let data = vec![3u8; 4096];
    let up_req = Request::builder()
        .method("POST")
        .uri("/api/v1/files")
        .body(Body::from(data.clone()))
        .unwrap();
    let up_resp = app.clone().oneshot(up_req).await.unwrap();
    assert!(up_resp.status().is_success());
    let up_body = body::to_bytes(up_resp.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&up_body).unwrap();
    let id = json.get("id").and_then(|v| v.as_str()).unwrap().to_string();

    let meta_req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/files/{}/meta", id))
        .body(Body::empty())
        .unwrap();
    let meta_resp = app.clone().oneshot(meta_req).await.unwrap();
    assert!(meta_resp.status().is_success());
    let m_bytes = body::to_bytes(meta_resp.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    let m: serde_json::Value = serde_json::from_slice(&m_bytes).unwrap();
    assert_eq!(m.get("found").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(
        m.get("len").and_then(|v| v.as_u64()),
        Some(data.len() as u64)
    );
}
