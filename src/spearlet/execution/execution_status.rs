use crate::proto::{
    sms::ExecutionStatus as SmsExecutionStatus,
    spearlet::ExecutionStatus as SpearletExecutionStatus,
};

/// Public execution status used by spearlet-facing APIs and in-memory records.
/// spearlet 对外 API 与内存记录使用的执行公开状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExecutionPublicStatus {
    Unknown,
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
    Timeout,
}

impl ExecutionPublicStatus {
    /// Parse a public status string into the normalized execution status model.
    /// 将公开状态字符串解析为规范化的执行状态模型。
    pub(crate) fn from_public_str(status: &str) -> Self {
        match status.trim().to_ascii_lowercase().as_str() {
            "pending" => Self::Pending,
            "running" => Self::Running,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            // Keep accepting "terminated" as a compatibility alias for cancelled.
            // 保留接受 "terminated" 作为 cancelled 的兼容别名。
            "cancelled" | "terminated" => Self::Cancelled,
            "timeout" => Self::Timeout,
            _ => Self::Unknown,
        }
    }

    /// Convert an SMS execution status into the normalized public status model.
    /// 将 SMS 执行状态转换为规范化公开状态模型。
    pub(crate) fn from_sms_status(status: SmsExecutionStatus) -> Self {
        match status {
            SmsExecutionStatus::Pending => Self::Pending,
            SmsExecutionStatus::Running => Self::Running,
            SmsExecutionStatus::Completed => Self::Completed,
            SmsExecutionStatus::Failed => Self::Failed,
            SmsExecutionStatus::Cancelled => Self::Cancelled,
            SmsExecutionStatus::Timeout => Self::Timeout,
            SmsExecutionStatus::Unknown => Self::Unknown,
        }
    }

    /// Render the normalized status as the public string stored in responses.
    /// 将规范化状态渲染为响应中存储的公开字符串。
    pub(crate) fn as_public_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Timeout => "timeout",
        }
    }

    /// Check whether the status is terminal from the public API perspective.
    /// 检查该状态在公开 API 视角下是否为终态。
    pub(crate) fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled | Self::Timeout
        )
    }

    /// Check whether the status represents a successful completion.
    /// 检查该状态是否表示成功完成。
    pub(crate) fn is_successful(self) -> bool {
        self == Self::Completed
    }

    /// Convert the normalized public status into the spearlet execution proto enum.
    /// 将规范化公开状态转换为 spearlet 执行 proto 枚举。
    pub(crate) fn to_spearlet_proto(self) -> i32 {
        match self {
            Self::Unknown => SpearletExecutionStatus::Unspecified as i32,
            Self::Pending => SpearletExecutionStatus::Pending as i32,
            Self::Running => SpearletExecutionStatus::Running as i32,
            Self::Completed => SpearletExecutionStatus::Completed as i32,
            Self::Failed => SpearletExecutionStatus::Failed as i32,
            Self::Cancelled => SpearletExecutionStatus::Terminated as i32,
            Self::Timeout => SpearletExecutionStatus::Timeout as i32,
        }
    }

    /// Convert a spearlet execution proto enum value into the normalized public status model.
    /// 将 spearlet 执行 proto 枚举值转换为规范化公开状态模型。
    pub(crate) fn from_spearlet_proto(status: i32) -> Self {
        match SpearletExecutionStatus::try_from(status).unwrap_or(SpearletExecutionStatus::Unspecified)
        {
            SpearletExecutionStatus::Pending => Self::Pending,
            SpearletExecutionStatus::Running => Self::Running,
            SpearletExecutionStatus::Completed => Self::Completed,
            SpearletExecutionStatus::Failed => Self::Failed,
            SpearletExecutionStatus::Terminated => Self::Cancelled,
            SpearletExecutionStatus::Timeout => Self::Timeout,
            SpearletExecutionStatus::Unspecified => Self::Unknown,
        }
    }

    /// Render the status for spearlet HTTP responses.
    /// 将状态渲染为 spearlet HTTP 响应使用的字符串。
    pub(crate) fn as_http_str(self) -> &'static str {
        match self {
            Self::Unknown => "UNSPECIFIED",
            Self::Pending => "PENDING",
            Self::Running => "RUNNING",
            Self::Completed => "COMPLETED",
            Self::Failed => "FAILED",
            // Preserve the existing HTTP API wording for compatibility.
            // 为保持兼容，沿用现有 HTTP API 的 TERMINATED 文案。
            Self::Cancelled => "TERMINATED",
            Self::Timeout => "TIMEOUT",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminated_alias_maps_to_cancelled() {
        assert_eq!(
            ExecutionPublicStatus::from_public_str("terminated"),
            ExecutionPublicStatus::Cancelled
        );
    }

    #[test]
    fn cancelled_maps_to_terminated_proto() {
        assert_eq!(
            ExecutionPublicStatus::Cancelled.to_spearlet_proto(),
            SpearletExecutionStatus::Terminated as i32
        );
    }

    #[test]
    fn timeout_is_terminal_but_not_successful() {
        assert!(ExecutionPublicStatus::Timeout.is_terminal());
        assert!(!ExecutionPublicStatus::Timeout.is_successful());
    }

    #[test]
    fn terminated_proto_maps_to_cancelled_public_status() {
        assert_eq!(
            ExecutionPublicStatus::from_spearlet_proto(SpearletExecutionStatus::Terminated as i32),
            ExecutionPublicStatus::Cancelled
        );
    }

    #[test]
    fn cancelled_renders_as_terminated_for_http_compatibility() {
        assert_eq!(ExecutionPublicStatus::Cancelled.as_http_str(), "TERMINATED");
    }
}
