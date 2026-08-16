use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use base64::{engine::general_purpose, Engine as _};
use serde::Deserialize;
use tracing::{debug, error};

use crate::proto::spearlet::{
    GetExecutionRequest, InvokeRequest, TerminateExecutionRequest,
};
use crate::spearlet::execution::execution_status::ExecutionPublicStatus;

use super::AppState;

#[derive(Deserialize)]
pub(super) struct ExecuteFunctionBody {
    task_id: Option<String>,
    function_name: Option<String>,
    invocation_id: Option<String>,
    execution_id: Option<String>,
    session_id: Option<String>,
    mode: Option<String>,
    timeout_ms: Option<u64>,
    force_new_instance: Option<bool>,
    headers: Option<std::collections::HashMap<String, String>>,
    environment: Option<std::collections::HashMap<String, String>>,
    metadata: Option<std::collections::HashMap<String, String>>,
    input_base64: Option<String>,
    input_content_type: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct ExecutionStatusQuery {
    include_output: Option<bool>,
}

#[derive(Deserialize)]
pub(super) struct CancelExecutionBody {
    reason: Option<String>,
}

/// Execute function endpoint / 执行函数端点
/// POST /functions/execute
pub(super) async fn execute_function(
    State(state): State<AppState>,
    Json(body): Json<ExecuteFunctionBody>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    debug!("POST /functions/execute");

    let task_id = body.task_id.unwrap_or_default();
    if task_id.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mode = body.mode.unwrap_or_else(|| "sync".to_string());
    let mode = mode.to_ascii_lowercase();
    let proto_mode = match mode.as_str() {
        "sync" => crate::proto::spearlet::ExecutionMode::Sync as i32,
        "async" => crate::proto::spearlet::ExecutionMode::Async as i32,
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    let mut input_data = Vec::new();
    if let Some(b64) = body.input_base64.as_ref() {
        input_data = general_purpose::STANDARD
            .decode(b64)
            .map_err(|_| StatusCode::BAD_REQUEST)?;
    }

    let req = InvokeRequest {
        invocation_id: body.invocation_id.unwrap_or_default(),
        execution_id: body.execution_id.unwrap_or_default(),
        task_id: task_id.clone(),
        function_name: body.function_name.unwrap_or_default(),
        input: Some(crate::proto::spearlet::Payload {
            content_type: body
                .input_content_type
                .unwrap_or_else(|| "application/octet-stream".to_string()),
            data: input_data,
        }),
        headers: body.headers.unwrap_or_default(),
        environment: body.environment.unwrap_or_default(),
        timeout_ms: body.timeout_ms.unwrap_or(0),
        session_id: body.session_id.unwrap_or_default(),
        mode: proto_mode,
        force_new_instance: body.force_new_instance.unwrap_or(false),
        metadata: body.metadata.unwrap_or_default(),
    };

    let mut client = state.invocation_client.clone();
    match client.invoke(req).await {
        Ok(response) => {
            let resp = response.into_inner();
            let output_b64 = resp
                .output
                .as_ref()
                .map(|p| general_purpose::STANDARD.encode(&p.data))
                .unwrap_or_default();
            Ok(Json(serde_json::json!({
                "success": true,
                "invocation_id": resp.invocation_id,
                "execution_id": resp.execution_id,
                "instance_id": resp.instance_id,
                "status": ExecutionPublicStatus::from_spearlet_proto(resp.status).as_http_str(),
                "output_base64": output_b64,
                "error": resp.error.map(|e| serde_json::json!({"code": e.code, "message": e.message}))
            })))
        }
        Err(e) => {
            error!("Failed to execute function for task {}: {}", task_id, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get execution status endpoint / 获取执行状态端点
/// GET /functions/executions/:execution_id
pub(super) async fn get_execution_status(
    State(state): State<AppState>,
    Path(execution_id): Path<String>,
    Query(params): Query<ExecutionStatusQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    debug!("GET /functions/executions/{}", execution_id);

    let include_output = params.include_output.unwrap_or(false);
    let req = GetExecutionRequest {
        execution_id: execution_id.clone(),
        include_output,
    };

    let mut client = state.execution_client.clone();
    match client.get_execution(req).await {
        Ok(response) => {
            let exec = response.into_inner();
            let output_b64 = exec
                .output
                .as_ref()
                .map(|p| general_purpose::STANDARD.encode(&p.data))
                .unwrap_or_default();
            Ok(Json(serde_json::json!({
                "execution_id": exec.execution_id,
                "invocation_id": exec.invocation_id,
                "task_id": exec.task_id,
                "function_name": exec.function_name,
                "instance_id": exec.instance_id,
                "status": ExecutionPublicStatus::from_spearlet_proto(exec.status).as_http_str(),
                "output_base64": output_b64,
                "error": exec.error.map(|e| serde_json::json!({"code": e.code, "message": e.message})),
                "started_at": exec.started_at.map(|t| chrono::DateTime::<chrono::Utc>::from_timestamp(t.seconds, t.nanos as u32).map(|dt| dt.to_rfc3339()).unwrap_or_default()),
                "completed_at": exec.completed_at.map(|t| chrono::DateTime::<chrono::Utc>::from_timestamp(t.seconds, t.nanos as u32).map(|dt| dt.to_rfc3339()).unwrap_or_default())
            })))
        }
        Err(e) => {
            if e.code() == tonic::Code::NotFound {
                return Err(StatusCode::NOT_FOUND);
            }
            error!("Failed to get execution {}: {}", execution_id, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Cancel execution endpoint / 取消执行端点
/// POST /functions/executions/:execution_id/cancel
pub(super) async fn cancel_execution(
    State(state): State<AppState>,
    Path(execution_id): Path<String>,
    body: Option<Json<CancelExecutionBody>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    debug!("POST /functions/executions/{}/cancel", execution_id);

    let reason = body.and_then(|b| b.0.reason).unwrap_or_default();

    let req = TerminateExecutionRequest {
        execution_id: execution_id.clone(),
        reason,
    };

    let mut client = state.execution_client.clone();
    match client.terminate_execution(req).await {
        Ok(response) => {
            let resp = response.into_inner();
            Ok(Json(serde_json::json!({
                "success": resp.success,
                "execution_id": execution_id,
                "final_status": ExecutionPublicStatus::from_spearlet_proto(resp.final_status).as_http_str(),
                "message": resp.message
            })))
        }
        Err(e) => {
            error!("Failed to terminate execution {}: {}", execution_id, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
