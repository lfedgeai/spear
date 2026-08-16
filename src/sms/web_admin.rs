use anyhow::Result;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{HeaderMap, Request, StatusCode};
use axum::middleware;
use axum::middleware::Next;
use axum::response::IntoResponse;
use axum::{
    extract::{Path, Query},
    response::{Html, Json},
    Router,
};
use futures::StreamExt;
use serde::Deserialize;
use serde_json::json;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio_stream::wrappers::IntervalStream;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::proto::sms::{
    ai_backend_control_plane_service_client::AiBackendControlPlaneServiceClient,
    admin_credential_service_client::AdminCredentialServiceClient,
    backend_registry_service_client::BackendRegistryServiceClient,
    mcp_registry_service_client::McpRegistryServiceClient,
    node_service_client::NodeServiceClient, placement_service_client::PlacementServiceClient,
    DeleteCredentialRequest, ListCredentialsRequest, ListNodesRequest, UpsertCredentialRequest,
};
use crate::sms::gateway::GatewayState;
use crate::sms::query_support::{collect_task_instances_bounded, instance_is_active_and_fresh};
pub use router::create_admin_router;
use presenter::{
    credential_info_json, execution_detail_json, execution_history_row_json,
    instance_execution_summary_json, instance_json, mcp_server_json, node_backends_snapshot_json,
    node_detail_json, task_instance_row_json,
};
use types::{
    AiBackendsQuery, AiModelViewsQuery, ExecutionHistoryQuery, ListQuery, PageTokenQuery,
    StreamQuery,
};

use crate::proto::spearlet::{
    execution_service_client::ExecutionServiceClient,
    instance_service_client::InstanceServiceClient,
    invocation_service_client::InvocationServiceClient, DestroyInstanceRequest, ExecutionMode,
    ReconcileTaskAssignmentsNowRequest, TerminateExecutionRequest,
};

mod invocation_flow;
mod node_rpc;
mod router;
mod task_admin;
mod presenter;
mod types;

pub(crate) use task_admin::{
    create_task, delete_task_admin, get_task_detail, list_tasks, CreateTaskBody, DeleteTaskBody,
};

pub struct WebAdminServer {
    addr: SocketAddr,
    grpc_addr: SocketAddr,
    enabled: bool,
    max_upload_bytes: usize,
    files_dir: String,
}

impl WebAdminServer {
    pub fn new(
        addr: SocketAddr,
        grpc_addr: SocketAddr,
        enabled: bool,
        max_upload_bytes: usize,
        files_dir: String,
    ) -> Self {
        Self {
            addr,
            grpc_addr,
            enabled,
            max_upload_bytes,
            files_dir,
        }
    }

    pub async fn start_with_shutdown<F>(self, shutdown: F) -> Result<()>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        if !self.enabled {
            return Ok(());
        }
        let cancel_token = CancellationToken::new();
        let (listener, app) = self.prepare_with_token(cancel_token.clone()).await?;
        let shutdown_and_cancel = async move {
            shutdown.await;
            cancel_token.cancel();
        };
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_and_cancel)
            .await?;
        Ok(())
    }

    async fn prepare_with_token(
        &self,
        cancel_token: CancellationToken,
    ) -> Result<(tokio::net::TcpListener, Router)> {
        let grpc_url = format!("http://{}", self.grpc_addr);
        let channel = tonic::transport::Channel::from_shared(grpc_url)
            .expect("Invalid gRPC URL")
            .connect_lazy();
        let node_client = NodeServiceClient::new(channel.clone());
        let task_client =
            crate::proto::sms::task_service_client::TaskServiceClient::new(channel.clone());
        let task_assignment_client = crate::proto::sms::task_placement_assignment_service_client::TaskPlacementAssignmentServiceClient::new(channel.clone());
        let placement_client = PlacementServiceClient::new(channel.clone());
        let instance_registry_client =
            crate::proto::sms::instance_registry_service_client::InstanceRegistryServiceClient::new(
                channel.clone(),
            );
        let execution_registry_client =
            crate::proto::sms::execution_registry_service_client::ExecutionRegistryServiceClient::new(
                channel.clone(),
            );
        let execution_index_client =
            crate::proto::sms::execution_index_service_client::ExecutionIndexServiceClient::new(
                channel.clone(),
            );
        let mcp_registry_client = McpRegistryServiceClient::new(channel.clone());
        let backend_registry_client = BackendRegistryServiceClient::new(channel.clone());
        let ai_backend_control_plane_client =
            AiBackendControlPlaneServiceClient::new(channel.clone());
        let admin_credential_client = AdminCredentialServiceClient::new(channel.clone());
        let state = GatewayState {
            config: Arc::new(crate::sms::config::SmsConfig::default()),
            node_client,
            task_client,
            task_assignment_client,
            placement_client,
            instance_registry_client,
            execution_registry_client,
            execution_index_client,
            mcp_registry_client,
            backend_registry_client,
            ai_backend_control_plane_client,
            admin_credential_client,
            stream_sessions: crate::sms::gateway::StreamSessionStore::new(),
            execution_stream_pool: crate::sms::gateway::ExecutionStreamPool::new(),
            cancel_token: cancel_token.clone(),
            max_upload_bytes: self.max_upload_bytes,
            files_dir: self.files_dir.clone(),
        };
        let mut app = create_admin_router(state);
        if let Ok(token) = std::env::var("SMS_WEB_ADMIN_TOKEN") {
            let bearer = format!("Bearer {}", token);
            app = app.layer(middleware::from_fn(
                move |req: Request<axum::body::Body>, next: Next| {
                    let bearer = bearer.clone();
                    async move {
                        let authorized = req
                            .headers()
                            .get(axum::http::header::AUTHORIZATION)
                            .and_then(|h| h.to_str().ok())
                            .map(|v| v == bearer)
                            .unwrap_or(false);
                        if !authorized {
                            return StatusCode::UNAUTHORIZED.into_response();
                        }
                        next.run(req).await
                    }
                },
            ));
        }
        let listener = tokio::net::TcpListener::bind(self.addr).await?;
        Ok((listener, app))
    }
}

#[derive(Deserialize)]
struct McpServerUpsertBody {
    server_id: String,
    display_name: Option<String>,
    transport: String,
    stdio: Option<McpStdioBody>,
    http: Option<McpHttpBody>,
    tool_namespace: Option<String>,
    allowed_tools: Option<Vec<String>>,
    budgets: Option<McpBudgetsBody>,
    approval_policy: Option<McpApprovalPolicyBody>,
}

#[derive(Deserialize)]
struct McpStdioBody {
    command: String,
    args: Option<Vec<String>>,
    env: Option<std::collections::HashMap<String, String>>,
    cwd: Option<String>,
}

