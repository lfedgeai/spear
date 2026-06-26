use serde_json::json;

use crate::proto::spearlet::{ExecutionStatus, InvokeRequest, Payload};
use crate::spearlet::execution::DEFAULT_ENTRY_FUNCTION_NAME;

/// Build the common ExistingTask invocation request used by Web Admin flows.
/// 构建 Web Admin 流程共用的 ExistingTask invocation 请求。
pub(super) fn build_invoke_request(
    task_id: &str,
    request_id: &str,
    execution_id: &str,
    mode: i32,
) -> InvokeRequest {
    InvokeRequest {
        invocation_id: request_id.to_string(),
        execution_id: execution_id.to_string(),
        task_id: task_id.to_string(),
        function_name: DEFAULT_ENTRY_FUNCTION_NAME.to_string(),
        input: Some(Payload {
            content_type: "application/octet-stream".to_string(),
            data: Vec::new(),
        }),
        headers: Default::default(),
        environment: Default::default(),
        timeout_ms: 0,
        session_id: String::new(),
        mode,
        force_new_instance: false,
        metadata: Default::default(),
    }
}

/// Normalize invocation completion status into `(success, message)`.
/// 将 invocation 完成结果归一化为 `(success, message)`。
pub(super) fn summarize_invoke_result(status: i32, error_message: Option<&str>) -> (bool, String) {
    let success = status == ExecutionStatus::Completed as i32;
    let message = error_message.unwrap_or("ok").to_string();
    (success, message)
}

/// Build the direct invocation success JSON payload shared by Web Admin handlers.
/// 构建 Web Admin 直连调用路径共用的成功 JSON 响应。
pub(super) fn direct_invoke_success_json(
    node_uuid: &str,
    invocation_id: &str,
    execution_id: &str,
    message: &str,
    success: bool,
) -> serde_json::Value {
    json!({
        "success": success,
        "node_uuid": node_uuid,
        "invocation_id": invocation_id,
        "execution_id": execution_id,
        "message": message,
    })
}
