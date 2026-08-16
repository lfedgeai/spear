use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use tonic::Request;

use super::common::ErrorResponse;
use crate::proto::sms::{
    GetExecutionRequest, GetInstanceRequest, ListInstanceExecutionsRequest,
    ListTaskInstancesRequest,
};
use crate::sms::gateway::GatewayState;

#[derive(Debug, Deserialize)]
pub struct PageParams {
    pub limit: Option<i32>,
    pub page_token: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InstanceSummaryResponse {
    pub instance_id: String,
    pub node_uuid: String,
    pub status: String,
    pub last_seen_ms: i64,
    pub current_execution_id: String,
}

#[derive(Debug, Serialize)]
pub struct ExecutionSummaryResponse {
    pub execution_id: String,
    pub task_id: String,
    pub status: String,
    pub started_at_ms: i64,
    pub completed_at_ms: i64,
    pub function_name: String,
}

#[derive(Debug, Serialize)]
pub struct ListTaskInstancesHttpResponse {
    pub instances: Vec<InstanceSummaryResponse>,
    pub next_page_token: String,
}

#[derive(Debug, Serialize)]
pub struct ListInstanceExecutionsHttpResponse {
    pub executions: Vec<ExecutionSummaryResponse>,
    pub next_page_token: String,
}

#[derive(Debug, Serialize)]
pub struct GetExecutionHttpResponse {
    pub found: bool,
    pub execution: Option<ExecutionResponse>,
}

#[derive(Debug, Serialize)]
pub struct GetInstanceHttpResponse {
    pub found: bool,
    pub active: bool,
    pub instance: Option<InstanceResponse>,
}

#[derive(Debug, Serialize)]
pub struct InstanceResponse {
    pub instance_id: String,
    pub task_id: String,
    pub node_uuid: String,
    pub status: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub last_seen_ms: i64,
    pub current_execution_id: String,
    pub metadata: std::collections::HashMap<String, String>,
}

#[derive(Debug, Serialize)]
pub struct LogRefResponse {
    pub backend: String,
    pub uri_prefix: String,
    pub content_type: String,
    pub compression: String,
}

#[derive(Debug, Serialize)]
pub struct ExecutionResponse {
    pub execution_id: String,
    pub invocation_id: String,
    pub task_id: String,
    pub function_name: String,
    pub node_uuid: String,
    pub instance_id: String,
    pub status: String,
    pub started_at_ms: i64,
    pub completed_at_ms: i64,
    pub log_ref: Option<LogRefResponse>,
    pub metadata: std::collections::HashMap<String, String>,
    pub updated_at_ms: i64,
}

pub async fn list_task_instances(
    State(state): State<GatewayState>,
    Path(task_id): Path<String>,
    Query(params): Query<PageParams>,
) -> Result<Json<ListTaskInstancesHttpResponse>, (StatusCode, Json<ErrorResponse>)> {
    let limit = params.limit.unwrap_or(50).max(1);
    let page_token = params.page_token.unwrap_or_default();
    let req = Request::new(ListTaskInstancesRequest {
        task_id,
        limit,
        page_token,
    });

    let resp = state
        .execution_index_client
        .clone()
        .list_task_instances(req)
        .await;
    match resp {
        Ok(r) => {
            let inner = r.into_inner();
            Ok(Json(ListTaskInstancesHttpResponse {
                instances: inner
                    .instances
                    .into_iter()
                    .map(|s| InstanceSummaryResponse {
                        instance_id: s.instance_id,
                        node_uuid: s.node_uuid,
                        status: crate::sms::instance_status_to_public_str(s.status).to_string(),
                        last_seen_ms: s.last_seen_ms,
                        current_execution_id: s.current_execution_id,
                    })
                    .collect(),
                next_page_token: inner.next_page_token,
            }))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "LIST_TASK_INSTANCES_FAILED".to_string(),
                message: e.to_string(),
            }),
        )),
    }
}