#[derive(Deserialize)]
struct McpHttpBody {
    url: String,
    headers: Option<std::collections::HashMap<String, String>>,
    auth_ref: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct UpsertCredentialBody {
    name: String,
    secret: Option<String>,
    description: Option<String>,
    disabled: Option<bool>,
}

#[derive(Deserialize)]
pub(crate) struct AiBackendWriteBody {
    display_name: String,
    provider: String,
    model: String,
    hosting: String,
    backend_kind: String,
    management_mode: Option<String>,
    credential_ref: Option<String>,
    desired_state: Option<String>,
    spec: AiBackendSpecBody,
    labels: Option<std::collections::HashMap<String, String>>,
    metadata: Option<serde_json::Value>,
}

#[derive(Deserialize)]
pub(crate) struct AiBackendSpecBody {
    base_url: Option<String>,
    operations: Vec<String>,
    features: Option<Vec<String>>,
    transports: Option<Vec<String>>,
    weight: Option<u32>,
    priority: Option<i32>,
}

#[derive(Deserialize)]
pub(crate) struct SetAiBackendDesiredStateBody {
    desired_state: String,
}

#[derive(Deserialize)]
pub(crate) struct AiBackendPlacementWriteBody {
    placement_id: Option<String>,
    backend_id: String,
    node_uuid: String,
    desired_state: Option<String>,
    weight_override: Option<i32>,
    priority_override: Option<i32>,
}

#[derive(Deserialize)]
pub(crate) struct AiBackendPlacementsQuery {
    backend_id: Option<String>,
    node_uuid: Option<String>,
}

#[derive(Deserialize)]
struct McpBudgetsBody {
    tool_timeout_ms: Option<u64>,
    max_concurrency: Option<u64>,
    max_tool_output_bytes: Option<u64>,
}

#[derive(Deserialize)]
struct McpApprovalPolicyBody {
    default_policy: Option<String>,
    per_tool: Option<std::collections::HashMap<String, String>>,
}

async fn get_node_backends(state: GatewayState, p: Path<String>) -> Json<serde_json::Value> {
    use crate::proto::sms::GetNodeBackendsRequest;

    let node_uuid = p.0;
    if node_uuid.trim().is_empty() {
        return Json(json!({"found": false}));
    }
    let mut client = state.backend_registry_client.clone();
    match client
        .get_node_backends(GetNodeBackendsRequest {
            node_uuid: node_uuid.clone(),
        })
        .await
    {
        Ok(resp) => {
            let inner = resp.into_inner();
            let projection = node_backends_snapshot_json(inner.snapshot.as_ref());
            Json(json!({
                "found": inner.found,
                "node_uuid": node_uuid,
                "backends": projection.get("backends").cloned().unwrap_or_else(|| json!([])),
                "snapshot": projection.get("snapshot").cloned().unwrap_or(serde_json::Value::Null),
            }))
        }
        Err(e) => Json(json!({"found": false, "message": e.to_string()})),
    }
}

async fn list_ai_backends_admin(
    state: GatewayState,
    Query(q): Query<AiBackendsQuery>,
) -> Json<serde_json::Value> {
    let mut client = state.ai_backend_control_plane_client.clone();
    match client
        .list_ai_backends(crate::proto::sms::ListAiBackendsRequest {
            limit: q.limit.unwrap_or(0),
            offset: q.offset.unwrap_or(0),
            q: q.q.unwrap_or_default(),
            hosting: q.hosting.unwrap_or_default(),
            desired_state: q.desired_state.unwrap_or_default(),
            provider: q.provider.unwrap_or_default(),
            model: q.model.unwrap_or_default(),
        })
        .await
    {
        Ok(resp) => {
            let inner = resp.into_inner();
            let backends = inner
                .backends
                .into_iter()
                .map(ai_backend_record_json)
                .collect::<Vec<_>>();
            Json(json!({"success": true, "backends": backends, "total_count": inner.total_count}))
        }
        Err(e) => Json(json!({"success": false, "message": e.to_string()})),
    }
}

async fn get_ai_backend_admin(
    state: GatewayState,
    p: Path<String>,
) -> Json<serde_json::Value> {
    let backend_id = p.0;
    if backend_id.trim().is_empty() {
        return Json(json!({"success": false, "message": "backend_id is required"}));
    }

    let mut client = state.ai_backend_control_plane_client.clone();
    match client
        .get_ai_backend(crate::proto::sms::GetAiBackendRequest { backend_id })
        .await
    {
        Ok(resp) => {
            let inner = resp.into_inner();
            Json(json!({
                "success": true,
                "found": inner.found,
                "backend": inner.backend.map(ai_backend_record_json),
            }))
        }
        Err(e) => Json(json!({"success": false, "message": e.to_string()})),
    }
}

async fn create_ai_backend_admin(
    state: GatewayState,
    body: Json<AiBackendWriteBody>,
) -> Json<serde_json::Value> {
    let backend = match ai_backend_proto_from_body(body.0, String::new()) {
        Ok(value) => value,
        Err(message) => return Json(json!({"success": false, "message": message})),
    };

    let mut client = state.ai_backend_control_plane_client.clone();
    match client
        .create_ai_backend(crate::proto::sms::CreateAiBackendRequest {
            backend: Some(backend),
        })
        .await
    {
        Ok(resp) => {
            let inner = resp.into_inner();
            Json(json!({
                "success": true,
                "backend": inner.backend.map(ai_backend_record_json),
            }))
        }
        Err(e) => Json(json!({"success": false, "message": e.to_string()})),
    }
}

async fn update_ai_backend_admin(
    state: GatewayState,
    p: Path<String>,
    body: Json<AiBackendWriteBody>,
) -> Json<serde_json::Value> {
    let backend_id = p.0;
    if backend_id.trim().is_empty() {
        return Json(json!({"success": false, "message": "backend_id is required"}));
    }

    let backend = match ai_backend_proto_from_body(body.0, backend_id) {
        Ok(value) => value,
        Err(message) => return Json(json!({"success": false, "message": message})),
    };

    let mut client = state.ai_backend_control_plane_client.clone();
    match client
        .update_ai_backend(crate::proto::sms::UpdateAiBackendRequest {
            backend: Some(backend),
        })
        .await
    {
        Ok(resp) => {
            let inner = resp.into_inner();
            Json(json!({
                "success": true,
                "backend": inner.backend.map(ai_backend_record_json),
            }))
        }
        Err(e) => Json(json!({"success": false, "message": e.to_string()})),
    }
}

async fn delete_ai_backend_admin(
    state: GatewayState,
    p: Path<String>,
) -> Json<serde_json::Value> {
    let backend_id = p.0;
    if backend_id.trim().is_empty() {
        return Json(json!({"success": false, "message": "backend_id is required"}));
    }

    let mut client = state.ai_backend_control_plane_client.clone();
    match client
        .delete_ai_backend(crate::proto::sms::DeleteAiBackendRequest { backend_id })
        .await
    {
        Ok(resp) => Json(json!({"success": true, "deleted": resp.into_inner().deleted})),
        Err(e) => Json(json!({"success": false, "message": e.to_string()})),
    }
}

async fn set_ai_backend_desired_state_admin(
    state: GatewayState,
    p: Path<String>,
    body: Json<SetAiBackendDesiredStateBody>,
) -> Json<serde_json::Value> {
    let backend_id = p.0;
    if backend_id.trim().is_empty() {
        return Json(json!({"success": false, "message": "backend_id is required"}));
    }

    let desired_state = match parse_ai_backend_desired_state(&body.0.desired_state) {
        Ok(value) => value,
        Err(message) => return Json(json!({"success": false, "message": message})),
    };

    let mut client = state.ai_backend_control_plane_client.clone();
    match client
        .set_ai_backend_desired_state(crate::proto::sms::SetAiBackendDesiredStateRequest {
            backend_id,
            desired_state: desired_state as i32,
        })
        .await
    {
        Ok(resp) => {
            let inner = resp.into_inner();
            Json(json!({
                "success": true,
                "backend": inner.backend.map(ai_backend_record_json),
            }))
        }
        Err(e) => Json(json!({"success": false, "message": e.to_string()})),
    }
}

async fn list_ai_backend_placements_admin(
    state: GatewayState,
    Query(q): Query<AiBackendPlacementsQuery>,
) -> Json<serde_json::Value> {
    let mut client = state.ai_backend_control_plane_client.clone();
    match client
        .list_ai_backend_placements(crate::proto::sms::ListAiBackendPlacementsRequest {
            backend_id: q.backend_id.unwrap_or_default(),
            node_uuid: q.node_uuid.unwrap_or_default(),
        })
        .await
    {
        Ok(resp) => {
            let inner = resp.into_inner();
            let placements = inner
                .placements
                .into_iter()
                .map(ai_backend_placement_json)
                .collect::<Vec<_>>();
            Json(json!({
                "success": true,
                "placements": placements,
                "total_count": inner.total_count,
            }))
        }
        Err(e) => Json(json!({"success": false, "message": e.to_string()})),
    }
}

async fn upsert_ai_backend_placement_admin(
    state: GatewayState,
    body: Json<AiBackendPlacementWriteBody>,
) -> Json<serde_json::Value> {
    let placement = match ai_backend_placement_proto_from_body(body.0) {
        Ok(value) => value,
        Err(message) => return Json(json!({"success": false, "message": message})),
    };

    let mut client = state.ai_backend_control_plane_client.clone();
    match client
        .upsert_ai_backend_placement(crate::proto::sms::UpsertAiBackendPlacementRequest {
            placement: Some(placement),
        })
        .await
    {
        Ok(resp) => {
            let inner = resp.into_inner();
            Json(json!({
                "success": true,
                "placement": inner.placement.map(ai_backend_placement_json),
            }))
        }
        Err(e) => Json(json!({"success": false, "message": e.to_string()})),
    }
}

async fn delete_ai_backend_placement_admin(
    state: GatewayState,
    p: Path<String>,
) -> Json<serde_json::Value> {
    let placement_id = p.0;
    if placement_id.trim().is_empty() {
        return Json(json!({"success": false, "message": "placement_id is required"}));
    }

    let mut client = state.ai_backend_control_plane_client.clone();
    match client
        .delete_ai_backend_placement(crate::proto::sms::DeleteAiBackendPlacementRequest {
            placement_id,
        })
        .await
    {
        Ok(resp) => Json(json!({"success": true, "deleted": resp.into_inner().deleted})),
        Err(e) => Json(json!({"success": false, "message": e.to_string()})),
    }
}

async fn list_ai_backend_assignments_admin(
    state: GatewayState,
    p: Path<String>,
) -> Json<serde_json::Value> {
    let node_uuid = p.0;
    if node_uuid.trim().is_empty() {
        return Json(json!({"success": false, "message": "node_uuid is required"}));
    }

    let mut client = state.ai_backend_control_plane_client.clone();
    match client
        .list_ai_backend_assignments(crate::proto::sms::ListAiBackendAssignmentsRequest {
            node_uuid,
        })
        .await
    {
        Ok(resp) => {
            let inner = resp.into_inner();
            let assignments = inner
                .assignments
                .into_iter()
                .map(ai_backend_assignment_json)
                .collect::<Vec<_>>();
            Json(json!({
                "success": true,
                "assignments": assignments,
                "total_count": inner.total_count,
            }))
        }
        Err(e) => Json(json!({"success": false, "message": e.to_string()})),
    }
}

async fn list_ai_backend_node_statuses_admin(
    state: GatewayState,
    p: Path<String>,
) -> Json<serde_json::Value> {
    let backend_id = p.0;
    if backend_id.trim().is_empty() {
        return Json(json!({"success": false, "message": "backend_id is required"}));
    }

    let mut client = state.ai_backend_control_plane_client.clone();
    match client
        .list_ai_backend_node_statuses(crate::proto::sms::ListAiBackendNodeStatusesRequest {
            backend_id,
        })
        .await
    {
        Ok(resp) => {
            let inner = resp.into_inner();
            let statuses = inner
                .statuses
                .into_iter()
                .map(ai_backend_node_status_json)
                .collect::<Vec<_>>();
            Json(json!({
                "success": true,
                "statuses": statuses,
                "total_count": inner.total_count,
            }))
        }
        Err(e) => Json(json!({"success": false, "message": e.to_string()})),
    }
}

async fn list_ai_model_views_admin(
    state: GatewayState,
    Query(q): Query<AiModelViewsQuery>,
) -> Json<serde_json::Value> {
    let mut client = state.ai_backend_control_plane_client.clone();
    match client
        .list_ai_model_views(crate::proto::sms::ListAiModelViewsRequest {
            limit: q.limit.unwrap_or(0),
            offset: q.offset.unwrap_or(0),
            q: q.q.unwrap_or_default(),
            hosting: q.hosting.unwrap_or_default(),
            status: q.status.unwrap_or_default(),
            provider: q.provider.unwrap_or_default(),
            model: q.model.unwrap_or_default(),
        })
        .await
    {
        Ok(resp) => {
            let inner = resp.into_inner();
            let views = inner
                .views
                .into_iter()
                .map(ai_model_view_json)
                .collect::<Vec<_>>();
            Json(json!({"success": true, "views": views, "total_count": inner.total_count}))
        }
        Err(e) => Json(json!({"success": false, "message": e.to_string()})),
    }
}

/// Build a proto backend record from HTTP JSON input / 从 HTTP JSON 输入构建 proto backend 记录
fn ai_backend_proto_from_body(
    body: AiBackendWriteBody,
    backend_id: String,
) -> Result<crate::proto::sms::AiBackendRecord, String> {
    if body.display_name.trim().is_empty() {
        return Err("display_name is required".to_string());
    }
    if body.provider.trim().is_empty() {
        return Err("provider is required".to_string());
    }
    if body.model.trim().is_empty() {
        return Err("model is required".to_string());
    }
    if body.backend_kind.trim().is_empty() {
        return Err("backend_kind is required".to_string());
    }
    if body.spec.operations.is_empty() {
        return Err("spec.operations is required".to_string());
    }

    let hosting = parse_ai_backend_hosting(&body.hosting)?;
    let management_mode = match body.management_mode.as_deref() {
        Some(value) => parse_ai_backend_management_mode(value)?,
        None => default_ai_backend_management_mode(hosting),
    };
    let desired_state = match body.desired_state.as_deref() {
        Some(value) => parse_ai_backend_desired_state(value)?,
        None => crate::proto::sms::AiBackendDesiredState::Enabled,
    };
    let credential_ref = trim_to_option(body.credential_ref.clone());

    let record = crate::sms::ai_backends::model::AiBackendRecordModel {
        backend_id,
        display_name: body.display_name.trim().to_string(),
        provider: body.provider.trim().to_string(),
        model: body.model.trim().to_string(),
        hosting: match hosting {
            crate::proto::sms::AiBackendHosting::Remote => {
                crate::sms::ai_backends::model::AiBackendHostingModel::Remote
            }
            crate::proto::sms::AiBackendHosting::Local => {
                crate::sms::ai_backends::model::AiBackendHostingModel::Local
            }
            crate::proto::sms::AiBackendHosting::Unspecified => {
                return Err("hosting is required".to_string())
            }
        },
        backend_kind: body.backend_kind.trim().to_string(),
        desired_state: match desired_state {
            crate::proto::sms::AiBackendDesiredState::Enabled => {
                crate::sms::ai_backends::model::AiBackendDesiredStateModel::Enabled
            }
            crate::proto::sms::AiBackendDesiredState::Disabled => {
                crate::sms::ai_backends::model::AiBackendDesiredStateModel::Disabled
            }
            crate::proto::sms::AiBackendDesiredState::Unspecified => {
                return Err("desired_state is required".to_string())
            }
        },
        management_mode: match management_mode {
            crate::proto::sms::AiBackendManagementMode::SmsRemote => {
                crate::sms::ai_backends::model::AiBackendManagementModeModel::SmsRemote
            }
            crate::proto::sms::AiBackendManagementMode::SmsLocal => {
                crate::sms::ai_backends::model::AiBackendManagementModeModel::SmsLocal
            }
            crate::proto::sms::AiBackendManagementMode::Unspecified => {
                return Err("management_mode is required".to_string())
            }
        },
        credential_ref: credential_ref.clone(),
        spec: crate::sms::ai_backends::model::AiBackendSpecModel {
            name: String::new(),
            kind: body.backend_kind.trim().to_string(),
            operations: body.spec.operations,
            features: body.spec.features.unwrap_or_default(),
            transports: body
                .spec
                .transports
                .unwrap_or_else(|| vec!["http".to_string()]),
            weight: body.spec.weight.unwrap_or(100),
            priority: body.spec.priority.unwrap_or(0),
            base_url: body.spec.base_url.unwrap_or_default(),
            provider: body.provider.trim().to_string(),
            model: body.model.trim().to_string(),
            credential_ref: credential_ref.unwrap_or_default(),
            origin: 0,
            deployment_id: String::new(),
        },
        labels: body.labels.unwrap_or_default().into_iter().collect(),
        metadata: body.metadata.unwrap_or_else(|| json!({})),
        generation: 0,
        created_at_ms: 0,
        updated_at_ms: 0,
    };
    Ok(crate::sms::ai_backends::proto_conv::proto_backend_from_domain(&record))
}

/// Build a proto placement record from HTTP JSON input / 从 HTTP JSON 输入构建 proto placement 记录
fn ai_backend_placement_proto_from_body(
    body: AiBackendPlacementWriteBody,
) -> Result<crate::proto::sms::AiBackendPlacementRecord, String> {
    if body.backend_id.trim().is_empty() {
        return Err("backend_id is required".to_string());
    }
    if body.node_uuid.trim().is_empty() {
        return Err("node_uuid is required".to_string());
    }

    Ok(crate::proto::sms::AiBackendPlacementRecord {
        placement_id: body.placement_id.unwrap_or_default(),
        backend_id: body.backend_id,
        node_uuid: body.node_uuid,
        desired_state: match body.desired_state.as_deref() {
            Some(value) => parse_ai_backend_desired_state(value)? as i32,
            None => crate::proto::sms::AiBackendDesiredState::Enabled as i32,
        },
        weight_override: body.weight_override,
        priority_override: body.priority_override,
        generation: 0,
        created_at_ms: 0,
        updated_at_ms: 0,
    })
}

/// Convert one backend record into admin JSON / 把单个 backend 记录转换为管理端 JSON
fn ai_backend_record_json(record: crate::proto::sms::AiBackendRecord) -> serde_json::Value {
    json!({
        "backend_id": record.backend_id,
        "display_name": record.display_name,
        "provider": record.provider,
        "model": record.model,
        "hosting": ai_backend_hosting_label(record.hosting),
        "backend_kind": record.backend_kind,
        "desired_state": ai_backend_desired_state_label(record.desired_state),
        "management_mode": ai_backend_management_mode_label(record.management_mode),
        "credential_ref": trim_to_option(record.credential_ref),
        "spec": record.spec.map(ai_backend_spec_json),
        "labels": record.labels,
        "metadata": proto_struct_to_json(record.metadata.as_ref()),
        "generation": record.generation,
        "created_at_ms": record.created_at_ms,
        "updated_at_ms": record.updated_at_ms,
    })
}

/// Convert one backend spec into admin JSON / 把单个 backend spec 转换为管理端 JSON
fn ai_backend_spec_json(spec: crate::proto::sms::BackendSpec) -> serde_json::Value {
    json!({
        "name": spec.name,
        "kind": spec.kind,
        "operations": spec.operations,
        "features": spec.features,
        "transports": spec.transports,
        "weight": spec.weight,
        "priority": spec.priority,
        "base_url": spec.base_url,
        "provider": spec.provider,
        "model": spec.model,
        "credential_ref": trim_to_option(Some(spec.credential_ref)),
        "origin": spec.origin,
        "deployment_id": spec.deployment_id,
    })
}

/// Convert one placement record into admin JSON / 把单个 placement 记录转换为管理端 JSON
fn ai_backend_placement_json(
    record: crate::proto::sms::AiBackendPlacementRecord,
) -> serde_json::Value {
    json!({
        "placement_id": record.placement_id,
        "backend_id": record.backend_id,
        "node_uuid": record.node_uuid,
        "desired_state": ai_backend_desired_state_label(record.desired_state),
        "weight_override": record.weight_override,
        "priority_override": record.priority_override,
        "generation": record.generation,
        "created_at_ms": record.created_at_ms,
        "updated_at_ms": record.updated_at_ms,
    })
}

/// Convert one node status record into admin JSON / 把单个节点状态记录转换为管理端 JSON
fn ai_backend_node_status_json(
    record: crate::proto::sms::AiBackendNodeStatusRecord,
) -> serde_json::Value {
    json!({
        "backend_id": record.backend_id,
        "node_uuid": record.node_uuid,
        "observed_generation": record.observed_generation,
        "status": ai_backend_node_status_label(record.status),
        "status_reason": record.status_reason,
        "runtime_backend_name": trim_to_option(Some(record.runtime_backend_name)),
        "endpoint": trim_to_option(Some(record.endpoint)),
        "available": record.available,
        "operations": record.operations,
        "features": record.features,
        "transports": record.transports,
        "last_heartbeat_at_ms": record.last_heartbeat_at_ms,
    })
}

/// Convert one resolved assignment into admin JSON / 把单个解析后的 assignment 转换为管理端 JSON
fn ai_backend_assignment_json(
    assignment: crate::proto::sms::ResolvedAiBackendAssignment,
) -> serde_json::Value {
    json!({
        "backend": assignment.backend.map(ai_backend_record_json),
        "placement": assignment.placement.map(ai_backend_placement_json),
    })
}

/// Convert one read-model row into admin JSON / 把单个只读模型行转换为管理端 JSON
fn ai_model_view_json(view: crate::proto::sms::AiModelView) -> serde_json::Value {
    json!({
        "provider": view.provider,
        "model": view.model,
        "hosting": ai_backend_hosting_label(view.hosting),
        "backend_ids": view.backend_ids,
        "operations": view.operations,
        "features": view.features,
        "transports": view.transports,
        "enabled_nodes": view.enabled_nodes,
        "ready_nodes": view.ready_nodes,
        "total_nodes": view.total_nodes,
        "instances": view.instances.into_iter().map(ai_model_view_instance_json).collect::<Vec<_>>(),
    })
}

/// Convert one read-model instance into admin JSON / 把单个只读模型实例转换为管理端 JSON
fn ai_model_view_instance_json(
    instance: crate::proto::sms::AiModelViewInstance,
) -> serde_json::Value {
    json!({
        "backend_id": instance.backend_id,
        "node_uuid": instance.node_uuid,
        "placement_state": ai_backend_desired_state_label(instance.placement_state),
        "backend_state": ai_backend_desired_state_label(instance.backend_state),
        "runtime_status": instance.runtime_status.map(ai_backend_node_status_label),
        "runtime_backend_name": trim_to_option(instance.runtime_backend_name),
        "endpoint": trim_to_option(instance.endpoint),
        "available": instance.available,
    })
}

/// Convert protobuf Struct into JSON / 把 protobuf Struct 转换为 JSON
fn proto_struct_to_json(value: Option<&prost_types::Struct>) -> serde_json::Value {
    let fields = value
        .map(|inner| {
            inner
                .fields
                .iter()
                .map(|(key, value)| (key.clone(), proto_value_to_json(value)))
                .collect()
        })
        .unwrap_or_default();
    serde_json::Value::Object(fields)
}

/// Convert protobuf Value into JSON / 把 protobuf Value 转换为 JSON
fn proto_value_to_json(value: &prost_types::Value) -> serde_json::Value {
    match value.kind.as_ref() {
        Some(prost_types::value::Kind::NullValue(_)) | None => serde_json::Value::Null,
        Some(prost_types::value::Kind::BoolValue(inner)) => serde_json::Value::Bool(*inner),
        Some(prost_types::value::Kind::NumberValue(inner)) => serde_json::json!(inner),
        Some(prost_types::value::Kind::StringValue(inner)) => serde_json::Value::String(inner.clone()),
        Some(prost_types::value::Kind::StructValue(inner)) => proto_struct_to_json(Some(inner)),
        Some(prost_types::value::Kind::ListValue(inner)) => {
            serde_json::Value::Array(inner.values.iter().map(proto_value_to_json).collect())
        }
    }
}

/// Parse backend hosting from HTTP text / 从 HTTP 文本解析 backend hosting
fn parse_ai_backend_hosting(
    value: &str,
) -> Result<crate::proto::sms::AiBackendHosting, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "remote" => Ok(crate::proto::sms::AiBackendHosting::Remote),
        "local" => Ok(crate::proto::sms::AiBackendHosting::Local),
        _ => Err("hosting must be remote or local".to_string()),
    }
}

