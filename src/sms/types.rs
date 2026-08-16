//! SMS module types and constants
//! SMS模块类型和常量
//!
//! This module contains common types and constants used throughout the SMS module.
//! 此模块包含SMS模块中使用的通用类型和常量。

use crate::proto::sms::{ExecutionStatus, InstanceStatus, TaskPriority, TaskStatus};

/// Generic constant representing "no filter" for any filtering operation
/// 表示任何过滤操作中"无过滤器"的通用常量
pub const NO_FILTER: i32 = -1;

pub fn task_status_to_public_str(status: i32) -> &'static str {
    match TaskStatus::try_from(status).ok() {
        Some(TaskStatus::Registered) => "registered",
        Some(TaskStatus::Active) => "active",
        Some(TaskStatus::Inactive) => "inactive",
        Some(TaskStatus::Deleting) => "deleting",
        _ => "unknown",
    }
}

pub fn parse_task_status_public_str(status: &str) -> Option<i32> {
    match status.trim().to_ascii_lowercase().as_str() {
        "unknown" => Some(TaskStatus::Unknown as i32),
        "registered" => Some(TaskStatus::Registered as i32),
        "active" => Some(TaskStatus::Active as i32),
        "inactive" => Some(TaskStatus::Inactive as i32),
        "deleting" => Some(TaskStatus::Deleting as i32),
        _ => None,
    }
}

pub fn task_priority_to_public_str(priority: i32) -> &'static str {
    match TaskPriority::try_from(priority).ok() {
        Some(TaskPriority::Low) => "low",
        Some(TaskPriority::Normal) => "normal",
        Some(TaskPriority::High) => "high",
        Some(TaskPriority::Urgent) => "urgent",
        _ => "unknown",
    }
}

pub fn instance_status_to_public_str(status: i32) -> &'static str {
    match InstanceStatus::try_from(status).unwrap_or(InstanceStatus::Unknown) {
        InstanceStatus::Running => "running",
        InstanceStatus::Idle => "idle",
        InstanceStatus::Terminating => "terminating",
        InstanceStatus::Terminated => "terminated",
        InstanceStatus::Unknown => "unknown",
    }
}

pub fn execution_status_to_public_str(status: i32) -> &'static str {
    match ExecutionStatus::try_from(status).unwrap_or(ExecutionStatus::Unknown) {
        ExecutionStatus::Pending => "pending",
        ExecutionStatus::Running => "running",
        ExecutionStatus::Completed => "completed",
        ExecutionStatus::Failed => "failed",
        ExecutionStatus::Cancelled => "cancelled",
        ExecutionStatus::Timeout => "timeout",
        ExecutionStatus::Unknown => "unknown",
    }
}

pub fn is_instance_routable(status: i32) -> bool {
    matches!(
        InstanceStatus::try_from(status).unwrap_or(InstanceStatus::Unknown),
        InstanceStatus::Running | InstanceStatus::Idle
    )
}

pub fn is_instance_live(status: i32) -> bool {
    !matches!(
        InstanceStatus::try_from(status).unwrap_or(InstanceStatus::Unknown),
        InstanceStatus::Terminated | InstanceStatus::Unknown
    )
}

pub fn is_execution_terminal(status: i32) -> bool {
    matches!(
        ExecutionStatus::try_from(status).unwrap_or(ExecutionStatus::Unknown),
        ExecutionStatus::Completed
            | ExecutionStatus::Failed
            | ExecutionStatus::Cancelled
            | ExecutionStatus::Timeout
    )
}

pub fn parse_task_priority_public_str(priority: &str) -> i32 {
    match priority.trim().to_ascii_lowercase().as_str() {
        "low" => TaskPriority::Low as i32,
        "normal" => TaskPriority::Normal as i32,
        "high" => TaskPriority::High as i32,
        "urgent" => TaskPriority::Urgent as i32,
        _ => TaskPriority::Normal as i32,
    }
}

/// Enum representing different filter states for better type safety
/// 表示不同过滤器状态的枚举，提供更好的类型安全性
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterState {
    /// No filter applied / 不应用过滤器
    None,
    /// Filter with specific value / 使用特定值过滤
    Value(i32),
}

impl FilterState {
    /// Convert FilterState to i32 for protobuf compatibility
    /// 将FilterState转换为i32以兼容protobuf
    pub fn to_i32(self) -> i32 {
        match self {
            FilterState::None => NO_FILTER,
            FilterState::Value(v) => v,
        }
    }

