use super::task::TaskStatus;

/// Public task status used by spearlet-facing presentation layers.
/// spearlet 对外展示层使用的 task 公开状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TaskPublicStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

impl TaskPublicStatus {
    /// Convert local task lifecycle state into the normalized public status.
    /// 将本地 task 生命周期状态转换为规范化公开状态。
    pub(crate) fn from_local(status: &TaskStatus) -> Self {
        match status {
            TaskStatus::Initializing | TaskStatus::Ready => Self::Pending,
            TaskStatus::Running
            | TaskStatus::Paused
            | TaskStatus::Scaling
            | TaskStatus::Stopping => Self::Running,
            TaskStatus::Stopped => Self::Completed,
            TaskStatus::Error(_) => Self::Failed,
        }
    }

    /// Render the status for spearlet HTTP responses.
    /// 将状态渲染为 spearlet HTTP 响应使用的字符串。
    pub(crate) fn as_http_str(self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Running => "RUNNING",
            Self::Completed => "COMPLETED",
            Self::Failed => "FAILED",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopping_maps_to_running_http_status() {
        assert_eq!(
            TaskPublicStatus::from_local(&TaskStatus::Stopping).as_http_str(),
            "RUNNING"
        );
    }

    #[test]
    fn stopped_maps_to_completed_http_status() {
        assert_eq!(
            TaskPublicStatus::from_local(&TaskStatus::Stopped).as_http_str(),
            "COMPLETED"
        );
    }

    #[test]
    fn error_maps_to_failed_http_status() {
        assert_eq!(
            TaskPublicStatus::from_local(&TaskStatus::Error("boom".to_string())).as_http_str(),
            "FAILED"
        );
    }
}
