//! Task service implementation / 任务服务实现

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::proto::sms::{Task, TaskStatus};
use crate::sms::error::{SmsError, SmsResult};
use crate::sms::task_semantics::is_routable_task;

const ENDPOINT_MAX_LEN: usize = 64;

/// Task service for managing distributed tasks / 管理分布式任务的服务
#[derive(Debug, Clone)]
pub struct TaskService {
    /// In-memory storage for tasks / 任务的内存存储
    tasks: Arc<RwLock<HashMap<String, Task>>>,
    /// endpoint -> task_id index / endpoint -> task_id 索引
    endpoint_index: Arc<RwLock<HashMap<String, String>>>,
}

impl TaskService {
    /// Create a new task service / 创建新的任务服务
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            endpoint_index: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a task / 注册任务
    pub async fn register_task(&mut self, mut task: Task) -> SmsResult<()> {
        let normalized_endpoint = normalize_gateway_endpoint(&task.endpoint)?;
        task.endpoint = normalized_endpoint.clone();

        let old_task = {
            let tasks = self.tasks.read().await;
            tasks.get(&task.task_id).cloned()
        };

        if !normalized_endpoint.is_empty() {
            let mut idx = self.endpoint_index.write().await;
            match idx.get(&normalized_endpoint) {
                Some(existing_task_id) if existing_task_id != &task.task_id => {
                    return Err(SmsError::Conflict(format!(
                        "endpoint already exists: {}",
                        normalized_endpoint
                    )))
                }
                Some(_) => {}
                None => {
                    idx.insert(normalized_endpoint.clone(), task.task_id.clone());
                }
            }

            if let Some(old) = old_task.as_ref() {
                let old_norm = normalize_gateway_endpoint(&old.endpoint)?;
                if !old_norm.is_empty()
                    && old_norm != normalized_endpoint
                    && idx
                        .get(&old_norm)
                        .map(|v| v == &task.task_id)
                        .unwrap_or(false)
                {
                    idx.remove(&old_norm);
                }
            }
        }
        let mut tasks = self.tasks.write().await;
        tasks.insert(task.task_id.clone(), task);
        Ok(())
    }

    /// Get a task by ID / 根据ID获取任务
    pub async fn get_task(&self, task_id: &str) -> SmsResult<Option<Task>> {
        let tasks = self.tasks.read().await;
        Ok(tasks.get(task_id).cloned())
    }

    /// Resolve a routable task by endpoint.
    /// 根据 endpoint 解析当前可路由的 task。
    pub async fn resolve_routable_task_by_endpoint(
        &self,
        endpoint: &str,
    ) -> SmsResult<Option<Task>> {
        let normalized = normalize_gateway_endpoint(endpoint)?;
        let idx = self.endpoint_index.read().await;
        if let Some(task_id) = idx.get(&normalized).cloned() {
            drop(idx);
            return Ok(self.get_task(&task_id).await?.filter(is_routable_task));
        }
        Ok(None)
    }

    /// List all tasks / 列出所有任务
    pub async fn list_tasks(&self) -> SmsResult<Vec<Task>> {
        let tasks = self.tasks.read().await;
        Ok(tasks.values().cloned().collect())
    }

    /// Update task heartbeat / 更新任务心跳
    pub async fn update_task_heartbeat(&mut self, task_id: &str, timestamp: i64) -> SmsResult<()> {
        let mut tasks = self.tasks.write().await;
        if let Some(task) = tasks.get_mut(task_id) {
            task.last_heartbeat = timestamp;
        }
        Ok(())
    }

    /// Remove a task / 移除任务
    pub async fn remove_task(&mut self, task_id: &str) -> SmsResult<bool> {
        let mut tasks = self.tasks.write().await;
        let removed = tasks.remove(task_id);
        drop(tasks);
        if let Some(task) = removed.as_ref() {
            let normalized = normalize_gateway_endpoint(&task.endpoint)?;
            if !normalized.is_empty() {
                let mut idx = self.endpoint_index.write().await;
                idx.remove(&normalized);
            }
        }
        Ok(removed.is_some())
    }

    /// Mark a task as deleting while preserving endpoint reservation / 将任务标记为删除中，同时保留 endpoint 占用
    pub async fn mark_task_deleting(
        &mut self,
        task_id: &str,
        reason: &str,
        requested_at: i64,
    ) -> SmsResult<Option<Task>> {
        let mut tasks = self.tasks.write().await;
        let Some(task) = tasks.get_mut(task_id) else {
            return Ok(None);
        };

        task.status = TaskStatus::Deleting as i32;
        task.deletion_requested_at = requested_at;
        task.deletion_reason = reason.to_string();
        task.desired_replicas = 0;

        Ok(Some(task.clone()))
    }

    /// Complete a pending task deletion / 完成处于删除中的任务删除
    pub async fn complete_task_deletion(&mut self, task_id: &str) -> SmsResult<bool> {
        let task = match self.get_task(task_id).await? {
            Some(task) => task,
            None => return Ok(false),
        };
        if task.status != TaskStatus::Deleting as i32 {
            return Err(SmsError::InvalidRequest(format!(
                "task {} is not deleting",
                task_id
            )));
        }
        self.remove_task(task_id).await
    }

