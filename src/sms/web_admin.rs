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
    ai_backend_control_plane_service_client::AiBackendControlPlaneServiceClient,
    backend_registry_service_client::BackendRegistryServiceClient,
    mcp_registry_service_client::McpRegistryServiceClient, node_service_client::NodeServiceClient,
    placement_service_client::PlacementServiceClient, ListNodesRequest,
};
use crate::sms::gateway::GatewayState;
use crate::sms::node_api::{
    admin_node_detail_response, node_to_admin_list_item, AdminNodeListResponse,
};
use crate::sms::query_support::{collect_task_instances_bounded, instance_is_active_and_fresh};
use crate::sms::runtime_api::{
    execution_summary_to_row, execution_to_detail, execution_to_history_row, instance_to_detail,
    task_instance_row, AdminExecutionDetailEnvelope, AdminExecutionHistoryListResponse,
    AdminExecutionMutationResponse, AdminExecutionSummaryListResponse, AdminInstanceDetailEnvelope,
    AdminInstanceMutationResponse, AdminTaskInstanceListResponse,
};
use presenter::node_backends_snapshot_json;
pub use router::create_admin_router;
use types::{ExecutionHistoryQuery, ListQuery, PageTokenQuery, StreamQuery};

use crate::proto::spearlet::{
    execution_service_client::ExecutionServiceClient,
    instance_service_client::InstanceServiceClient,
    invocation_service_client::InvocationServiceClient, DestroyInstanceRequest, ExecutionMode,
    ReconcileTaskAssignmentsNowRequest, TerminateExecutionRequest,
};

mod ai_backend_admin;
mod credential_mcp_admin;
mod invocation_flow;
mod node_rpc;
mod presenter;
mod router;
mod task_admin;
mod types;

pub(crate) use ai_backend_admin::{
    create_ai_backend_admin, delete_ai_backend_admin, delete_ai_backend_placement_admin,
    get_ai_backend_admin, list_ai_backend_assignments_admin, list_ai_backend_node_statuses_admin,
    list_ai_backend_placements_admin, list_ai_backends_admin, list_ai_model_views_admin,
    preflight_ai_backend_admin, set_ai_backend_desired_state_admin, update_ai_backend_admin,
    upsert_ai_backend_placement_admin, AiBackendPlacementWriteBody, AiBackendPlacementsQuery,
    AiBackendPreflightBody, AiBackendWriteBody, SetAiBackendDesiredStateBody,
};
pub(crate) use credential_mcp_admin::{
    delete_credential_admin, delete_mcp_server, get_mcp_server, list_credentials_admin,
    list_mcp_servers, upsert_credential_admin, upsert_mcp_server, McpServerUpsertBody,
    UpsertCredentialBody,
};
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
) -> Json<AdminExecutionMutationResponse> {
    use crate::proto::sms::GetExecutionRequest;
    let execution_id = p.0;
    if execution_id.trim().is_empty() {
        return Json(AdminExecutionMutationResponse {
            success: false,
            node_uuid: None,
            execution_id: None,
            final_status: None,
            message: "execution_id is required".to_string(),
        });
    }

    let mut exe_client = state.execution_index_client.clone();
    let exe = match exe_client
        .get_execution(tonic::Request::new(GetExecutionRequest {
            execution_id: execution_id.clone(),
        }))
        .await
    {
        Ok(r) => r.into_inner(),
        Err(e) => {
            return Json(AdminExecutionMutationResponse {
                success: false,
                node_uuid: None,
                execution_id: Some(execution_id.clone()),
                final_status: None,
                message: e.to_string(),
            })
        }
    };
    if !exe.found {
        return Json(AdminExecutionMutationResponse {
            success: false,
            node_uuid: None,
            execution_id: Some(execution_id.clone()),
            final_status: None,
            message: "execution not found".to_string(),
        });
    }
    let Some(exe) = exe.execution else {
        return Json(AdminExecutionMutationResponse {
            success: false,
            node_uuid: None,
            execution_id: Some(execution_id.clone()),
            final_status: None,
            message: "execution not found".to_string(),
        });
    };
    if exe.node_uuid.trim().is_empty() {
        return Json(AdminExecutionMutationResponse {
            success: false,
            node_uuid: None,
            execution_id: Some(execution_id.clone()),
            final_status: None,
            message: "node_uuid is missing for execution".to_string(),
        });
    }

    let target = match node_rpc::resolve_node_channel(&state, &exe.node_uuid).await {
        Ok(t) => t,
        Err(resp) => {
            let message = resp
                .0
                .get("message")
                .and_then(|value| value.as_str())
                .unwrap_or("failed to resolve node")
                .to_string();
            return Json(AdminExecutionMutationResponse {
                success: false,
                node_uuid: Some(exe.node_uuid),
                execution_id: Some(execution_id.clone()),
                final_status: None,
                message,
            });
        }
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
        Err(e) => {
            return Json(AdminExecutionMutationResponse {
                success: false,
                node_uuid: Some(target.node_uuid),
                execution_id: Some(execution_id),
                final_status: None,
                message: e.to_string(),
            })
        }
    };

    Json(AdminExecutionMutationResponse {
        success: resp.success,
        node_uuid: Some(target.node_uuid),
        execution_id: Some(execution_id),
        final_status: Some(
            crate::sms::execution_status_to_public_str(resp.final_status).to_string(),
        ),
        message: resp.message,
    })
}