/// Parse desired state from HTTP text / 从 HTTP 文本解析期望状态
fn parse_ai_backend_desired_state(
    value: &str,
) -> Result<crate::proto::sms::AiBackendDesiredState, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "enabled" => Ok(crate::proto::sms::AiBackendDesiredState::Enabled),
        "disabled" => Ok(crate::proto::sms::AiBackendDesiredState::Disabled),
        _ => Err("desired_state must be enabled or disabled".to_string()),
    }
}

/// Parse management mode from HTTP text / 从 HTTP 文本解析管理模式
fn parse_ai_backend_management_mode(
    value: &str,
) -> Result<crate::proto::sms::AiBackendManagementMode, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "sms_remote" | "remote" => Ok(crate::proto::sms::AiBackendManagementMode::SmsRemote),
        "sms_local" | "local" => Ok(crate::proto::sms::AiBackendManagementMode::SmsLocal),
        _ => Err("management_mode must be sms_remote or sms_local".to_string()),
    }
}

/// Choose the default management mode from hosting / 根据 hosting 选择默认管理模式
fn default_ai_backend_management_mode(
    hosting: crate::proto::sms::AiBackendHosting,
) -> crate::proto::sms::AiBackendManagementMode {
    match hosting {
        crate::proto::sms::AiBackendHosting::Remote => {
            crate::proto::sms::AiBackendManagementMode::SmsRemote
        }
        crate::proto::sms::AiBackendHosting::Local => {
            crate::proto::sms::AiBackendManagementMode::SmsLocal
        }
        crate::proto::sms::AiBackendHosting::Unspecified => {
            crate::proto::sms::AiBackendManagementMode::Unspecified
        }
    }
}