    /// List tasks with filters / 使用过滤器列出任务
    pub async fn list_tasks_with_filters(
        &self,
        status_filter: Option<i32>,
        priority_filter: Option<i32>,
        limit: Option<i32>,
        offset: Option<i32>,
    ) -> SmsResult<Vec<Task>> {
        let tasks = self.tasks.read().await;
        let mut filtered_tasks: Vec<Task> = tasks
            .values()
            .filter(|task| {
                // Filter by status if specified / 如果指定则按状态过滤
                if let Some(status) = status_filter {
                    if status >= 0 && task.status != status {
                        return false;
                    }
                }

                // Filter by priority if specified / 如果指定则按优先级过滤
                if let Some(priority) = priority_filter {
                    if priority >= 0 && task.priority != priority {
                        return false;
                    }
                }

                true
            })
            .cloned()
            .collect();

        // Apply offset and limit / 应用偏移量和限制
        let offset = offset.unwrap_or(0) as usize;
        let limit = limit.unwrap_or(100) as usize;

        if offset < filtered_tasks.len() {
            filtered_tasks = filtered_tasks
                .into_iter()
                .skip(offset)
                .take(limit)
                .collect();
        } else {
            filtered_tasks.clear();
        }

        Ok(filtered_tasks)
    }
}

impl Default for TaskService {
    fn default() -> Self {
        Self::new()
    }
}

fn normalize_gateway_endpoint(v: &str) -> SmsResult<String> {
    let trimmed = v.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    if trimmed.len() > ENDPOINT_MAX_LEN {
        return Err(SmsError::InvalidRequest(format!(
            "endpoint too long: {} (max {})",
            trimmed.len(),
            ENDPOINT_MAX_LEN
        )));
    }
    if !trimmed
        .as_bytes()
        .iter()
        .all(|c| c.is_ascii_alphanumeric() || *c == b'_' || *c == b'-')
    {
        return Err(SmsError::InvalidRequest(
            "endpoint must match ^[A-Za-z0-9_-]+$".to_string(),
        ));
    }
    Ok(trimmed.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_task(task_id: &str, endpoint: &str) -> Task {
        let mut t = Task::default();
        t.task_id = task_id.to_string();
        t.endpoint = endpoint.to_string();
        t
    }

    #[tokio::test]
    async fn endpoint_is_normalized_and_queryable() {
        let mut svc = TaskService::new();
        svc.register_task(make_task("t1", "Echo_01")).await.unwrap();

        let t = svc
            .resolve_routable_task_by_endpoint("echo_01")
            .await
            .unwrap();
        assert!(t.is_some());
        assert_eq!(t.unwrap().task_id, "t1");

        let t2 = svc
            .resolve_routable_task_by_endpoint("ECHO_01")
            .await
            .unwrap();
        assert!(t2.is_some());
        assert_eq!(t2.unwrap().endpoint, "echo_01");
    }

    #[tokio::test]
    async fn endpoint_conflict_is_rejected() {
        let mut svc = TaskService::new();
        svc.register_task(make_task("t1", "echo")).await.unwrap();
        let err = svc
            .register_task(make_task("t2", "ECHO"))
            .await
            .err()
            .unwrap();
        match err {
            SmsError::Conflict(_) => {}
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn remove_task_removes_endpoint_index() {
        let mut svc = TaskService::new();
        svc.register_task(make_task("t1", "echo")).await.unwrap();
        assert!(svc
            .resolve_routable_task_by_endpoint("echo")
            .await
            .unwrap()
            .is_some());

        let removed = svc.remove_task("t1").await.unwrap();
        assert!(removed);
        assert!(svc
            .resolve_routable_task_by_endpoint("echo")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn deleting_task_is_hidden_from_endpoint_resolution() {
        let mut svc = TaskService::new();
        svc.register_task(make_task("t1", "echo")).await.unwrap();
        let ts = chrono::Utc::now().timestamp();
        let deleting = svc
            .mark_task_deleting("t1", "delete requested", ts)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(deleting.status, TaskStatus::Deleting as i32);
        assert_eq!(deleting.deletion_reason, "delete requested");
        assert_eq!(deleting.deletion_requested_at, ts);
        assert!(svc
            .resolve_routable_task_by_endpoint("echo")
            .await
            .unwrap()
            .is_none());
        assert!(svc.get_task("t1").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn complete_task_deletion_requires_deleting_state() {
        let mut svc = TaskService::new();
        let task = make_task("t1", "echo");
        svc.register_task(task).await.unwrap();

        let err = svc.complete_task_deletion("t1").await.unwrap_err();
        match err {
            SmsError::InvalidRequest(_) => {}
            other => panic!("unexpected error: {other:?}"),
        }

        svc.mark_task_deleting("t1", "delete requested", 1)
            .await
            .unwrap();
        let removed = svc.complete_task_deletion("t1").await.unwrap();
        assert!(removed);
        assert!(svc.get_task("t1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn invalid_endpoint_is_rejected() {
        let mut svc = TaskService::new();
        let err = svc
            .register_task(make_task("t1", "a/b"))
            .await
            .err()
            .unwrap();
        match err {
            SmsError::InvalidRequest(_) => {}
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn task_endpoint_name_is_accepted() {
        let mut svc = TaskService::new();
        svc.register_task(make_task("t1", "test")).await.unwrap();
    }

    #[tokio::test]
    async fn endpoint_update_for_same_task_updates_index() {
        let mut svc = TaskService::new();
        svc.register_task(make_task("t1", "echo")).await.unwrap();
        svc.register_task(make_task("t1", "echo2")).await.unwrap();

        assert!(svc
            .resolve_routable_task_by_endpoint("echo")
            .await
            .unwrap()
            .is_none());
        assert!(svc
            .resolve_routable_task_by_endpoint("echo2")
            .await
            .unwrap()
            .is_some());
    }
}
