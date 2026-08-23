//! Task HTTP handlers for SPEAR Metadata Server
//! SPEAR元数据服务器的任务HTTP处理器
//!
//! This module provides HTTP handlers that act as a gateway to the gRPC TaskService
//! 此模块提供作为gRPC TaskService网关的HTTP处理器

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tonic::Request;
use tracing::{debug, error, info};

use super::common::ErrorResponse;
use crate::proto::sms::{DeleteTaskRequest, GetTaskRequest, ListTasksRequest, TaskPriority};
use crate::sms::gateway::GatewayState;
use crate::sms::task_api::{
    build_register_task_request, task_to_public_response, PublicTaskResponse, RegisterTaskSpec,
    TaskExecutableSpec,
};
use crate::sms::{parse_task_priority_public_str, parse_task_status_public_str, FilterState};

// HTTP request/response types / HTTP请求/响应类型

#[derive(Debug, Deserialize, Serialize)]
pub struct RegisterTaskParams {
    pub name: String,
    pub description: Option<String>,
    pub priority: Option<String>, // "low", "normal", "high"
    pub desired_replicas: Option<u32>,
    pub scheduling_strategy: Option<String>,
    pub endpoint: String,
    pub version: String,
    pub capabilities: Option<Vec<String>>,
    pub metadata: Option<HashMap<String, String>>,
    pub config: Option<HashMap<String, String>>,
    pub executable: Option<TaskExecutableParams>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TaskExecutableParams {
    pub r#type: String,
    pub uri: String,
    pub name: Option<String>,
    pub checksum_sha256: Option<String>,
    pub args: Option<Vec<String>>,
    pub env: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize)]
pub struct ListTasksParams {
    pub status: Option<String>, // "unknown", "registered", "active", "inactive", "deleting"
    pub priority: Option<String>, // "low", "normal", "high"
    pub limit: Option<i32>,
    pub offset: Option<i32>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DeleteTaskParams {
    pub reason: Option<String>,
    pub force: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct RegisterTaskResponse {
    pub success: bool,
    pub task_id: Option<String>,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct ListTasksResponse {
    pub tasks: Vec<PublicTaskResponse>,
    pub total_count: i32,
}

#[derive(Debug, Serialize)]
pub struct TaskActionResponse {
    pub success: bool,
    pub message: String,
}

// HTTP Handlers / HTTP处理器

/// Register a new task / 注册新任务
pub async fn register_task(
    State(gateway_state): State<GatewayState>,
    Json(params): Json<RegisterTaskParams>,
) -> Result<Json<RegisterTaskResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("HTTP: Registering task: {}", params.name);

    let priority = params
        .priority
        .as_deref()
        .map(parse_task_priority_public_str)
        .unwrap_or(TaskPriority::Normal as i32);

    let request = Request::new(build_register_task_request(RegisterTaskSpec {
        name: params.name.clone(),
        description: params
            .description
            .unwrap_or_else(|| format!("Task: {}", params.name)),
        priority,
        desired_replicas: params.desired_replicas.unwrap_or(1),
        scheduling_strategy: params.scheduling_strategy,
        endpoint: params.endpoint,
        version: params.version,
        capabilities: params.capabilities.unwrap_or_default(),
        metadata: params.metadata.unwrap_or_default(),
        config: params.config.unwrap_or_default(),
        executable: params.executable.map(|executable| TaskExecutableSpec {
            executable_type: executable.r#type,
            uri: executable.uri,
            name: executable.name,
            checksum_sha256: executable.checksum_sha256,
            args: executable.args,
            env: executable.env,
        }),
    }));

    match gateway_state
        .task_client
        .clone()
        .register_task(request)
        .await
    {
        Ok(response) => {
            let resp = response.into_inner();
            Ok(Json(RegisterTaskResponse {
                success: resp.success,
                task_id: Some(resp.task_id),
                message: resp.message,
            }))
        }
        Err(e) => {
            error!("Failed to register task: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "REGISTER_TASK_FAILED".to_string(),
                    message: format!("Failed to register task: {}", e),
                }),
            ))
        }
    }
}

/// List tasks with optional filters / 列出任务（可选过滤器）
pub async fn list_tasks(
    State(gateway_state): State<GatewayState>,
    Query(params): Query<ListTasksParams>,
) -> Result<Json<ListTasksResponse>, (StatusCode, Json<ErrorResponse>)> {
    debug!("HTTP: Listing tasks with filters: {:?}", params);

    // Convert optional filters to FilterState and then to i32 for protobuf compatibility
    // 将可选过滤器转换为FilterState，然后转换为i32以兼容protobuf
    let status_filter = params
        .status
        .as_ref()
        .and_then(|s| parse_task_status_public_str(s))
        .map(FilterState::Value)
        .unwrap_or(FilterState::None)
        .to_i32();

    let priority_filter = params
        .priority
        .as_ref()
        .map(|p| FilterState::Value(parse_task_priority_public_str(p)))
        .unwrap_or(FilterState::None)
        .to_i32();

    let request = Request::new(ListTasksRequest {
        status_filter,
        priority_filter,
        limit: params.limit.unwrap_or(100),
        offset: params.offset.unwrap_or(0),
    });

    match gateway_state.task_client.clone().list_tasks(request).await {
        Ok(response) => {
            let resp = response.into_inner();
            let tasks = resp
                .tasks
                .into_iter()
                .map(task_to_public_response)
                .collect();

            Ok(Json(ListTasksResponse {
                tasks,
                total_count: resp.total_count,
            }))
        }
        Err(e) => {
            error!("Failed to list tasks: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "LIST_TASKS_FAILED".to_string(),
                    message: format!("Failed to list tasks: {}", e),
                }),
            ))
        }
    }
}

/// Get a specific task by ID / 根据ID获取特定任务
pub async fn get_task(
    State(gateway_state): State<GatewayState>,
    Path(task_id): Path<String>,
) -> Result<Json<PublicTaskResponse>, (StatusCode, Json<ErrorResponse>)> {
    debug!("HTTP: Getting task: {}", task_id);

    let request = Request::new(GetTaskRequest { task_id });

    match gateway_state.task_client.clone().get_task(request).await {
        Ok(response) => {
            let resp = response.into_inner();
            if let Some(task) = resp.task {
                Ok(Json(task_to_public_response(task)))
            } else {
                Err((
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse {
                        error: "TASK_NOT_FOUND".to_string(),
                        message: "Task not found".to_string(),
                    }),
                ))
            }
        }
        Err(e) => {
            error!("Failed to get task: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "GET_TASK_FAILED".to_string(),
                    message: format!("Failed to get task: {}", e),
                }),
            ))
        }
    }
}

/// Delete a task / 删除任务
pub async fn delete_task(
    State(gateway_state): State<GatewayState>,
    Path(task_id): Path<String>,
    Json(params): Json<DeleteTaskParams>,
) -> Result<Json<TaskActionResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("HTTP: Deleting task: {}", task_id);

    let request = Request::new(DeleteTaskRequest {
        task_id,
        reason: params.reason.unwrap_or_default(),
        force: params.force.unwrap_or(false),
    });

    match gateway_state.task_client.clone().delete_task(request).await {
        Ok(response) => {
            let resp = response.into_inner();
            Ok(Json(TaskActionResponse {
                success: resp.success,
                message: resp.message,
            }))
        }
        Err(e) => {
            error!("Failed to delete task: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "DELETE_TASK_FAILED".to_string(),
                    message: format!("Failed to delete task: {}", e),
                }),
            ))
        }
    }
}
