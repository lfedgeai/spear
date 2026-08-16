use crate::proto::sms::{Task, TaskStatus};

/// Whether a task should participate in endpoint routing and dispatch.
/// 一个 task 是否应参与 endpoint 路由与分发。
pub(crate) fn is_routable_task(task: &Task) -> bool {
    task.status != TaskStatus::Deleting as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_task(status: i32) -> Task {
        Task {
            task_id: "task-1".to_string(),
            name: "demo".to_string(),
            description: String::new(),
            endpoint: "/demo".to_string(),
            registered_at: 0,
            last_heartbeat: 0,
            status,
            priority: 0,
            version: String::new(),
            capabilities: Vec::new(),
            metadata: Default::default(),
            config: Default::default(),
            executable: None,
            deletion_requested_at: 0,
            deletion_reason: String::new(),
            result_uris: Vec::new(),
            last_result_uri: String::new(),
            last_result_status: String::new(),
            last_completed_at: 0,
            last_result_metadata: Default::default(),
            desired_replicas: 1,
            scheduling_strategy: crate::proto::sms::TaskSchedulingStrategy::Spread as i32,
        }
    }

    #[test]
    fn deleting_task_is_not_routable() {
        assert!(!is_routable_task(&sample_task(TaskStatus::Deleting as i32)));
    }

    #[test]
    fn active_task_is_routable() {
        assert!(is_routable_task(&sample_task(TaskStatus::Active as i32)));
    }
}
