use crate::proto::sms::{InvocationOutcomeClass, ReportInvocationOutcomeRequest};

/// Parse HTTP string outcome into the SMS enum representation.
/// 将 HTTP 字符串 outcome 解析成 SMS 枚举值。
pub fn parse_http_outcome_class(v: Option<String>) -> i32 {
    match v.as_deref().map(|s| s.to_ascii_lowercase()) {
        Some(s) if s == "success" => InvocationOutcomeClass::Success as i32,
        Some(s) if s == "overloaded" => InvocationOutcomeClass::Overloaded as i32,
        Some(s) if s == "unavailable" => InvocationOutcomeClass::Unavailable as i32,
        Some(s) if s == "timeout" => InvocationOutcomeClass::Timeout as i32,
        Some(s) if s == "rejected" => InvocationOutcomeClass::Rejected as i32,
        Some(s) if s == "bad_request" => InvocationOutcomeClass::BadRequest as i32,
        Some(s) if s == "internal" => InvocationOutcomeClass::Internal as i32,
        _ => InvocationOutcomeClass::Unknown as i32,
    }
}

/// Convert RPC-level errors into placement outcome classes.
/// 将 RPC 级错误转换为 placement outcome class。
pub fn classify_invoke_rpc_error(code: tonic::Code) -> i32 {
    match code {
        tonic::Code::DeadlineExceeded => InvocationOutcomeClass::Timeout as i32,
        tonic::Code::Unavailable => InvocationOutcomeClass::Unavailable as i32,
        tonic::Code::ResourceExhausted => InvocationOutcomeClass::Overloaded as i32,
        tonic::Code::InvalidArgument => InvocationOutcomeClass::BadRequest as i32,
        tonic::Code::Unauthenticated | tonic::Code::PermissionDenied => {
            InvocationOutcomeClass::Rejected as i32
        }
        _ => InvocationOutcomeClass::Internal as i32,
    }
}

/// Whether one outcome class should stop spillback immediately.
/// 某个 outcome class 是否应立即停止 spillback。
pub fn is_terminal_outcome_class(outcome_class: i32) -> bool {
    outcome_class == InvocationOutcomeClass::BadRequest as i32
        || outcome_class == InvocationOutcomeClass::Rejected as i32
}

/// Build one placement feedback request shared by SMS callers.
/// 构建 SMS 调用侧共用的 placement feedback request。
pub fn build_outcome_request(
    decision_id: &str,
    request_id: &str,
    task_id: &str,
    node_uuid: &str,
    outcome_class: i32,
    error_message: String,
) -> ReportInvocationOutcomeRequest {
    ReportInvocationOutcomeRequest {
        decision_id: decision_id.to_string(),
        request_id: request_id.to_string(),
        task_id: task_id.to_string(),
        node_uuid: node_uuid.to_string(),
        outcome_class,
        error_message,
    }
}

/// Normalize proto integer outcome into the typed SMS enum.
/// 将 proto 整数 outcome 归一化为 SMS typed enum。
pub fn normalize_outcome_class(v: i32) -> InvocationOutcomeClass {
    InvocationOutcomeClass::try_from(v).unwrap_or(InvocationOutcomeClass::Unknown)
}