    /// Create FilterState from i32 value
    /// 从i32值创建FilterState
    pub fn from_i32(value: i32) -> Self {
        if value == NO_FILTER {
            FilterState::None
        } else {
            FilterState::Value(value)
        }
    }

    /// Check if filter is active (has a value)
    /// 检查过滤器是否激活（有值）
    pub fn is_active(self) -> bool {
        matches!(self, FilterState::Value(_))
    }

    /// Check if filter is disabled (no filter)
    /// 检查过滤器是否禁用（无过滤器）
    pub fn is_none(self) -> bool {
        matches!(self, FilterState::None)
    }

    /// Get the filter value if it exists
    /// 获取过滤器值（如果存在）
    pub fn value(self) -> Option<i32> {
        match self {
            FilterState::None => None,
            FilterState::Value(v) => Some(v),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_state_to_i32() {
        assert_eq!(FilterState::None.to_i32(), NO_FILTER);
        assert_eq!(FilterState::Value(42).to_i32(), 42);
    }

    #[test]
    fn test_filter_state_from_i32() {
        assert_eq!(FilterState::from_i32(NO_FILTER), FilterState::None);
        assert_eq!(FilterState::from_i32(42), FilterState::Value(42));
    }

    #[test]
    fn test_filter_state_is_active() {
        assert!(!FilterState::None.is_active());
        assert!(FilterState::Value(42).is_active());
    }

    #[test]
    fn test_filter_state_is_none() {
        assert!(FilterState::None.is_none());
        assert!(!FilterState::Value(42).is_none());
    }

    #[test]
    fn test_filter_state_value() {
        assert_eq!(FilterState::None.value(), None);
        assert_eq!(FilterState::Value(42).value(), Some(42));
    }

    #[test]
    fn test_task_status_to_public_str_maps_known_statuses() {
        assert_eq!(
            task_status_to_public_str(TaskStatus::Registered as i32),
            "registered"
        );
        assert_eq!(
            task_status_to_public_str(TaskStatus::Active as i32),
            "active"
        );
        assert_eq!(
            task_status_to_public_str(TaskStatus::Inactive as i32),
            "inactive"
        );
        assert_eq!(
            task_status_to_public_str(TaskStatus::Deleting as i32),
            "deleting"
        );
    }

    #[test]
    fn test_parse_task_status_public_str_registered() {
        assert_eq!(
            parse_task_status_public_str("registered"),
            Some(TaskStatus::Registered as i32)
        );
        assert_eq!(
            parse_task_status_public_str("deleting"),
            Some(TaskStatus::Deleting as i32)
        );
    }

    #[test]
    fn test_instance_status_to_public_str_maps_known_statuses() {
        assert_eq!(
            instance_status_to_public_str(InstanceStatus::Running as i32),
            "running"
        );
        assert_eq!(
            instance_status_to_public_str(InstanceStatus::Idle as i32),
            "idle"
        );
        assert_eq!(
            instance_status_to_public_str(InstanceStatus::Terminating as i32),
            "terminating"
        );
        assert_eq!(
            instance_status_to_public_str(InstanceStatus::Terminated as i32),
            "terminated"
        );
    }

    #[test]
    fn test_execution_status_to_public_str_maps_known_statuses() {
        assert_eq!(
            execution_status_to_public_str(ExecutionStatus::Pending as i32),
            "pending"
        );
        assert_eq!(
            execution_status_to_public_str(ExecutionStatus::Completed as i32),
            "completed"
        );
        assert_eq!(
            execution_status_to_public_str(ExecutionStatus::Cancelled as i32),
            "cancelled"
        );
    }

    #[test]
    fn test_instance_predicates() {
        assert!(is_instance_routable(InstanceStatus::Running as i32));
        assert!(is_instance_routable(InstanceStatus::Idle as i32));
        assert!(!is_instance_routable(InstanceStatus::Terminating as i32));
        assert!(is_instance_live(InstanceStatus::Running as i32));
        assert!(!is_instance_live(InstanceStatus::Terminated as i32));
    }

    #[test]
    fn test_execution_terminal_predicate() {
        assert!(!is_execution_terminal(ExecutionStatus::Pending as i32));
        assert!(!is_execution_terminal(ExecutionStatus::Running as i32));
        assert!(is_execution_terminal(ExecutionStatus::Completed as i32));
        assert!(is_execution_terminal(ExecutionStatus::Failed as i32));
        assert!(is_execution_terminal(ExecutionStatus::Cancelled as i32));
        assert!(is_execution_terminal(ExecutionStatus::Timeout as i32));
    }
}