/// Normalize an optional string by trimming empties / 通过裁剪空字符串归一化可选字符串
fn trim_to_option(value: Option<String>) -> Option<String> {
    value.and_then(|inner| {
        let trimmed = inner.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

/// Render hosting enum as stable text / 把 hosting 枚举渲染为稳定文本
fn ai_backend_hosting_label(value: i32) -> &'static str {
    match crate::proto::sms::AiBackendHosting::try_from(value)
        .unwrap_or(crate::proto::sms::AiBackendHosting::Unspecified)
    {
        crate::proto::sms::AiBackendHosting::Remote => "remote",
        crate::proto::sms::AiBackendHosting::Local => "local",
        crate::proto::sms::AiBackendHosting::Unspecified => "unspecified",
    }
}

/// Render desired-state enum as stable text / 把期望状态枚举渲染为稳定文本
fn ai_backend_desired_state_label(value: i32) -> &'static str {
    match crate::proto::sms::AiBackendDesiredState::try_from(value)
        .unwrap_or(crate::proto::sms::AiBackendDesiredState::Unspecified)
    {
        crate::proto::sms::AiBackendDesiredState::Enabled => "enabled",
        crate::proto::sms::AiBackendDesiredState::Disabled => "disabled",
        crate::proto::sms::AiBackendDesiredState::Unspecified => "unspecified",
    }
}

/// Render management-mode enum as stable text / 把管理模式枚举渲染为稳定文本
fn ai_backend_management_mode_label(value: i32) -> &'static str {
    match crate::proto::sms::AiBackendManagementMode::try_from(value)
        .unwrap_or(crate::proto::sms::AiBackendManagementMode::Unspecified)
    {
        crate::proto::sms::AiBackendManagementMode::SmsRemote => "sms_remote",
        crate::proto::sms::AiBackendManagementMode::SmsLocal => "sms_local",
        crate::proto::sms::AiBackendManagementMode::Unspecified => "unspecified",
    }
}

