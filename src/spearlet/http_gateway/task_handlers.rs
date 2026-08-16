use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use tracing::debug;

use crate::spearlet::execution::task_public_status::TaskPublicStatus;

use super::AppState;

/// List tasks endpoint / 列出任务端点
/// GET /tasks
pub(super) async fn list_tasks(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    debug!("GET /tasks");

    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(100)
        .min(1000);
    let offset = params
        .get("offset")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    let filter_status = params.get("status").map(|s| s.to_ascii_uppercase());

    let mgr = state.function_service.get_execution_manager();
    let mut tasks = mgr.list_tasks();
    tasks.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    let mut items: Vec<serde_json::Value> = tasks
        .into_iter()
        .filter_map(|t| {
            let st = TaskPublicStatus::from_local(&t.status()).as_http_str();
            if let Some(fs) = filter_status.as_ref() {
                if fs != st {
                    return None;
                }
            }
            let metrics = t.metrics.read().clone();
            let updated_at = *t.updated_at.read();
            Some(serde_json::json!({
                "task_id": t.id.clone(),
                "function_name": t.spec.name.clone(),
                "status": st,
                "created_at": chrono::DateTime::<chrono::Utc>::from(t.created_at).to_rfc3339(),
                "updated_at": chrono::DateTime::<chrono::Utc>::from(updated_at).to_rfc3339(),
                "execution_count": metrics.total_executions
            }))
        })
        .collect();

    let total = items.len();
    let has_more = offset.saturating_add(limit) < total;
    if offset >= items.len() {
        items.clear();
    } else {
        items = items
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect::<Vec<_>>();
    }

    Ok(Json(serde_json::json!({
        "tasks": items,
        "total": total,
        "limit": limit,
        "offset": offset,
        "has_more": has_more
    })))
}

/// Get task details endpoint / 获取任务详情端点
/// GET /tasks/:task_id
pub(super) async fn get_task(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    debug!("GET /tasks/{}", task_id);

    let mgr = state.function_service.get_execution_manager();
    let Some(task) = mgr.get_task_by_id(&task_id) else {
        return Err(StatusCode::NOT_FOUND);
    };

    let st = TaskPublicStatus::from_local(&task.status()).as_http_str();
    let metrics = task.metrics.read().clone();
    let updated_at = *task.updated_at.read();
    let last_exec = mgr
        .list_executions(Some(&task_id), None, 1)
        .into_iter()
        .next();

    Ok(Json(serde_json::json!({
        "task_id": task.id.clone(),
        "function_name": task.spec.name.clone(),
        "status": st,
        "parameters": task.spec.handler_config.clone(),
        "created_at": chrono::DateTime::<chrono::Utc>::from(task.created_at).to_rfc3339(),
        "updated_at": chrono::DateTime::<chrono::Utc>::from(updated_at).to_rfc3339(),
        "execution_count": metrics.total_executions,
        "last_execution": last_exec.map(|e| serde_json::json!({
            "execution_id": e.execution_id,
            "status": e.status,
            "error": e.error_message
        }))
    })))
}

/// Get task executions endpoint / 获取任务执行记录端点
/// GET /tasks/:task_id/executions
pub(super) async fn get_task_executions(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    debug!("GET /tasks/{}/executions", task_id);

    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(50)
        .min(500);
    let offset = params
        .get("offset")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);

    let mgr = state.function_service.get_execution_manager();

    let items = mgr.list_executions(Some(&task_id), None, limit.saturating_add(offset));
    let total = items.len();
    let has_more = offset.saturating_add(limit) < total;

    let executions = items
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|e| {
            serde_json::json!({
                "execution_id": e.execution_id,
                "invocation_id": e.invocation_id,
                "task_id": e.task_id,
                "function_name": e.function_name,
                "status": e.status,
                "execution_time_ms": e.execution_time_ms,
                "error": e.error_message,
                "timestamp": chrono::DateTime::<chrono::Utc>::from(e.timestamp).to_rfc3339()
            })
        })
        .collect::<Vec<_>>();

    Ok(Json(serde_json::json!({
        "task_id": task_id,
        "executions": executions,
        "total": total,
        "limit": limit,
        "offset": offset,
        "has_more": has_more
    })))
}
