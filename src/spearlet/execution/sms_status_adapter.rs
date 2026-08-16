use crate::proto::sms::{
    ExecutionStatus as SmsExecutionStatus, InstanceStatus as SmsInstanceStatus,
    TaskStatus as SmsTaskStatus,
};

use super::{
    instance::InstanceStatus as LocalInstanceStatus, runtime::ExecutionStatus as RuntimeExecutionStatus,
    task::TaskStatus as LocalTaskStatus,
};

/// Map runtime execution status into the SMS execution status model.
/// 将运行时执行状态映射到 SMS 执行状态模型。
pub(super) fn runtime_execution_status_to_sms(
    status: RuntimeExecutionStatus,
) -> SmsExecutionStatus {
    match status {
        RuntimeExecutionStatus::Unknown => SmsExecutionStatus::Unknown,
        RuntimeExecutionStatus::Pending => SmsExecutionStatus::Pending,
        RuntimeExecutionStatus::Running => SmsExecutionStatus::Running,
        RuntimeExecutionStatus::Completed => SmsExecutionStatus::Completed,
        RuntimeExecutionStatus::Failed => SmsExecutionStatus::Failed,
        RuntimeExecutionStatus::Cancelled => SmsExecutionStatus::Cancelled,
        RuntimeExecutionStatus::Timeout => SmsExecutionStatus::Timeout,
    }
}

/// Map success/failure flags into the SMS execution status model.
/// 将成功/失败标记映射到 SMS 执行状态模型。
pub(super) fn result_flags_to_sms_execution_status(
    is_successful: bool,
    has_failed: bool,
) -> SmsExecutionStatus {
    if is_successful {
        SmsExecutionStatus::Completed
    } else if has_failed {
        SmsExecutionStatus::Failed
    } else {
        SmsExecutionStatus::Pending
    }
}

/// Map local task lifecycle into the coarser SMS task status model.
/// 将本地 task 生命周期映射到更粗粒度的 SMS task 状态模型。
pub(super) fn local_task_status_to_sms(
    status: &LocalTaskStatus,
    instance_count: usize,
) -> SmsTaskStatus {
    if instance_count > 0 {
        return SmsTaskStatus::Active;
    }
    match status {
        LocalTaskStatus::Initializing => SmsTaskStatus::Registered,
        LocalTaskStatus::Ready
        | LocalTaskStatus::Running
        | LocalTaskStatus::Paused
        | LocalTaskStatus::Scaling
        | LocalTaskStatus::Stopping
        | LocalTaskStatus::Stopped
        | LocalTaskStatus::Error(_) => SmsTaskStatus::Inactive,
    }
}

/// Map local instance lifecycle into the SMS instance status model.
/// 将本地 instance 生命周期映射到 SMS instance 状态模型。
pub(super) fn local_instance_status_to_sms(
    status: &LocalInstanceStatus,
) -> SmsInstanceStatus {
    match status {
        LocalInstanceStatus::Creating | LocalInstanceStatus::Starting => SmsInstanceStatus::Unknown,
        LocalInstanceStatus::Ready => SmsInstanceStatus::Idle,
        LocalInstanceStatus::Running | LocalInstanceStatus::Busy => SmsInstanceStatus::Running,
        LocalInstanceStatus::Unhealthy | LocalInstanceStatus::Stopping => {
            SmsInstanceStatus::Terminating
        }
        LocalInstanceStatus::Stopped | LocalInstanceStatus::Error(_) => {
            SmsInstanceStatus::Terminated
        }
    }
}

/// Map local instance state plus current execution activity into an SMS instance status.
/// 将本地 instance 状态与当前执行活动一起映射成 SMS instance 状态。
pub(super) fn observed_instance_status_to_sms(
    status: &LocalInstanceStatus,
    has_current_execution: bool,
) -> SmsInstanceStatus {
    if has_current_execution {
        SmsInstanceStatus::Running
    } else {
        local_instance_status_to_sms(status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_preserves_timeout_sms_status() {
        assert_eq!(
            runtime_execution_status_to_sms(RuntimeExecutionStatus::Timeout),
            SmsExecutionStatus::Timeout
        );
    }

    #[test]
    fn ready_instance_maps_to_idle_sms_status() {
        assert_eq!(
            local_instance_status_to_sms(&LocalInstanceStatus::Ready),
            SmsInstanceStatus::Idle
        );
    }

    #[test]
    fn task_with_instances_maps_to_active_sms_status() {
        assert_eq!(
            local_task_status_to_sms(&LocalTaskStatus::Initializing, 1),
            SmsTaskStatus::Active
        );
    }
}
