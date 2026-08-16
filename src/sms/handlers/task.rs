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
use crate::proto::sms::{
    DeleteTaskRequest, ExecutableType, GetTaskRequest, ListTasksRequest, RegisterTaskRequest,
    TaskExecutable, TaskPriority, TaskSchedulingStrategy,
};
use crate::sms::gateway::GatewayState;
use crate::sms::{
    parse_task_priority_public_str, parse_task_status_public_str, task_priority_to_public_str,
    task_status_to_public_str, FilterState,
};

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
pub struct TaskResponse {
    pub task_id: String,
    pub name: String,
    pub description: String,
    pub status: String,
    pub priority: String,
    pub desired_replicas: u32,
    pub scheduling_strategy: String,
    pub endpoint: String,
    pub version: String,
    pub capabilities: Vec<String>,
    pub registered_at: i64,
    pub last_heartbeat: i64,
    pub metadata: HashMap<String, String>,
    pub config: HashMap<String, String>,
    pub result_uris: Vec<String>,
    pub last_result_uri: String,
    pub last_result_status: String,
    pub last_completed_at: i64,
    pub last_result_metadata: HashMap<String, String>,
}

#[derive(Debug, Serialize)]
pub struct RegisterTaskResponse {
    pub success: bool,
    pub task_id: Option<String>,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct ListTasksResponse {
    pub tasks: Vec<TaskResponse>,
    pub total_count: i32,
}

#[derive(Debug, Serialize)]
pub struct TaskActionResponse {
    pub success: bool,
    pub message: String,
}

// Helper function to convert proto Task to TaskResponse / 转换proto Task为TaskResponse的辅助函数
fn task_to_response(task: crate::proto::sms::Task) -> TaskResponse {
    TaskResponse {
        task_id: task.task_id,
        name: task.name,
        description: task.description,
        status: task_status_to_public_str(task.status).to_string(),
        priority: task_priority_to_public_str(task.priority).to_string(),
        desired_replicas: task.desired_replicas,
        scheduling_strategy: match TaskSchedulingStrategy::try_from(task.scheduling_strategy)
            .unwrap_or(TaskSchedulingStrategy::Spread)
        {
            TaskSchedulingStrategy::Spread => "spread".to_string(),
            TaskSchedulingStrategy::Unknown => "unknown".to_string(),
        },
        endpoint: task.endpoint,
        version: task.version,
        capabilities: task.capabilities,
        registered_at: task.registered_at,
        last_heartbeat: task.last_heartbeat,
        metadata: task.metadata,
        config: task.config,
        result_uris: task.result_uris,
        last_result_uri: task.last_result_uri,
        last_result_status: task.last_result_status,
        last_completed_at: task.last_completed_at,
        last_result_metadata: task.last_result_metadata,
    }
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

    let meta = params.metadata.clone().unwrap_or_default();
    let scheduling_strategy = match params.scheduling_strategy.as_deref() {
        Some("spread") | None => TaskSchedulingStrategy::Spread as i32,
        _ => TaskSchedulingStrategy::Unknown as i32,
    };
    let request = Request::new(RegisterTaskRequest {
        name: params.name.clone(),
        description: params
            .description
            .unwrap_or_else(|| format!("Task: {}", params.name)),
        priority,
        endpoint: params.endpoint,
        version: params.version,
        capabilities: params.capabilities.unwrap_or_default(),
        metadata: meta.clone(),
        config: params.config.unwrap_or_default(),
        executable: params.executable.as_ref().map(|e| TaskExecutable {
            r#type: match e.r#type.to_lowercase().as_str() {
                "binary" => ExecutableType::Binary as i32,
                "script" => ExecutableType::Script as i32,
                "container" => ExecutableType::Container as i32,
                "wasm" => ExecutableType::Wasm as i32,
                "process" => ExecutableType::Process as i32,
                _ => ExecutableType::Unknown as i32,
            },
            uri: e.uri.clone(),
            name: e.name.clone().unwrap_or_default(),
            checksum_sha256: e.checksum_sha256.clone().unwrap_or_default(),
            args: e.args.clone().unwrap_or_default(),
            env: e.env.clone().unwrap_or_default(),
        }),
        desired_replicas: params.desired_replicas.unwrap_or(1),
        scheduling_strategy,
    });

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
            let tasks = resp.tasks.into_iter().map(task_to_response).collect();

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
) -> Result<Json<TaskResponse>, (StatusCode, Json<ErrorResponse>)> {
    debug!("HTTP: Getting task: {}", task_id);

    let request = Request::new(GetTaskRequest { task_id });

    match gateway_state.task_client.clone().get_task(request).await {
        Ok(response) => {
            let resp = response.into_inner();
            if let Some(task) = resp.task {
                Ok(Json(task_to_response(task)))
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