/// Render node-status enum as stable text / 把节点状态枚举渲染为稳定文本
fn ai_backend_node_status_label(value: i32) -> &'static str {
    match crate::proto::sms::AiBackendNodeStatus::try_from(value)
        .unwrap_or(crate::proto::sms::AiBackendNodeStatus::Unspecified)
    {
        crate::proto::sms::AiBackendNodeStatus::Pending => "pending",
        crate::proto::sms::AiBackendNodeStatus::Reconciling => "reconciling",
        crate::proto::sms::AiBackendNodeStatus::Ready => "ready",
        crate::proto::sms::AiBackendNodeStatus::Degraded => "degraded",
        crate::proto::sms::AiBackendNodeStatus::Error => "error",
        crate::proto::sms::AiBackendNodeStatus::Disabled => "disabled",
        crate::proto::sms::AiBackendNodeStatus::Unspecified => "unspecified",
    }
}

async fn list_credentials_admin(state: GatewayState) -> Json<serde_json::Value> {
    let mut client = state.admin_credential_client.clone();
    match client.list_credentials(ListCredentialsRequest {}).await {
        Ok(resp) => {
            let inner = resp.into_inner();
            let mut backend_client = state.ai_backend_control_plane_client.clone();
            let referenced_by_count = match backend_client
                .list_ai_backends(crate::proto::sms::ListAiBackendsRequest {
                    limit: 0,
                    offset: 0,
                    q: String::new(),
                    hosting: String::new(),
                    desired_state: String::new(),
                    provider: String::new(),
                    model: String::new(),
                })
                .await
            {
                Ok(resp) => {
                    // Count credential references from the unified AI backend records.
                    // 从统一 AI backend 记录中统计 credential 引用数。
                    let mut counts = std::collections::HashMap::<String, usize>::new();
                    for backend in resp.into_inner().backends {
                        let credential_ref = backend.credential_ref.as_deref().unwrap_or("").trim();
                        if credential_ref.is_empty() {
                            continue;
                        }
                        *counts.entry(credential_ref.to_string()).or_insert(0) += 1;
                    }
                    counts
                }
                Err(_) => std::collections::HashMap::new(),
            };
            let credentials = inner
                .credentials
                .into_iter()
                .map(|c| {
                    let referenced = referenced_by_count.get(&c.name).copied().unwrap_or(0);
                    credential_info_json(c, referenced)
                })
                .collect::<Vec<_>>();
            Json(json!({"success": true, "revision": inner.revision, "credentials": credentials}))
        }
        Err(e) => Json(json!({"success": false, "message": e.to_string()})),
    }
}

async fn upsert_credential_admin(
    state: GatewayState,
    body: Json<UpsertCredentialBody>,
) -> Json<serde_json::Value> {
    let b = body.0;
    if b.name.trim().is_empty() {
        return Json(json!({"success": false, "message": "name is required"}));
    }
    let mut client = state.admin_credential_client.clone();
    let resp = match client
        .upsert_credential(UpsertCredentialRequest {
            name: b.name,
            secret: b.secret.unwrap_or_default(),
            description: b.description.unwrap_or_default(),
            disabled: b.disabled.unwrap_or(false),
        })
        .await
    {
        Ok(r) => r.into_inner(),
        Err(e) => return Json(json!({"success": false, "message": e.to_string()})),
    };
    Json(json!({"success": true, "revision": resp.revision}))
}

async fn delete_credential_admin(
    state: GatewayState,
    p: Path<String>,
) -> Json<serde_json::Value> {
    let name = p.0;
    if name.trim().is_empty() {
        return Json(json!({"success": false, "message": "name is required"}));
    }
    let mut client = state.admin_credential_client.clone();
    let resp = match client
        .delete_credential(DeleteCredentialRequest { name })
        .await
    {
        Ok(r) => r.into_inner(),
        Err(e) => return Json(json!({"success": false, "message": e.to_string()})),
    };
    Json(json!({"success": true, "revision": resp.revision, "deleted": resp.deleted}))
}

async fn list_mcp_servers(state: GatewayState) -> Json<serde_json::Value> {
    let mut client = state.mcp_registry_client.clone();
    match client
        .list_mcp_servers(crate::proto::sms::ListMcpServersRequest { since_revision: 0 })
        .await
    {
        Ok(resp) => {
            let inner = resp.into_inner();
            let servers = inner
                .servers
                .into_iter()
                .map(mcp_server_json)
                .collect::<Vec<_>>();
            Json(json!({"success": true, "revision": inner.revision, "servers": servers}))
        }
        Err(e) => Json(json!({"success": false, "message": e.to_string()})),
    }
}

async fn get_mcp_server(state: GatewayState, p: Path<String>) -> Json<serde_json::Value> {
    let server_id = p.0;
    if server_id.trim().is_empty() {
        return Json(json!({"success": true, "found": false}));
    }
    let mut client = state.mcp_registry_client.clone();
    let resp = match client
        .list_mcp_servers(crate::proto::sms::ListMcpServersRequest { since_revision: 0 })
        .await
    {
        Ok(r) => r.into_inner(),
        Err(e) => return Json(json!({"success": false, "message": e.to_string()})),
    };

    for s in resp.servers.into_iter() {
        if s.server_id != server_id {
            continue;
        }
        let server = mcp_server_json(s);
        return Json(json!({"success": true, "found": true, "server": server}));
    }

    Json(json!({"success": true, "found": false}))
}

async fn upsert_mcp_server(
    state: GatewayState,
    body: Json<McpServerUpsertBody>,
) -> Json<serde_json::Value> {
    use crate::proto::sms::{
        McpApprovalPolicy, McpBudgets, McpHttpConfig, McpServerRecord, McpStdioConfig, McpTransport,
    };

    let body = body.0;
    if body.server_id.trim().is_empty() {
        return Json(json!({"success": false, "message": "server_id is required"}));
    }

    let transport = match body.transport.as_str() {
        "stdio" => McpTransport::Stdio as i32,
        "streamable_http" => McpTransport::StreamableHttp as i32,
        _ => {
            return Json(json!({"success": false, "message": "invalid transport"}));
        }
    };

    let stdio = body.stdio.map(|s| McpStdioConfig {
        command: s.command,
        args: s.args.unwrap_or_default(),
        env: s.env.unwrap_or_default(),
        cwd: s.cwd.unwrap_or_default(),
    });
    let http = body.http.map(|h| McpHttpConfig {
        url: h.url,
        headers: h.headers.unwrap_or_default(),
        auth_ref: h.auth_ref.unwrap_or_default(),
    });

    let budgets = body.budgets.map(|b| McpBudgets {
        tool_timeout_ms: b.tool_timeout_ms.unwrap_or(0),
        max_concurrency: b.max_concurrency.unwrap_or(0),
        max_tool_output_bytes: b.max_tool_output_bytes.unwrap_or(0),
    });

    let approval_policy = body.approval_policy.map(|p| McpApprovalPolicy {
        default_policy: p.default_policy.unwrap_or_default(),
        per_tool: p.per_tool.unwrap_or_default(),
    });

    let record = McpServerRecord {
        server_id: body.server_id.trim().to_string(),
        display_name: body.display_name.unwrap_or_default(),
        transport,
        stdio,
        http,
        tool_namespace: body.tool_namespace.unwrap_or_default(),
        allowed_tools: body.allowed_tools.unwrap_or_default(),
        approval_policy,
        budgets,
        updated_at_ms: 0,
    };

    let mut client = state.mcp_registry_client.clone();
    match client
        .upsert_mcp_server(crate::proto::sms::UpsertMcpServerRequest {
            record: Some(record),
        })
        .await
    {
        Ok(resp) => Json(json!({"success": true, "revision": resp.into_inner().revision})),
        Err(e) => Json(json!({"success": false, "message": e.to_string()})),
    }
}