pub(super) async fn destroy_instance_admin(
    state: GatewayState,
    p: Path<String>,
    axum::extract::Json(body): axum::extract::Json<DestroyInstanceBody>,
) -> Json<AdminInstanceMutationResponse> {
    let instance_id = p.0;
    if instance_id.trim().is_empty() {
        return Json(AdminInstanceMutationResponse {
            success: false,
            node_uuid: None,
            instance_id: None,
            message: "instance_id is required".to_string(),
        });
    }
    if body.node_uuid.trim().is_empty() {
        return Json(AdminInstanceMutationResponse {
            success: false,
            node_uuid: None,
            instance_id: Some(instance_id),
            message: "node_uuid is required".to_string(),
        });
    }

    let target = match node_rpc::resolve_node_channel(&state, &body.node_uuid).await {
        Ok(t) => t,
        Err(resp) => {
            let message = resp
                .0
                .get("message")
                .and_then(|value| value.as_str())
                .unwrap_or("failed to resolve node")
                .to_string();
            return Json(AdminInstanceMutationResponse {
                success: false,
                node_uuid: Some(body.node_uuid),
                instance_id: Some(instance_id),
                message,
            });
        }
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
            return Json(AdminInstanceMutationResponse {
                success: true,
                node_uuid: Some(target.node_uuid),
                instance_id: Some(instance_id),
                message: "instance already absent".to_string(),
            });
        }
        Err(e) => {
            return Json(AdminInstanceMutationResponse {
                success: false,
                node_uuid: Some(target.node_uuid),
                instance_id: Some(instance_id),
                message: e.to_string(),
            })
        }
    };

    Json(AdminInstanceMutationResponse {
        success: resp.success,
        node_uuid: Some(target.node_uuid),
        instance_id: Some(instance_id),
        message: resp.message,
    })
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

async fn list_nodes(
    state: GatewayState,
    Query(q): Query<ListQuery>,
) -> Json<AdminNodeListResponse> {
    let mut client = state.node_client.clone();
    let req = ListNodesRequest {
        status_filter: q.status.unwrap_or_default(),
    };
    let resp = client.list_nodes(req).await.unwrap().into_inner();
    let total = resp.nodes.len();
    let mut list = resp
        .nodes
        .into_iter()
        .map(node_to_admin_list_item)
        .collect::<Vec<_>>();
    if let Some(q) = q.q.as_ref().map(|s| s.to_lowercase()) {
        list.retain(|item| {
            let meta_hit = item
                .metadata
                .iter()
                .any(|(k, v)| k.to_lowercase().contains(&q) || v.to_lowercase().contains(&q));
            item.uuid.to_lowercase().contains(&q)
                || item.ip_address.to_lowercase().contains(&q)
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
            "last_heartbeat" => list.sort_by_key(|item| item.last_heartbeat),
            "registered_at" => list.sort_by_key(|item| item.registered_at),
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
    Json(AdminNodeListResponse {
        nodes: list,
        total_count: total,
    })
}

async fn list_task_instances_admin(
    state: GatewayState,
    Path(task_id): Path<String>,
    Query(q): Query<PageTokenQuery>,
) -> Json<AdminTaskInstanceListResponse> {
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
                instances.push(task_instance_row(i, detail.as_ref()));
            }
            Json(AdminTaskInstanceListResponse {
                success: true,
                instances,
                next_page_token: inner.next_page_token,
                message: None,
            })
        }
        Err(e) => Json(AdminTaskInstanceListResponse {
            success: false,
            instances: Vec::new(),
            next_page_token: String::new(),
            message: Some(e.to_string()),
        }),
    }
}

