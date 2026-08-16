use std::collections::HashMap;

use crate::spearlet::execution::runtime::ExecutionStatus as RuntimeExecutionStatus;

use super::{
    execution_status::ExecutionPublicStatus,
    sms_reporter::SmsAppendLogLine,
    sms_status_adapter::{result_flags_to_sms_execution_status, runtime_execution_status_to_sms},
};

/// Final execution state shared by sync and async completion flows.
/// 同步与异步完成流程共享的最终执行状态。
#[derive(Debug, Clone, Copy)]
pub(super) struct FinalExecutionState {
    /// SMS execution status code / SMS 执行状态码
    pub(super) sms_status: i32,
    /// Public status string / 对外状态字符串
    pub(super) public_status: &'static str,
    /// System log level for completion line / 完成日志行的系统级别
    pub(super) log_level: &'static str,
}

impl FinalExecutionState {
    /// Build a final state from runtime success/failure flags.
    /// 根据运行时成功/失败标记构建最终状态。
    pub(super) fn from_result_flags(is_successful: bool, has_failed: bool) -> Self {
        Self::from_sms_status(result_flags_to_sms_execution_status(is_successful, has_failed))
    }

    /// Build a final state directly from runtime execution status.
    /// 直接根据运行时执行状态构建最终状态。
    pub(super) fn from_runtime_status(status: RuntimeExecutionStatus) -> Self {
        Self::from_sms_status(runtime_execution_status_to_sms(status))
    }

    fn from_sms_status(status: crate::proto::sms::ExecutionStatus) -> Self {
        let public_status = ExecutionPublicStatus::from_sms_status(status);
        Self {
            sms_status: status as i32,
            public_status: public_status.as_public_str(),
            log_level: match status {
                crate::proto::sms::ExecutionStatus::Completed
                | crate::proto::sms::ExecutionStatus::Running => "info",
                crate::proto::sms::ExecutionStatus::Failed
                | crate::proto::sms::ExecutionStatus::Cancelled
                | crate::proto::sms::ExecutionStatus::Timeout
                | crate::proto::sms::ExecutionStatus::Pending
                | crate::proto::sms::ExecutionStatus::Unknown => "warn",
            },
        }
    }
}

/// Convert runtime metadata values into a string map.
/// 将运行时元数据值转换为字符串 map。
pub(super) fn stringify_runtime_metadata(
    metadata: HashMap<String, serde_json::Value>,
) -> HashMap<String, String> {
    metadata
        .into_iter()
        .map(|(key, value)| (key, value.to_string()))
        .collect()
}

/// Enrich final metadata with duration and optional error message.
/// 使用耗时和可选错误信息补充最终元数据。
pub(super) fn enrich_final_metadata(
    mut metadata: HashMap<String, String>,
    duration_ms: u64,
    error_message: Option<&str>,
) -> HashMap<String, String> {
    metadata.insert("execution_time_ms".to_string(), duration_ms.to_string());
    if let Some(error_message) = error_message {
        metadata.insert("error_message".to_string(), error_message.to_string());
    }
    metadata
}

/// Build the final system log line emitted before log finalization.
/// 构建日志封口前输出的最终系统日志行。
pub(super) fn build_completion_log_line(
    completed_at_ms: i64,
    final_state: FinalExecutionState,
    duration_ms: u64,
) -> SmsAppendLogLine {
    SmsAppendLogLine {
        ts_ms: Some(completed_at_ms as u64),
        stream: Some("system".to_string()),
        level: Some(final_state.log_level.to_string()),
        message: format!(
            "execution_completed status={} duration_ms={}",
            final_state.public_status, duration_ms
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_maps_to_timeout_final_state() {
        let final_state = FinalExecutionState::from_runtime_status(RuntimeExecutionStatus::Timeout);
        assert_eq!(
            final_state.sms_status,
            crate::proto::sms::ExecutionStatus::Timeout as i32
        );
        assert_eq!(final_state.public_status, "timeout");
        assert_eq!(final_state.log_level, "warn");
    }

    #[test]
    fn enrich_final_metadata_adds_duration_and_error() {
        let mut metadata = HashMap::new();
        metadata.insert("k".to_string(), "v".to_string());

        let enriched = enrich_final_metadata(metadata, 42, Some("boom"));
        assert_eq!(enriched.get("k"), Some(&"v".to_string()));
        assert_eq!(enriched.get("execution_time_ms"), Some(&"42".to_string()));
        assert_eq!(enriched.get("error_message"), Some(&"boom".to_string()));
    }
}