pub async fn list_instance_executions(
    State(state): State<GatewayState>,
    Path(instance_id): Path<String>,
    Query(params): Query<PageParams>,
) -> Result<Json<ListInstanceExecutionsHttpResponse>, (StatusCode, Json<ErrorResponse>)> {
    let limit = params.limit.unwrap_or(50).max(1);
    let page_token = params.page_token.unwrap_or_default();
    let req = Request::new(ListInstanceExecutionsRequest {
        instance_id,
        limit,
        page_token,
    });

    let resp = state
        .execution_index_client
        .clone()
        .list_instance_executions(req)
        .await;
    match resp {
        Ok(r) => {
            let inner = r.into_inner();
            Ok(Json(ListInstanceExecutionsHttpResponse {
                executions: inner
                    .executions
                    .into_iter()
                    .map(|s| ExecutionSummaryResponse {
                        execution_id: s.execution_id,
                        task_id: s.task_id,
                        status: crate::sms::execution_status_to_public_str(s.status).to_string(),
                        started_at_ms: s.started_at_ms,
                        completed_at_ms: s.completed_at_ms,
                        function_name: s.function_name,
                    })
                    .collect(),
                next_page_token: inner.next_page_token,
            }))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "LIST_INSTANCE_EXECUTIONS_FAILED".to_string(),
                message: e.to_string(),
            }),
        )),
    }
}

pub async fn get_instance(
    State(state): State<GatewayState>,
    Path(instance_id): Path<String>,
) -> Result<Json<GetInstanceHttpResponse>, (StatusCode, Json<ErrorResponse>)> {
    let req = Request::new(GetInstanceRequest { instance_id });
    let resp = state.execution_index_client.clone().get_instance(req).await;
    match resp {
        Ok(r) => {
            let inner = r.into_inner();
            let instance = inner.instance.map(|i| InstanceResponse {
                instance_id: i.instance_id,
                task_id: i.task_id,
                node_uuid: i.node_uuid,
                status: crate::sms::instance_status_to_public_str(i.status).to_string(),
                created_at_ms: i.created_at_ms,
                updated_at_ms: i.updated_at_ms,
                last_seen_ms: i.last_seen_ms,
                current_execution_id: i.current_execution_id,
                metadata: i.metadata,
            });
            Ok(Json(GetInstanceHttpResponse {
                found: inner.found,
                active: inner.active,
                instance,
            }))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "GET_INSTANCE_FAILED".to_string(),
                message: e.to_string(),
            }),
        )),
    }
}

pub async fn get_execution(
    State(state): State<GatewayState>,
    Path(execution_id): Path<String>,
) -> Result<Json<GetExecutionHttpResponse>, (StatusCode, Json<ErrorResponse>)> {
    let req = Request::new(GetExecutionRequest { execution_id });
    let resp = state
        .execution_index_client
        .clone()
        .get_execution(req)
        .await;
    match resp {
        Ok(r) => {
            let inner = r.into_inner();
            let execution = inner.execution.map(|e| ExecutionResponse {
                execution_id: e.execution_id,
                invocation_id: e.invocation_id,
                task_id: e.task_id,
                function_name: e.function_name,
                node_uuid: e.node_uuid,
                instance_id: e.instance_id,
                status: crate::sms::execution_status_to_public_str(e.status).to_string(),
                started_at_ms: e.started_at_ms,
                completed_at_ms: e.completed_at_ms,
                log_ref: e.log_ref.map(|lr| LogRefResponse {
                    backend: lr.backend,
                    uri_prefix: lr.uri_prefix,
                    content_type: lr.content_type,
                    compression: lr.compression,
                }),
                metadata: e.metadata,
                updated_at_ms: e.updated_at_ms,
            });
            Ok(Json(GetExecutionHttpResponse {
                found: inner.found,
                execution,
            }))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "GET_EXECUTION_FAILED".to_string(),
                message: e.to_string(),
            }),
        )),
    }
}