async fn get_instance_admin(
    state: GatewayState,
    Path(instance_id): Path<String>,
) -> Json<AdminInstanceDetailEnvelope> {
    use crate::proto::sms::GetInstanceRequest;
    let mut client = state.execution_index_client.clone();
    let resp = client
        .get_instance(tonic::Request::new(GetInstanceRequest { instance_id }))
        .await;
    match resp {
        Ok(r) => {
            let inner = r.into_inner();
            Json(AdminInstanceDetailEnvelope {
                success: true,
                found: inner.found,
                active: inner.active,
                instance: inner.instance.map(instance_to_detail),
                message: None,
            })
        }
        Err(e) => Json(AdminInstanceDetailEnvelope {
            success: false,
            found: false,
            active: false,
            instance: None,
            message: Some(e.to_string()),
        }),
    }
}

async fn list_instance_executions_admin(
    state: GatewayState,
    Path(instance_id): Path<String>,
    Query(q): Query<PageTokenQuery>,
) -> Json<AdminExecutionSummaryListResponse> {
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
                .map(execution_summary_to_row)
                .collect::<Vec<_>>();
            Json(AdminExecutionSummaryListResponse {
                success: true,
                executions,
                next_page_token: inner.next_page_token,
                message: None,
            })
        }
        Err(e) => Json(AdminExecutionSummaryListResponse {
            success: false,
            executions: Vec::new(),
            next_page_token: String::new(),
            message: Some(e.to_string()),
        }),
    }
}

async fn list_execution_history_admin(
    state: GatewayState,
    Query(q): Query<ExecutionHistoryQuery>,
) -> Json<AdminExecutionHistoryListResponse> {
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
                .map(execution_to_history_row)
                .collect::<Vec<_>>();
            Json(AdminExecutionHistoryListResponse {
                success: true,
                executions,
                next_page_token: inner.next_page_token,
                message: None,
            })
        }
        Err(e) => Json(AdminExecutionHistoryListResponse {
            success: false,
            executions: Vec::new(),
            next_page_token: String::new(),
            message: Some(e.to_string()),
        }),
    }
}

async fn get_execution_admin(
    state: GatewayState,
    Path(execution_id): Path<String>,
) -> Json<AdminExecutionDetailEnvelope> {
    use crate::proto::sms::GetExecutionRequest;
    let mut client = state.execution_index_client.clone();
    let resp = client
        .get_execution(tonic::Request::new(GetExecutionRequest { execution_id }))
        .await;
    match resp {
        Ok(r) => {
            let inner = r.into_inner();
            if !inner.found {
                return Json(AdminExecutionDetailEnvelope {
                    success: true,
                    found: false,
                    execution: None,
                    message: None,
                });
            }
            Json(AdminExecutionDetailEnvelope {
                success: true,
                found: true,
                execution: inner.execution.map(execution_to_detail),
                message: None,
            })
        }
        Err(e) => Json(AdminExecutionDetailEnvelope {
            success: false,
            found: false,
            execution: None,
            message: Some(e.to_string()),
        }),
    }
}

async fn get_node_detail(
    state: GatewayState,
    Path(uuid): Path<String>,
) -> Json<crate::sms::node_api::AdminNodeDetailEnvelope> {
    use crate::proto::sms::GetNodeWithResourceRequest;
    let mut client = state.node_client.clone();
    let resp = client
        .get_node_with_resource(GetNodeWithResourceRequest { uuid })
        .await
        .unwrap()
        .into_inner();
    Json(admin_node_detail_response(resp.node, resp.resource))
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