async fn delete_mcp_server(state: GatewayState, p: Path<String>) -> Json<serde_json::Value> {
    let server_id = p.0;
    if server_id.trim().is_empty() {
        return Json(json!({"success": false, "message": "server_id is required"}));
    }
    let mut client = state.mcp_registry_client.clone();
    match client
        .delete_mcp_server(crate::proto::sms::DeleteMcpServerRequest {
            server_id: server_id.trim().to_string(),
        })
        .await
    {
        Ok(resp) => Json(json!({"success": true, "revision": resp.into_inner().revision})),
        Err(e) => Json(json!({"success": false, "message": e.to_string()})),
    }
}

#[derive(serde::Deserialize)]
struct CreateExecutionBody {
    task_id: String,
    node_uuid: Option<String>,
    request_id: Option<String>,
    execution_id: Option<String>,
    execution_mode: Option<String>,
}

#[derive(serde::Deserialize)]
pub(super) struct TerminateExecutionBody {
    reason: Option<String>,
}

#[derive(serde::Deserialize)]
pub(super) struct DestroyInstanceBody {
    node_uuid: String,
    reason: Option<String>,
}

fn parse_execution_mode(v: Option<&str>) -> i32 {
    match v.map(|s| s.to_ascii_lowercase()) {
        Some(s) if s == "async" => ExecutionMode::Async as i32,
        _ => ExecutionMode::Sync as i32,
    }
}

async fn list_ready_node_uuids_for_task(
    state: &GatewayState,
    task_id: &str,
) -> Result<Vec<String>, String> {
    let mut idx_client = state.execution_index_client.clone();
    let mut counts = std::collections::HashMap::<String, usize>::new();
    let now_ms = chrono::Utc::now().timestamp_millis();
    for inst in collect_task_instances_bounded(&mut idx_client, task_id, 100)
        .await
        .map_err(|e| format!("execution_index error: {e}"))?
    {
        if !instance_is_active_and_fresh(&state.config, inst.status, inst.last_seen_ms, now_ms) {
            continue;
        }
        *counts.entry(inst.node_uuid).or_insert(0) += 1;
    }

    let mut node_uuids: Vec<_> = counts.into_keys().collect();
    node_uuids.sort();
    Ok(node_uuids)
}

async fn trigger_assignment_reconcile_hints(
    state: &GatewayState,
    task_id: &str,
) -> Result<usize, String> {
    let mut assignment_client = state.task_assignment_client.clone();
    let assignments = assignment_client
        .list_task_assignments(tonic::Request::new(
            crate::proto::sms::ListTaskAssignmentsRequest {
                task_id: task_id.to_string(),
            },
        ))
        .await
        .map_err(|e| format!("task_assignment error: {e}"))?
        .into_inner()
        .assignments;

    let mut nudged = 0usize;
    for assignment in assignments {
        if assignment.desired_instances == 0 {
            continue;
        }
        let target = match node_rpc::resolve_node_channel(state, &assignment.node_uuid).await {
            Ok(target) => target,
            Err(_) => continue,
        };
        let mut client = InstanceServiceClient::new(target.channel);
        if client
            .reconcile_task_assignments_now(ReconcileTaskAssignmentsNowRequest {
                task_id: task_id.to_string(),
            })
            .await
            .is_ok()
        {
            nudged += 1;
        }
    }
    Ok(nudged)
}

pub(super) async fn terminate_execution_admin(
    state: GatewayState,
    p: Path<String>,
    axum::extract::Json(body): axum::extract::Json<TerminateExecutionBody>,
) -> Json<serde_json::Value> {
    use crate::proto::sms::GetExecutionRequest;
    let execution_id = p.0;
    if execution_id.trim().is_empty() {
        return Json(json!({"success": false, "message": "execution_id is required"}));
    }

    let mut exe_client = state.execution_index_client.clone();
    let exe = match exe_client
        .get_execution(tonic::Request::new(GetExecutionRequest {
            execution_id: execution_id.clone(),
        }))
        .await
    {
        Ok(r) => r.into_inner(),
        Err(e) => return Json(json!({"success": false, "message": e.to_string()})),
    };
    if !exe.found {
        return Json(json!({"success": false, "message": "execution not found"}));
    }
    let Some(exe) = exe.execution else {
        return Json(json!({"success": false, "message": "execution not found"}));
    };
    if exe.node_uuid.trim().is_empty() {
        return Json(json!({"success": false, "message": "node_uuid is missing for execution"}));
    }

    let target = match node_rpc::resolve_node_channel(&state, &exe.node_uuid).await {
        Ok(t) => t,
        Err(resp) => return resp,
    };

    let mut client = ExecutionServiceClient::new(target.channel);
    let reason = body.reason.unwrap_or_default();
    let resp = match client
        .terminate_execution(TerminateExecutionRequest {
            execution_id: execution_id.clone(),
            reason,
        })
        .await
    {
        Ok(r) => r.into_inner(),
        Err(e) => return Json(json!({"success": false, "message": e.to_string()})),
    };

    Json(json!({
        "success": resp.success,
        "node_uuid": target.node_uuid,
        "execution_id": execution_id,
        "final_status": resp.final_status,
        "message": resp.message,
    }))
}

pub(super) async fn destroy_instance_admin(
    state: GatewayState,
    p: Path<String>,
    axum::extract::Json(body): axum::extract::Json<DestroyInstanceBody>,
) -> Json<serde_json::Value> {
    let instance_id = p.0;
    if instance_id.trim().is_empty() {
        return Json(json!({"success": false, "message": "instance_id is required"}));
    }
    if body.node_uuid.trim().is_empty() {
        return Json(json!({"success": false, "message": "node_uuid is required"}));
    }

    let target = match node_rpc::resolve_node_channel(&state, &body.node_uuid).await {
        Ok(t) => t,
        Err(resp) => return resp,
    };

    let mut client = InstanceServiceClient::new(target.channel);
    let reason = body.reason.unwrap_or_default();
    let resp = match client
        .destroy_instance(DestroyInstanceRequest {
            instance_id: instance_id.clone(),
            reason,
        })
        .await
    {
        Ok(r) => r.into_inner(),
        Err(e) if e.code() == tonic::Code::NotFound => {
            return Json(json!({
                "success": true,
                "node_uuid": target.node_uuid,
                "instance_id": instance_id,
                "message": "instance already absent",
            }));
        }
        Err(e) => return Json(json!({"success": false, "message": e.to_string()})),
    };

    Json(json!({
        "success": resp.success,
        "node_uuid": target.node_uuid,
        "instance_id": instance_id,
        "message": resp.message,
    }))
}

