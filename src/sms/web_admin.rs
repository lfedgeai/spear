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
    admin_credential_service_client::AdminCredentialServiceClient,
    admin_ai_config_service_client::AdminAiConfigServiceClient,
    backend_registry_service_client::BackendRegistryServiceClient,
    mcp_registry_service_client::McpRegistryServiceClient,
    model_deployment_registry_service_client::ModelDeploymentRegistryServiceClient,
    node_service_client::NodeServiceClient, placement_service_client::PlacementServiceClient,
    DeleteCredentialRequest, ListCredentialsRequest, ListNodesRequest, UpsertCredentialRequest,
};
use crate::sms::gateway::GatewayState;
pub use router::create_admin_router;
use types::{AiModelsQuery, ListQuery, PageTokenQuery, StreamQuery};

use crate::proto::spearlet::{
    execution_service_client::ExecutionServiceClient,
    instance_service_client::InstanceServiceClient,
    invocation_service_client::InvocationServiceClient, DestroyInstanceRequest, ExecutionMode,
    TerminateExecutionRequest,
};

mod ai_model_catalog;
mod invocation_flow;
mod node_rpc;
mod router;
mod types;

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
        let admin_credential_client = AdminCredentialServiceClient::new(channel.clone());
        let admin_ai_config_client = AdminAiConfigServiceClient::new(channel.clone());
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
            admin_credential_client,
            admin_ai_config_client,
            model_deployment_registry_client,
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
pub(crate) struct UpsertRemoteBackendBody {
    name: String,
    kind: String,
    base_url: String,
    model: Option<String>,
    credential_ref: Option<String>,
    weight: Option<u32>,
    priority: Option<i32>,
    operations: Vec<String>,
    features: Option<Vec<String>>,
    transports: Option<Vec<String>>,
    provider: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct UpsertCredentialBody {
    name: String,
    secret: Option<String>,
    description: Option<String>,
    disabled: Option<bool>,
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
            let meta = inner.snapshot.as_ref().map(|s| {
                json!({
                    "revision": s.revision,
                    "reported_at_ms": s.reported_at_ms,
                })
            });
            let backends = inner
                .snapshot
                .as_ref()
                .map(|s| {
                    s.backends
                        .iter()
                        .filter_map(|b| {
                            b.spec.as_ref().map(|spec| {
                                json!({
                                    "name": spec.name,
                                    "kind": spec.kind,
                                    "operations": spec.operations,
                                    "features": spec.features,
                                    "transports": spec.transports,
                                    "weight": spec.weight,
                                    "priority": spec.priority,
                                    "base_url": spec.base_url,
                                    "status": b.status,
                                    "status_reason": b.status_reason,
                                    "provider": spec.provider,
                                    "model": spec.model,
                                    "hosting": spec.hosting,
                                })
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            Json(json!({
                "found": inner.found,
                "node_uuid": node_uuid,
                "backends": backends,
                "snapshot": meta,
            }))
        }
        Err(e) => Json(json!({"found": false, "message": e.to_string()})),
    }
}

async fn list_ai_models(
    state: GatewayState,
    Query(q): Query<AiModelsQuery>,
) -> Json<serde_json::Value> {
    use crate::proto::sms::ListNodeBackendSnapshotsRequest;

    let mut client = state.backend_registry_client.clone();
    let limit = q.limit.unwrap_or(500) as u32;
    let offset = q.offset.unwrap_or(0) as u32;
    let resp = match client
        .list_node_backend_snapshots(ListNodeBackendSnapshotsRequest { limit, offset })
        .await
    {
        Ok(r) => r.into_inner(),
        Err(e) => return Json(json!({"success": false, "message": e.to_string()})),
    };

    let provider_filter = q.provider.as_deref().map(|s| s.to_ascii_lowercase());
    let hosting_filter = q.hosting.as_deref().map(|s| s.to_ascii_lowercase());
    let needle = q.q.as_deref().map(|s| s.to_ascii_lowercase());

    let list = ai_model_catalog::aggregate_models(
        resp.snapshots,
        provider_filter.as_deref(),
        hosting_filter.as_deref(),
        needle.as_deref(),
    );

    Json(json!({"success": true, "models": list, "total_count": resp.total_count}))
}

async fn get_ai_model_detail(
    state: GatewayState,
    p: Path<(String, String)>,
    Query(q_in): Query<AiModelsQuery>,
) -> Json<serde_json::Value> {
    let (provider, model) = p.0;
    if provider.trim().is_empty() || model.trim().is_empty() {
        return Json(json!({"success": true, "found": false}));
    }

    let q = AiModelsQuery {
        hosting: q_in.hosting.clone(),
        provider: Some(provider.clone()),
        limit: Some(500),
        offset: Some(0),
        q: Some(model.clone()),
    };

    let resp = list_ai_models(state, Query(q)).await;
    let mut found = None;
    if let Some(models) = resp.0.get("models").and_then(|v| v.as_array()) {
        for m in models.iter() {
            let p = m.get("provider").and_then(|v| v.as_str()).unwrap_or("");
            let md = m.get("model").and_then(|v| v.as_str()).unwrap_or("");
            if p == provider && md == model {
                found = Some(m.clone());
                break;
            }
        }
    }
    match found {
        Some(m) => Json(json!({"success": true, "found": true, "model": m})),
        None => Json(json!({"success": true, "found": false})),
    }
}

#[derive(Deserialize)]
pub(crate) struct CreateNodeModelDeploymentBody {
    provider: String,
    model: String,
    params: Option<std::collections::HashMap<String, String>>,
}

async fn list_node_model_deployments(
    state: GatewayState,
    p: Path<String>,
    Query(q): Query<ListQuery>,
) -> Json<serde_json::Value> {
    use crate::proto::sms::ListModelDeploymentsRequest;
    let node_uuid = p.0;
    if node_uuid.trim().is_empty() {
        return Json(json!({"success": false, "message": "node_uuid is required"}));
    }

    let limit = q.limit.unwrap_or(200) as u32;
    let offset = q.offset.unwrap_or(0) as u32;
    let mut client = state.model_deployment_registry_client.clone();
    let resp = match client
        .list_model_deployments(ListModelDeploymentsRequest {
            limit,
            offset,
            target_node_uuid: node_uuid.clone(),
            provider: String::new(),
        })
        .await
    {
        Ok(r) => r.into_inner(),
        Err(e) => return Json(json!({"success": false, "message": e.to_string()})),
    };

    let records = resp
        .records
        .into_iter()
        .map(|r| {
            json!({
                "deployment_id": r.deployment_id,
                "revision": r.revision,
                "created_at_ms": r.created_at_ms,
                "updated_at_ms": r.updated_at_ms,
                "spec": r.spec.as_ref().map(|s| json!({
                    "target_node_uuid": s.target_node_uuid,
                    "provider": s.provider,
                    "model": s.model,
                    "params": s.params,
                })),
                "status": r.status.as_ref().map(|s| json!({
                    "phase": s.phase,
                    "message": s.message,
                    "updated_at_ms": s.updated_at_ms,
                })),
            })
        })
        .collect::<Vec<_>>();

    Json(json!({
        "success": true,
        "revision": resp.revision,
        "total_count": resp.total_count,
        "deployments": records,
    }))
}

async fn create_node_model_deployment(
    state: GatewayState,
    p: Path<String>,
    body: Json<CreateNodeModelDeploymentBody>,
) -> Json<serde_json::Value> {
    use crate::proto::sms::{
        ModelDeploymentRecord, ModelDeploymentSpec, UpsertModelDeploymentRequest,
    };
    let node_uuid = p.0;
    if node_uuid.trim().is_empty() {
        return Json(json!({"success": false, "message": "node_uuid is required"}));
    }
    if body.provider.trim().is_empty() || body.model.trim().is_empty() {
        return Json(json!({"success": false, "message": "provider and model are required"}));
    }
    let provider_lc = body.provider.trim().to_ascii_lowercase();
    let provider = match provider_lc.as_str() {
        "vllm" => "vllm".to_string(),
        "llamacpp" | "llama_cpp" | "llama.cpp" => "llamacpp".to_string(),
        _ => {
            return Json(json!({
                "success": false,
                "message": "unsupported provider (only vllm/llamacpp supported)"
            }))
        }
    };
    let spec = ModelDeploymentSpec {
        target_node_uuid: node_uuid.clone(),
        provider,
        model: body.model.clone(),
        params: body.params.clone().unwrap_or_default(),
        backend: None,
    };
    let record = ModelDeploymentRecord {
        deployment_id: String::new(),
        revision: 0,
        created_at_ms: 0,
        updated_at_ms: 0,
        spec: Some(spec),
        status: None,
    };

    let mut client = state.model_deployment_registry_client.clone();
    let resp = match client
        .upsert_model_deployment(UpsertModelDeploymentRequest {
            record: Some(record),
        })
        .await
    {
        Ok(r) => r.into_inner(),
        Err(e) => return Json(json!({"success": false, "message": e.to_string()})),
    };

    Json(json!({
        "success": true,
        "deployment_id": resp.deployment_id,
        "revision": resp.revision,
    }))
}

async fn delete_node_model_deployment(
    state: GatewayState,
    p: Path<(String, String)>,
) -> Json<serde_json::Value> {
    use crate::proto::sms::DeleteModelDeploymentRequest;
    let (node_uuid, deployment_id) = p.0;
    if node_uuid.trim().is_empty() || deployment_id.trim().is_empty() {
        return Json(
            json!({"success": false, "message": "node_uuid and deployment_id are required"}),
        );
    }
    let mut client = state.model_deployment_registry_client.clone();
    let resp = match client
        .delete_model_deployment(DeleteModelDeploymentRequest { deployment_id })
        .await
    {
        Ok(r) => r.into_inner(),
        Err(e) => return Json(json!({"success": false, "message": e.to_string()})),
    };
    Json(json!({"success": true, "revision": resp.revision}))
}

async fn list_remote_backends_admin(state: GatewayState) -> Json<serde_json::Value> {
    let mut client = state.admin_ai_config_client.clone();
    match client
        .list_remote_backends(crate::proto::sms::ListRemoteBackendsRequest {})
        .await
    {
        Ok(resp) => {
            let inner = resp.into_inner();
            let backends = inner
                .backends
                .into_iter()
                .map(|b| {
                    json!({
                        "name": b.name,
                        "kind": b.kind,
                        "base_url": b.base_url,
                        "model": if b.model.is_empty() { None::<String> } else { Some(b.model) },
                        "credential_ref": if b.credential_ref.is_empty() { None::<String> } else { Some(b.credential_ref) },
                        "weight": b.weight,
                        "priority": b.priority,
                        "operations": b.operations,
                        "features": b.features,
                        "transports": b.transports,
                        "provider": if b.provider.is_empty() { None::<String> } else { Some(b.provider) },
                    })
                })
                .collect::<Vec<_>>();
            Json(json!({"success": true, "revision": inner.revision, "backends": backends}))
        }
        Err(e) => Json(json!({"success": false, "message": e.to_string()})),
    }
}

async fn upsert_remote_backend_admin(
    state: GatewayState,
    body: Json<UpsertRemoteBackendBody>,
) -> Json<serde_json::Value> {
    let b = body.0;
    if b.name.trim().is_empty() || b.kind.trim().is_empty() || b.base_url.trim().is_empty() {
        return Json(json!({"success": false, "message": "name/kind/base_url are required"}));
    }
    if b.operations.is_empty() {
        return Json(json!({"success": false, "message": "operations are required"}));
    }

    let mut client = state.admin_ai_config_client.clone();
    let resp = match client
        .upsert_remote_backend(crate::proto::sms::UpsertRemoteBackendRequest {
            backend: Some(crate::proto::sms::BackendSpec {
                name: b.name,
                kind: b.kind,
                operations: b.operations,
                features: b.features.unwrap_or_default(),
                transports: b.transports.unwrap_or_else(|| vec!["http".to_string()]),
                provider: b.provider.unwrap_or_default(),
                model: b.model.unwrap_or_default(),
                base_url: b.base_url,
                credential_ref: b.credential_ref.unwrap_or_default(),
                weight: b.weight.unwrap_or(100),
                priority: b.priority.unwrap_or(0),
                hosting: crate::proto::sms::BackendHosting::Remote as i32,
                origin: crate::proto::sms::BackendOrigin::Sms as i32,
                deployment_id: String::new(),
            }),
        })
        .await
    {
        Ok(r) => r.into_inner(),
        Err(e) => return Json(json!({"success": false, "message": e.to_string()})),
    };

    Json(json!({"success": true, "revision": resp.revision}))
}

async fn delete_remote_backend_admin(
    state: GatewayState,
    p: Path<String>,
) -> Json<serde_json::Value> {
    let name = p.0;
    if name.trim().is_empty() {
        return Json(json!({"success": false, "message": "name is required"}));
    }
    let mut client = state.admin_ai_config_client.clone();
    let resp = match client
        .delete_remote_backend(crate::proto::sms::DeleteRemoteBackendRequest { name })
        .await
    {
        Ok(r) => r.into_inner(),
        Err(e) => return Json(json!({"success": false, "message": e.to_string()})),
    };
    Json(json!({"success": true, "revision": resp.revision, "deleted": resp.deleted}))
}

async fn list_credentials_admin(state: GatewayState) -> Json<serde_json::Value> {
    let mut client = state.admin_credential_client.clone();
    match client.list_credentials(ListCredentialsRequest {}).await {
        Ok(resp) => {
            let inner = resp.into_inner();
            let mut backend_client = state.admin_ai_config_client.clone();
            let referenced_by_count = match backend_client
                .list_remote_backends(crate::proto::sms::ListRemoteBackendsRequest {})
                .await
            {
                Ok(resp) => {
                    // Reuse the admin remote backend snapshot to surface lightweight reference counts.
                    // 复用 admin remote backend 快照，为页面提供轻量级引用计数。
                    let mut counts = std::collections::HashMap::<String, usize>::new();
                    for backend in resp.into_inner().backends {
                        let credential_ref = backend.credential_ref.trim();
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
                    json!({
                        "name": c.name,
                        "provider_kind": c.provider_kind,
                        "version": c.version,
                        "description": if c.description.is_empty() { None::<String> } else { Some(c.description) },
                        "disabled": c.disabled,
                        "created_at_ms": c.created_at_ms,
                        "updated_at_ms": c.updated_at_ms,
                        "referenced_by_count": referenced_by_count.get(&c.name).copied().unwrap_or(0),
                    })
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
                .map(|s| {
                    let stdio = s.stdio.as_ref().map(|x| {
                        json!({
                            "command": x.command,
                            "args": x.args,
                            "env": x.env,
                            "cwd": x.cwd,
                        })
                    });
                    let http = s.http.as_ref().map(|x| {
                        json!({
                            "url": x.url,
                            "headers": x.headers,
                            "auth_ref": x.auth_ref,
                        })
                    });
                    let approval_policy = s.approval_policy.as_ref().map(|x| {
                        json!({
                            "default_policy": x.default_policy,
                            "per_tool": x.per_tool,
                        })
                    });
                    let budgets = s.budgets.as_ref().map(|x| {
                        json!({
                            "tool_timeout_ms": x.tool_timeout_ms,
                            "max_concurrency": x.max_concurrency,
                            "max_tool_output_bytes": x.max_tool_output_bytes,
                        })
                    });
                    json!({
                        "server_id": s.server_id,
                        "display_name": s.display_name,
                        "transport": s.transport,
                        "stdio": stdio,
                        "http": http,
                        "tool_namespace": s.tool_namespace,
                        "allowed_tools": s.allowed_tools,
                        "approval_policy": approval_policy,
                        "budgets": budgets,
                        "updated_at_ms": s.updated_at_ms,
                    })
                })
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
        let stdio = s.stdio.as_ref().map(|x| {
            json!({
                "command": x.command,
                "args": x.args,
                "env": x.env,
                "cwd": x.cwd,
            })
        });
        let http = s.http.as_ref().map(|x| {
            json!({
                "url": x.url,
                "headers": x.headers,
                "auth_ref": x.auth_ref,
            })
        });
        let approval_policy = s.approval_policy.as_ref().map(|x| {
            json!({
                "default_policy": x.default_policy,
                "per_tool": x.per_tool,
            })
        });
        let budgets = s.budgets.as_ref().map(|x| {
            json!({
                "tool_timeout_ms": x.tool_timeout_ms,
                "max_concurrency": x.max_concurrency,
                "max_tool_output_bytes": x.max_tool_output_bytes,
            })
        });
        let server = json!({
            "server_id": s.server_id,
            "display_name": s.display_name,
            "transport": s.transport,
            "stdio": stdio,
            "http": http,
            "tool_namespace": s.tool_namespace,
            "allowed_tools": s.allowed_tools,
            "approval_policy": approval_policy,
            "budgets": budgets,
            "updated_at_ms": s.updated_at_ms,
        });
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
    max_candidates: Option<u32>,
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
    let max_candidates = body.max_candidates.unwrap_or(3);

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

    let mut placement = state.placement_client.clone();
    // Step 1: ask SMS to return an ordered list of candidate nodes.
    // 第一步：调用 SMS placement，拿到有序候选节点列表。
    let placement_resp = placement
        .place_invocation(crate::proto::sms::PlaceInvocationRequest {
            request_id: request_id.clone(),
            task_id: body.task_id.clone(),
            max_candidates,
            labels: std::collections::HashMap::new(),
        })
        .await;
    let placement_resp = match placement_resp {
        Ok(r) => r.into_inner(),
        Err(e) => {
            return Json(json!({ "success": false, "message": e.to_string() }));
        }
    };
    if placement_resp.candidates.is_empty() {
        return Json(json!({ "success": false, "message": "no candidates" }));
    }

    for c in placement_resp.candidates.iter() {
        // Step 2: try candidates in order (spillback).
        // 第二步：按顺序尝试候选节点（spillback）。
        let channel = node_rpc::connect_lazy_node_channel(&c.ip_address, c.port);
        let Some(channel) = channel else {
            // Node address is invalid: treat as unavailable and spillback.
            // 节点地址不可用：按 unavailable 处理并继续 spillback。
            let _ = placement
                .report_invocation_outcome(crate::sms::placement::outcome::build_outcome_request(
                    &placement_resp.decision_id,
                    &request_id,
                    &body.task_id,
                    &c.node_uuid,
                    crate::proto::sms::InvocationOutcomeClass::Unavailable as i32,
                    "invalid node url".to_string(),
                ))
                .await;
            continue;
        };
        let mut invc = InvocationServiceClient::new(channel);
        // Use ExistingTask invocation; Spearlet may fetch task from SMS when missing.
        // 使用 ExistingTask 调用；若节点本地缺 task，Spearlet 会从 SMS 拉取补齐后执行。
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
                    // Success: report feedback to SMS and return.
                    // 成功：回报给 SMS 用于后续 placement，然后直接返回。
                    let _ = placement
                        .report_invocation_outcome(crate::sms::placement::outcome::build_outcome_request(
                            &placement_resp.decision_id,
                            &request_id,
                            &body.task_id,
                            &c.node_uuid,
                            crate::proto::sms::InvocationOutcomeClass::Success as i32,
                            String::new(),
                        ))
                        .await;
                    return Json(json!({
                        "success": true,
                        "decision_id": placement_resp.decision_id,
                        "node_uuid": c.node_uuid,
                        "invocation_id": inner.invocation_id,
                        "execution_id": inner.execution_id,
                        "message": message,
                    }));
                }
                // Function-level failure is not retryable here: return immediately.
                // Function 级失败在这里不做重试：直接返回给前端。
                let _ = placement
                    .report_invocation_outcome(crate::sms::placement::outcome::build_outcome_request(
                        &placement_resp.decision_id,
                        &request_id,
                        &body.task_id,
                        &c.node_uuid,
                        crate::proto::sms::InvocationOutcomeClass::Internal as i32,
                        message.clone(),
                    ))
                    .await;
                return Json(json!({ "success": false, "message": message }));
            }
            Err(e) => {
                // gRPC error classification decides whether to spillback.
                // gRPC 错误分类用于决定是否继续 spillback。
                let class = crate::sms::placement::outcome::classify_invoke_rpc_error(e.code());
                let _ = placement
                    .report_invocation_outcome(crate::sms::placement::outcome::build_outcome_request(
                        &placement_resp.decision_id,
                        &request_id,
                        &body.task_id,
                        &c.node_uuid,
                        class,
                        e.to_string(),
                    ))
                    .await;
                if crate::sms::placement::outcome::is_terminal_outcome_class(class) {
                    // Non-retryable: stop spillback.
                    // 不可重试：终止 spillback。
                    return Json(json!({ "success": false, "message": e.to_string() }));
                }
                continue;
            }
        }
    }
    // All candidates exhausted.
    // 所有候选均失败。
    Json(json!({ "success": false, "message": "all candidates failed" }))
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

async fn list_tasks(state: GatewayState, Query(q): Query<ListQuery>) -> Json<serde_json::Value> {
    use crate::proto::sms::ListTasksRequest;
    let mut client = state.task_client.clone();
    let resp = client
        .list_tasks(ListTasksRequest {
            node_uuid: String::new(),
            status_filter: -1,
            priority_filter: -1,
            limit: 0,
            offset: 0,
        })
        .await
        .unwrap()
        .into_inner();
    let total = resp.total_count as usize;
    let mut list = resp
        .tasks
        .into_iter()
        .map(|t| {
            let status = crate::sms::task_status_to_public_str(t.status);
            let priority = crate::sms::task_priority_to_public_str(t.priority);
            let (exec_type, exec_uri, exec_name) = if let Some(exec) = t.executable {
                let et = match exec.r#type {
                    1 => "binary",
                    2 => "script",
                    3 => "container",
                    4 => "wasm",
                    5 => "process",
                    _ => "unknown",
                };
                (et.to_string(), exec.uri, exec.name)
            } else {
                (String::new(), String::new(), String::new())
            };
            json!({
                "task_id": t.task_id,
                "name": t.name,
                "description": t.description,
                "status": status,
                "priority": priority,
                "node_uuid": t.node_uuid,
                "endpoint": t.endpoint,
                "version": t.version,
                "capabilities": t.capabilities,
                "registered_at": t.registered_at,
                "last_heartbeat": t.last_heartbeat,
                "metadata": t.metadata,
                "config": t.config,
                "executable_type": exec_type,
                "executable_uri": exec_uri,
                "executable_name": exec_name,
                "result_uris": t.result_uris,
                "last_result_uri": t.last_result_uri,
                "last_result_status": t.last_result_status,
                "last_completed_at": t.last_completed_at,
                "last_result_metadata": t.last_result_metadata,
            })
        })
        .collect::<Vec<_>>();
    if let Some(qs) = q.q.as_ref().map(|s| s.to_lowercase()) {
        list.retain(|item| {
            let id = item["task_id"].as_str().unwrap_or("").to_lowercase();
            let name = item["name"].as_str().unwrap_or("").to_lowercase();
            let node = item["node_uuid"].as_str().unwrap_or("").to_lowercase();
            let endpoint = item["endpoint"].as_str().unwrap_or("").to_lowercase();
            let meta = item["metadata"].as_object();
            let meta_hit = meta
                .map(|m| {
                    m.iter().any(|(k, v)| {
                        k.to_lowercase().contains(&qs)
                            || v.as_str().unwrap_or("").to_lowercase().contains(&qs)
                    })
                })
                .unwrap_or(false);
            id.contains(&qs)
                || name.contains(&qs)
                || node.contains(&qs)
                || endpoint.contains(&qs)
                || meta_hit
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
    Json(json!({ "tasks": list, "total_count": total }))
}

fn instance_status_to_public_str(v: i32) -> &'static str {
    use crate::proto::sms::InstanceStatus;
    match InstanceStatus::try_from(v).unwrap_or(InstanceStatus::Unknown) {
        InstanceStatus::Running => "running",
        InstanceStatus::Idle => "idle",
        InstanceStatus::Terminating => "terminating",
        InstanceStatus::Terminated => "terminated",
        InstanceStatus::Unknown => "unknown",
    }
}

fn execution_status_to_public_str(v: i32) -> &'static str {
    use crate::proto::sms::ExecutionStatus;
    match ExecutionStatus::try_from(v).unwrap_or(ExecutionStatus::Unknown) {
        ExecutionStatus::Pending => "pending",
        ExecutionStatus::Running => "running",
        ExecutionStatus::Completed => "completed",
        ExecutionStatus::Failed => "failed",
        ExecutionStatus::Cancelled => "cancelled",
        ExecutionStatus::Timeout => "timeout",
        ExecutionStatus::Unknown => "unknown",
    }
}

async fn list_task_instances_admin(
    state: GatewayState,
    Path(task_id): Path<String>,
    Query(q): Query<PageTokenQuery>,
) -> Json<serde_json::Value> {
    use crate::proto::sms::ListTaskInstancesRequest;
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
            let instances = inner
                .instances
                .into_iter()
                .map(|i| {
                    let s = instance_status_to_public_str(i.status).to_string();
                    json!({
                        "instance_id": i.instance_id,
                        "node_uuid": i.node_uuid,
                        "status": s,
                        "last_seen_ms": i.last_seen_ms,
                        "current_execution_id": i.current_execution_id,
                    })
                })
                .collect::<Vec<_>>();
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
            let instance = inner.instance.map(|i| {
                json!({
                    "instance_id": i.instance_id,
                    "task_id": i.task_id,
                    "node_uuid": i.node_uuid,
                    "status": instance_status_to_public_str(i.status),
                    "created_at_ms": i.created_at_ms,
                    "updated_at_ms": i.updated_at_ms,
                    "last_seen_ms": i.last_seen_ms,
                    "current_execution_id": i.current_execution_id,
                    "metadata": i.metadata,
                })
            });
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
                .map(|e| {
                    let s = execution_status_to_public_str(e.status).to_string();
                    json!({
                        "execution_id": e.execution_id,
                        "task_id": e.task_id,
                        "status": s,
                        "started_at_ms": e.started_at_ms,
                        "completed_at_ms": e.completed_at_ms,
                        "function_name": e.function_name,
                    })
                })
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
            let e = inner.execution.unwrap();
            let status = execution_status_to_public_str(e.status).to_string();
            let log_ref = e.log_ref.map(|lr| {
                json!({
                    "backend": lr.backend,
                    "uri_prefix": lr.uri_prefix,
                    "content_type": lr.content_type,
                    "compression": lr.compression,
                })
            });
            Json(json!({
                "success": true,
                "found": true,
                "execution": {
                    "execution_id": e.execution_id,
                    "invocation_id": e.invocation_id,
                    "task_id": e.task_id,
                    "function_name": e.function_name,
                    "node_uuid": e.node_uuid,
                    "instance_id": e.instance_id,
                    "status": status,
                    "started_at_ms": e.started_at_ms,
                    "completed_at_ms": e.completed_at_ms,
                    "updated_at_ms": e.updated_at_ms,
                    "metadata": e.metadata,
                    "log_ref": log_ref,
                }
            }))
        }
        Err(e) => Json(json!({ "success": false, "message": e.to_string() })),
    }
}

#[derive(serde::Deserialize)]
struct CreateExecutableBody {
    r#type: Option<String>,
    uri: Option<String>,
    name: Option<String>,
    checksum_sha256: Option<String>,
    args: Option<Vec<String>>,
    env: Option<std::collections::HashMap<String, String>>,
}

#[derive(serde::Deserialize)]
struct CreateTaskBody {
    name: String,
    description: Option<String>,
    priority: Option<String>,
    node_uuid: String,
    endpoint: String,
    version: String,
    capabilities: Option<Vec<String>>,
    metadata: Option<std::collections::HashMap<String, String>>,
    config: Option<std::collections::HashMap<String, String>>,
    executable: Option<CreateExecutableBody>,
}

async fn create_task(
    state: GatewayState,
    axum::extract::Json(body): axum::extract::Json<CreateTaskBody>,
) -> Json<serde_json::Value> {
    use crate::proto::sms::{ExecutableType, RegisterTaskRequest, TaskExecutable, TaskPriority};
    let p = match body.priority.as_ref().map(|s| s.to_ascii_lowercase()) {
        Some(s) if s == "low" => TaskPriority::Low as i32,
        Some(s) if s == "high" => TaskPriority::High as i32,
        Some(s) if s == "urgent" => TaskPriority::Urgent as i32,
        Some(s) if s == "unknown" => TaskPriority::Unknown as i32,
        _ => TaskPriority::Normal as i32,
    };
    let exe = body.executable.as_ref().map(|e| {
        let t = match e.r#type.as_ref().map(|s| s.to_ascii_lowercase()) {
            Some(s) if s == "binary" => ExecutableType::Binary as i32,
            Some(s) if s == "script" => ExecutableType::Script as i32,
            Some(s) if s == "container" => ExecutableType::Container as i32,
            Some(s) if s == "wasm" => ExecutableType::Wasm as i32,
            Some(s) if s == "process" => ExecutableType::Process as i32,
            _ => ExecutableType::Unknown as i32,
        };
        TaskExecutable {
            r#type: t,
            uri: e.uri.clone().unwrap_or_default(),
            name: e.name.clone().unwrap_or_default(),
            checksum_sha256: e.checksum_sha256.clone().unwrap_or_default(),
            args: e.args.clone().unwrap_or_default(),
            env: e.env.clone().unwrap_or_default(),
        }
    });
    let meta = body.metadata.clone().unwrap_or_default();
    let req = RegisterTaskRequest {
        name: body.name,
        description: body.description.unwrap_or_default(),
        priority: p,
        node_uuid: body.node_uuid,
        endpoint: body.endpoint,
        version: body.version,
        capabilities: body.capabilities.unwrap_or_default(),
        metadata: meta.clone(),
        config: body.config.unwrap_or_default(),
        executable: exe,
    };
    let mut client = state.task_client.clone();
    let resp = client.register_task(tonic::Request::new(req)).await;
    match resp {
        Ok(r) => {
            let inner = r.into_inner();
            Json(
                json!({ "success": inner.success, "task_id": inner.task_id, "message": inner.message }),
            )
        }
        Err(e) => Json(json!({ "success": false, "message": e.to_string() })),
    }
}

async fn get_task_detail(
    state: GatewayState,
    Path(task_id): Path<String>,
) -> Json<serde_json::Value> {
    use crate::proto::sms::GetTaskRequest;
    let mut client = state.task_client.clone();
    let resp = client.get_task(GetTaskRequest { task_id }).await;
    match resp {
        Ok(r) => {
            let inner = r.into_inner();
            if let Some(t) = inner.task {
                let status = crate::sms::task_status_to_public_str(t.status);
                let priority = crate::sms::task_priority_to_public_str(t.priority);
                let (exec_type, exec_uri, exec_name, exec_sum, exec_args, exec_env) =
                    if let Some(exec) = t.executable {
                        let et = match exec.r#type {
                            1 => "binary",
                            2 => "script",
                            3 => "container",
                            4 => "wasm",
                            5 => "process",
                            _ => "unknown",
                        };
                        (
                            et.to_string(),
                            exec.uri,
                            exec.name,
                            exec.checksum_sha256,
                            exec.args,
                            exec.env,
                        )
                    } else {
                        (
                            String::new(),
                            String::new(),
                            String::new(),
                            String::new(),
                            vec![],
                            std::collections::HashMap::new(),
                        )
                    };
                Json(json!({
                    "found": true,
                    "task": {
                        "task_id": t.task_id,
                        "name": t.name,
                        "description": t.description,
                        "status": status,
                        "priority": priority,
                        "node_uuid": t.node_uuid,
                        "endpoint": t.endpoint,
                        "version": t.version,
                        "capabilities": t.capabilities,
                        "registered_at": t.registered_at,
                        "last_heartbeat": t.last_heartbeat,
                        "metadata": t.metadata,
                        "config": t.config,
                        "executable_type": exec_type,
                        "executable_uri": exec_uri,
                        "executable_name": exec_name,
                        "executable_checksum": exec_sum,
                        "executable_args": exec_args,
                        "executable_env": exec_env,
                        "result_uris": t.result_uris,
                        "last_result_uri": t.last_result_uri,
                        "last_result_status": t.last_result_status,
                        "last_completed_at": t.last_completed_at,
                        "last_result_metadata": t.last_result_metadata,
                    }
                }))
            } else {
                Json(json!({"found": false}))
            }
        }
        Err(_) => Json(json!({"found": false})),
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
    let node = resp.node;
    let resource = resp.resource;
    let body = json!({
        "found": node.is_some(),
        "node": node.map(|n| json!({
            "uuid": n.uuid,
            "ip_address": n.ip_address,
            "port": n.port,
            "status": n.status,
            "last_heartbeat": n.last_heartbeat,
            "registered_at": n.registered_at,
            "metadata": n.metadata,
        })),
        "resource": resource.map(|r| json!({
            "cpu_usage_percent": r.cpu_usage_percent,
            "memory_usage_percent": r.memory_usage_percent,
            "disk_usage_percent": r.disk_usage_percent,
            "total_memory_bytes": r.total_memory_bytes,
            "used_memory_bytes": r.used_memory_bytes,
            "available_memory_bytes": r.available_memory_bytes,
        })),
    });
    Json(body)
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