async fn create_invocation(
    state: GatewayState,
    axum::extract::Json(body): axum::extract::Json<CreateExecutionBody>,
) -> Json<serde_json::Value> {
    // Admin BFF: one-shot execution submission with two-level scheduling.
    // Admin BFF：一次性提交执行请求，走“两层调度”（SMS placement → Spearlet execute）。
    if body.task_id.is_empty() {
        return Json(json!({ "success": false, "message": "task_id is required" }));
    }
    let request_id = body
        .request_id
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let execution_id = body
        .execution_id
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let mode = parse_execution_mode(body.execution_mode.as_deref());
    if let Some(node_uuid) = body.node_uuid.as_ref().filter(|s| !s.is_empty()) {
        let target = match node_rpc::resolve_node_channel(&state, node_uuid).await {
            Ok(t) => t,
            Err(resp) => return resp,
        };
        let mut invc = InvocationServiceClient::new(target.channel);
        let req =
            invocation_flow::build_invoke_request(&body.task_id, &request_id, &execution_id, mode);
        return match invc.invoke(req).await {
            Ok(resp) => {
                let inner = resp.into_inner();
                let (success, message) = invocation_flow::summarize_invoke_result(
                    inner.status,
                    inner.error.as_ref().map(|e| e.message.as_str()),
                );
                Json(invocation_flow::direct_invoke_success_json(
                    &target.node_uuid,
                    &inner.invocation_id,
                    &inner.execution_id,
                    &message,
                    success,
                ))
            }
            Err(e) => Json(json!({ "success": false, "message": e.to_string() })),
        };
    }
    let ready_node_uuids = match list_ready_node_uuids_for_task(&state, &body.task_id).await {
        Ok(v) => v,
        Err(e) => return Json(json!({ "success": false, "message": e })),
    };
    if ready_node_uuids.is_empty() {
        let nudged = trigger_assignment_reconcile_hints(&state, &body.task_id)
            .await
            .unwrap_or(0);
        return Json(json!({
            "success": false,
            "message": if nudged > 0 {
                "task replicas are warming up; retry after assigned nodes reconcile"
            } else {
                "task has no ready replicas"
            }
        }));
    }

    for node_uuid in ready_node_uuids {
        let target = match node_rpc::resolve_node_channel(&state, &node_uuid).await {
            Ok(t) => t,
            Err(_) => continue,
        };
        let mut invc = InvocationServiceClient::new(target.channel);
        let req =
            invocation_flow::build_invoke_request(&body.task_id, &request_id, &execution_id, mode);
        match invc.invoke(req).await {
            Ok(resp) => {
                let inner = resp.into_inner();
                let (success, message) = invocation_flow::summarize_invoke_result(
                    inner.status,
                    inner.error.as_ref().map(|e| e.message.as_str()),
                );
                if success {
                    return Json(json!({
                        "success": true,
                        "node_uuid": target.node_uuid,
                        "invocation_id": inner.invocation_id,
                        "execution_id": inner.execution_id,
                        "message": message,
                    }));
                }
                return Json(json!({ "success": false, "message": message }));
            }
            Err(_) => continue,
        }
    }

    Json(json!({ "success": false, "message": "all ready-instance nodes failed" }))
}

async fn list_nodes(state: GatewayState, Query(q): Query<ListQuery>) -> Json<serde_json::Value> {
    let mut client = state.node_client.clone();
    let req = ListNodesRequest {
        status_filter: q.status.unwrap_or_default(),
    };
    let resp = client.list_nodes(req).await.unwrap().into_inner();
    let total = resp.nodes.len();
    let mut list = resp
        .nodes
        .into_iter()
        .map(|n| {
            let name = n.metadata.get("name").cloned().unwrap_or_default();
            json!({
                "uuid": n.uuid,
                "name": name,
                "ip_address": n.ip_address,
                "port": n.port,
                "status": n.status,
                "last_heartbeat": n.last_heartbeat,
                "registered_at": n.registered_at,
                "metadata": n.metadata,
            })
        })
        .collect::<Vec<_>>();
    if let Some(q) = q.q.as_ref().map(|s| s.to_lowercase()) {
        list.retain(|item| {
            let uuid = item["uuid"].as_str().unwrap_or("").to_lowercase();
            let ip = item["ip_address"].as_str().unwrap_or("").to_lowercase();
            let meta = item["metadata"].as_object();
            let meta_hit = meta
                .map(|m| {
                    m.iter().any(|(k, v)| {
                        k.to_lowercase().contains(&q)
                            || v.as_str().unwrap_or("").to_lowercase().contains(&q)
                    })
                })
                .unwrap_or(false);
            uuid.contains(&q) || ip.contains(&q) || meta_hit
        });
    }
    let (field, asc) = if let Some(sort) = &q.sort {
        let mut parts = sort.split(':');
        let field = parts.next().unwrap_or("").to_string();
        let order = parts.next().unwrap_or("asc");
        let asc = order != "desc";
        (field, asc)
    } else if let Some(field) = &q.sort_by {
        let asc = q.order.as_deref().unwrap_or("asc") != "desc";
        (field.clone(), asc)
    } else {
        (String::new(), true)
    };
    if !field.is_empty() {
        match field.as_str() {
            "last_heartbeat" => list.sort_by_key(|i| i["last_heartbeat"].as_i64().unwrap_or(0)),
            "registered_at" => list.sort_by_key(|i| i["registered_at"].as_i64().unwrap_or(0)),
            _ => {}
        }
        if !asc {
            list.reverse();
        }
    }
    if let Some(offset) = q.offset {
        if offset < list.len() {
            list = list.split_off(offset);
        } else {
            list.clear();
        }
    }
    if let Some(limit) = q.limit {
        if limit < list.len() {
            list.truncate(limit);
        }
    }
    Json(json!({ "nodes": list, "total_count": total }))
}

async fn list_task_instances_admin(
    state: GatewayState,
    Path(task_id): Path<String>,
    Query(q): Query<PageTokenQuery>,
) -> Json<serde_json::Value> {
    use crate::proto::sms::{GetInstanceRequest, ListTaskInstancesRequest};
    let mut client = state.execution_index_client.clone();
    let limit = q.limit.unwrap_or(50).max(1);
    let page_token = q.page_token.unwrap_or_default();
    let resp = client
        .list_task_instances(tonic::Request::new(ListTaskInstancesRequest {
            task_id,
            limit,
            page_token,
        }))
        .await;
    match resp {
        Ok(r) => {
            let inner = r.into_inner();
            let mut instances = Vec::with_capacity(inner.instances.len());
            for i in inner.instances {
                let detail = client
                    .get_instance(tonic::Request::new(GetInstanceRequest {
                        instance_id: i.instance_id.clone(),
                    }))
                    .await
                    .ok()
                    .map(|resp| resp.into_inner().instance)
                    .flatten();
                instances.push(task_instance_row_json(i, detail.as_ref()));
            }
            Json(json!({
                "success": true,
                "instances": instances,
                "next_page_token": inner.next_page_token,
            }))
        }
        Err(e) => Json(json!({ "success": false, "message": e.to_string() })),
    }
}

async fn get_instance_admin(
    state: GatewayState,
    Path(instance_id): Path<String>,
) -> Json<serde_json::Value> {
    use crate::proto::sms::GetInstanceRequest;
    let mut client = state.execution_index_client.clone();
    let resp = client
        .get_instance(tonic::Request::new(GetInstanceRequest { instance_id }))
        .await;
    match resp {
        Ok(r) => {
            let inner = r.into_inner();
            let instance = inner.instance.map(instance_json);
            Json(json!({
                "success": true,
                "found": inner.found,
                "active": inner.active,
                "instance": instance,
            }))
        }
        Err(e) => Json(json!({ "success": false, "message": e.to_string() })),
    }
}

async fn list_instance_executions_admin(
    state: GatewayState,
    Path(instance_id): Path<String>,
    Query(q): Query<PageTokenQuery>,
) -> Json<serde_json::Value> {
    use crate::proto::sms::ListInstanceExecutionsRequest;
    let mut client = state.execution_index_client.clone();
    let limit = q.limit.unwrap_or(50).max(1);
    let page_token = q.page_token.unwrap_or_default();
    let resp = client
        .list_instance_executions(tonic::Request::new(ListInstanceExecutionsRequest {
            instance_id,
            limit,
            page_token,
        }))
        .await;
    match resp {
        Ok(r) => {
            let inner = r.into_inner();
            let executions = inner
                .executions
                .into_iter()
                .map(instance_execution_summary_json)
                .collect::<Vec<_>>();
            Json(json!({
                "success": true,
                "executions": executions,
                "next_page_token": inner.next_page_token,
            }))
        }
        Err(e) => Json(json!({ "success": false, "message": e.to_string() })),
    }
}

async fn list_execution_history_admin(
    state: GatewayState,
    Query(q): Query<ExecutionHistoryQuery>,
) -> Json<serde_json::Value> {
    use crate::proto::sms::ListExecutionsRequest;

    // This endpoint serves the durable execution archive, not the task/runtime view.
    // Task pages link here when users need records that survive replica cleanup.
    let mut client = state.execution_index_client.clone();
    let resp = client
        .list_executions(tonic::Request::new(ListExecutionsRequest {
            task_id: q.task_id.unwrap_or_default(),
            status: q.status.unwrap_or_default(),
            limit: q.limit.unwrap_or(100),
            page_token: q.page_token.unwrap_or_default(),
        }))
        .await;

    match resp {
        Ok(r) => {
            let inner = r.into_inner();
            let executions = inner
                .executions
                .into_iter()
                .map(execution_history_row_json)
                .collect::<Vec<_>>();
            Json(json!({
                "success": true,
                "executions": executions,
                "next_page_token": inner.next_page_token,
            }))
        }
        Err(e) => Json(json!({ "success": false, "message": e.to_string() })),
    }
}

async fn get_execution_admin(
    state: GatewayState,
    Path(execution_id): Path<String>,
) -> Json<serde_json::Value> {
    use crate::proto::sms::GetExecutionRequest;
    let mut client = state.execution_index_client.clone();
    let resp = client
        .get_execution(tonic::Request::new(GetExecutionRequest { execution_id }))
        .await;
    match resp {
        Ok(r) => {
            let inner = r.into_inner();
            if !inner.found {
                return Json(json!({ "success": true, "found": false }));
            }
            Json(json!({
                "success": true,
                "found": true,
                "execution": execution_detail_json(inner.execution.unwrap())
            }))
        }
        Err(e) => Json(json!({ "success": false, "message": e.to_string() })),
    }
}

async fn get_node_detail(state: GatewayState, Path(uuid): Path<String>) -> Json<serde_json::Value> {
    use crate::proto::sms::GetNodeWithResourceRequest;
    let mut client = state.node_client.clone();
    let resp = client
        .get_node_with_resource(GetNodeWithResourceRequest { uuid })
        .await
        .unwrap()
        .into_inner();
    Json(node_detail_json(resp.node, resp.resource))
}

async fn get_node_ai_credential_sync(
    state: GatewayState,
    Path(uuid): Path<String>,
) -> Json<serde_json::Value> {
    let target = match node_rpc::resolve_node_http_target(&state, &uuid).await {
        Ok(target) => target,
        Err(resp) => return resp,
    };
    let url = format!("{}/monitoring/ai/credentials", target.base_url);
    // Proxy one lightweight read-only monitoring call from SMS Web Admin to Spearlet.
    // 由 SMS Web Admin 代理一次只读的 Spearlet 本地监控请求。
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
    {
        Ok(client) => client,
        Err(e) => return Json(json!({"success": false, "message": e.to_string()})),
    };
    let resp = match client.get(&url).send().await {
        Ok(resp) => resp,
        Err(e) => return Json(json!({"success": false, "message": e.to_string()})),
    };
    if !resp.status().is_success() {
        return Json(json!({
            "success": false,
            "message": format!("upstream returned HTTP {}", resp.status()),
        }));
    }
    let monitoring = match resp.json::<serde_json::Value>().await {
        Ok(value) => value,
        Err(e) => return Json(json!({"success": false, "message": e.to_string()})),
    };
    Json(json!({
        "success": true,
        "node_uuid": target.node_uuid,
        "monitoring": monitoring,
    }))
}

async fn get_stats(state: GatewayState) -> Json<serde_json::Value> {
    use crate::proto::sms::ListNodesRequest;
    let mut client = state.node_client.clone();
    let resp = client
        .list_nodes(ListNodesRequest {
            status_filter: String::new(),
        })
        .await
        .unwrap()
        .into_inner();
    let now = chrono::Utc::now().timestamp();
    let mut total = 0i64;
    let mut online = 0i64;
    let mut offline = 0i64;
    let mut recent_60s = 0i64;
    for n in resp.nodes {
        total += 1;
        let s = n.status.to_lowercase();
        if s == "online" || s == "active" {
            online += 1;
        } else {
            offline += 1;
        }
        if now - n.last_heartbeat <= 60 {
            recent_60s += 1;
        }
    }
    Json(json!({
        "total_count": total,
        "online_count": online,
        "offline_count": offline,
        "recent_60s_count": recent_60s,
    }))
}

async fn admin_index() -> Html<&'static str> {
    Html(include_str!("../../assets/admin/index.html"))
}

async fn admin_static(headers: HeaderMap, Path(path): Path<String>) -> impl IntoResponse {
    let path = path.trim_start_matches('/');
    let enc = headers
        .get(axum::http::header::ACCEPT_ENCODING)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_lowercase();
    let (bytes, mime, content_encoding) = match (path, enc.as_str()) {
        ("main.js", enc) if enc.contains("br") => (
            include_bytes!(concat!(env!("OUT_DIR"), "/main.js.br")).as_ref(),
            "application/javascript",
            Some("br"),
        ),
        ("main.js", enc) if enc.contains("gzip") => (
            include_bytes!(concat!(env!("OUT_DIR"), "/main.js.gz")).as_ref(),
            "application/javascript",
            Some("gzip"),
        ),
        ("main.css", enc) if enc.contains("br") => (
            include_bytes!(concat!(env!("OUT_DIR"), "/main.css.br")).as_ref(),
            "text/css",
            Some("br"),
        ),
        ("main.css", enc) if enc.contains("gzip") => (
            include_bytes!(concat!(env!("OUT_DIR"), "/main.css.gz")).as_ref(),
            "text/css",
            Some("gzip"),
        ),
        ("index.html", enc) if enc.contains("br") => (
            include_bytes!(concat!(env!("OUT_DIR"), "/index.html.br")).as_ref(),
            "text/html",
            Some("br"),
        ),
        ("index.html", enc) if enc.contains("gzip") => (
            include_bytes!(concat!(env!("OUT_DIR"), "/index.html.gz")).as_ref(),
            "text/html",
            Some("gzip"),
        ),
        ("main.js", _) => (
            include_bytes!("../../assets/admin/main.js").as_ref(),
            "application/javascript",
            None,
        ),
        ("main.css", _) => (
            include_bytes!("../../assets/admin/main.css").as_ref(),
            "text/css",
            None,
        ),
        ("index.html", _) => (
            include_bytes!("../../assets/admin/index.html").as_ref(),
            "text/html",
            None,
        ),
        _ => return axum::http::StatusCode::NOT_FOUND.into_response(),
    };
    let mut resp = axum::response::Response::new(bytes.into());
    resp.headers_mut()
        .insert(CONTENT_TYPE, mime.parse().unwrap());
    // Reduce caching to ensure UI updates are visible / 减少缓存以确保前端更新可见
    let cache = match path {
        "index.html" | "main.js" | "main.css" => "no-cache, no-store, must-revalidate",
        _ => "public, max-age=31536000",
    };
    resp.headers_mut()
        .insert(CACHE_CONTROL, cache.parse().unwrap());
    resp.headers_mut()
        .insert(axum::http::header::VARY, "Accept-Encoding".parse().unwrap());
    if let Some(ce) = content_encoding {
        resp.headers_mut()
            .insert(axum::http::header::CONTENT_ENCODING, ce.parse().unwrap());
    }
    resp
}

async fn nodes_stream(
    state: GatewayState,
    Query(q): Query<StreamQuery>,
) -> impl axum::response::IntoResponse {
    use axum::response::sse::{Event, Sse};
    type DynSseStream = Pin<Box<dyn futures::Stream<Item = Result<Event, Infallible>> + Send>>;
    let mut client = state.node_client.clone();
    if q.once.unwrap_or(false) {
        let event = match client
            .list_nodes(ListNodesRequest {
                status_filter: String::new(),
            })
            .await
        {
            Ok(r) => {
                let nodes = r.into_inner().nodes;
                let payload =
                    serde_json::to_string(&json!({"type":"snapshot","count": nodes.len()}))
                        .unwrap();
                Event::default().event("snapshot").data(payload)
            }
            Err(_) => Event::default().event("error").data("{}"),
        };
        let single: DynSseStream = Box::pin(futures::stream::once(async move {
            Ok::<Event, Infallible>(event)
        }));
        return Sse::new(single);
    }

    let cancel = state.cancel_token.clone();
    let stream: DynSseStream = Box::pin(
        IntervalStream::new(tokio::time::interval(Duration::from_secs(5)))
            .take_until(cancel.cancelled_owned())
            .then(move |_| {
                let mut client = client.clone();
                async move {
                    match client
                        .list_nodes(ListNodesRequest {
                            status_filter: String::new(),
                        })
                        .await
                    {
                        Ok(r) => {
                            let nodes = r.into_inner().nodes;
                            let payload = serde_json::to_string(
                                &json!({"type":"snapshot","count": nodes.len()}),
                            )
                            .unwrap();
                            Ok::<Event, Infallible>(
                                Event::default().event("snapshot").data(payload),
                            )
                        }
                        Err(_) => {
                            Ok::<Event, Infallible>(Event::default().event("error").data("{}"))
                        }
                    }
                }
            }),
    );
    Sse::new(stream)
}
