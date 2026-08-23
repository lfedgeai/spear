use std::{collections::HashMap, future::Future, sync::Arc, time::Duration};

use serde::Serialize;
use tokio::time::timeout;
use tonic::transport::Channel;

use crate::proto::sms::{
    execution_log_ingest_service_client::ExecutionLogIngestServiceClient,
    execution_registry_service_client::ExecutionRegistryServiceClient,
    instance_registry_service_client::InstanceRegistryServiceClient,
    task_service_client::TaskServiceClient, CompleteTaskDeletionRequest, DeleteInstanceRequest,
    Execution, ExecutionLogLine, FinalizeExecutionLogsRequest, Instance, TaskStatus,
    UpdateTaskResultRequest, UpdateTaskStatusRequest,
};
use crate::spearlet::config::SpearletConfig;

use super::{ExecutionError, ExecutionResult};

/// Log line payload for SMS append API.
/// SMS 日志追加接口使用的日志行载荷。
#[derive(Debug, Clone, Serialize)]
pub(super) struct SmsAppendLogLine {
    pub(super) ts_ms: Option<u64>,
    pub(super) stream: Option<String>,
    pub(super) level: Option<String>,
    pub(super) message: String,
}

/// Bridge local runtime state to SMS control plane.
/// 将本地运行态桥接上报到 SMS 控制面。
#[derive(Debug, Clone)]
pub(super) struct SmsReporter {
    config: Arc<SpearletConfig>,
    channel: Option<Channel>,
}

impl SmsReporter {
    /// Create a reporter from spearlet config and channel.
    /// 根据 spearlet 配置与 channel 创建上报器。
    pub(super) fn new(config: Arc<SpearletConfig>, channel: Option<Channel>) -> Self {
        Self { config, channel }
    }

    fn channel(&self) -> Option<Channel> {
        self.channel.clone()
    }

    fn timeout_window(&self) -> Duration {
        Duration::from_millis(self.config.sms_connect_timeout_ms)
            .min(Duration::from_secs(5))
            .max(Duration::from_millis(1))
    }

    fn node_uuid(&self) -> String {
        self.config.compute_node_uuid()
    }

    /// Spawn a background SMS RPC if the control-plane channel is available.
    /// 如果控制面 channel 可用，则异步启动一个后台 SMS RPC。
    fn spawn_background_call<F, Fut>(&self, operation: F)
    where
        F: FnOnce(Channel, Duration) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let Some(channel) = self.channel() else {
            return;
        };
        let per_attempt = self.timeout_window();
        tokio::spawn(operation(channel, per_attempt));
    }

    /// Build an instance report payload for SMS.
    /// 为 SMS 构造实例上报载荷。
    fn build_instance_report(
        &self,
        task_id: String,
        instance_id: String,
        current_execution_id: String,
        ts_ms: i64,
        status: i32,
    ) -> Instance {
        Instance {
            instance_id,
            task_id,
            node_uuid: self.node_uuid(),
            status,
            created_at_ms: ts_ms,
            updated_at_ms: ts_ms,
            last_seen_ms: ts_ms,
            current_execution_id,
            metadata: HashMap::new(),
        }
    }

    /// Build an execution report payload for SMS.
    /// 为 SMS 构造执行上报载荷。
    fn build_execution_report(
        &self,
        invocation_id: String,
        task_id: String,
        function_name: String,
        instance_id: String,
        execution_id: String,
        status: i32,
        started_at_ms: i64,
        completed_at_ms: i64,
        metadata: HashMap<String, String>,
    ) -> Execution {
        let updated_at_ms = if completed_at_ms > 0 {
            completed_at_ms
        } else {
            started_at_ms
        };
        Execution {
            execution_id,
            invocation_id,
            task_id,
            function_name,
            node_uuid: self.node_uuid(),
            instance_id,
            status,
            started_at_ms,
            completed_at_ms,
            log_ref: None,
            metadata,
            updated_at_ms,
        }
    }

    /// Build a task status update request for SMS.
    /// 为 SMS 构造 task 状态更新请求。
    fn build_update_task_status_request(
        &self,
        task_id: &str,
        status: TaskStatus,
        reason: Option<String>,
    ) -> UpdateTaskStatusRequest {
        UpdateTaskStatusRequest {
            task_id: task_id.to_string(),
            status: status as i32,
            node_uuid: self.node_uuid(),
            status_version: 0,
            updated_at: chrono::Utc::now().timestamp(),
            reason: reason.unwrap_or_default(),
        }
    }

    /// Build a task result update request for SMS.
    /// 为 SMS 构造 task 结果更新请求。
    fn build_update_task_result_request(
        &self,
        task_id: &str,
        result_uri: String,
        result_status: String,
        completed_at: i64,
        result_metadata: HashMap<String, String>,
    ) -> UpdateTaskResultRequest {
        UpdateTaskResultRequest {
            task_id: task_id.to_string(),
            result_uri,
            result_status,
            completed_at,
            result_metadata,
        }
    }

    /// Build a deletion-complete acknowledgement request for SMS.
    /// 为 SMS 构造删除完成确认请求。
    fn build_complete_task_deletion_request(&self, task_id: String) -> CompleteTaskDeletionRequest {
        CompleteTaskDeletionRequest { task_id }
    }

    /// Report instance state to SMS asynchronously.
    /// 异步向 SMS 上报实例状态。
    pub(super) fn report_instance(
        &self,
        task_id: String,
        instance_id: String,
        current_execution_id: String,
        ts_ms: i64,
        status: i32,
    ) {
        let instance =
            self.build_instance_report(task_id, instance_id, current_execution_id, ts_ms, status);
        self.spawn_background_call(move |channel, per_attempt| async move {
            let mut client = InstanceRegistryServiceClient::new(channel);
            let _ = timeout(per_attempt, client.report_instance(instance)).await;
        });
    }

    /// Delete an instance record from SMS synchronously.
    /// 同步从 SMS 删除实例记录。
    pub(super) async fn delete_instance_via_sms(
        &self,
        task_id: &str,
        instance_id: &str,
    ) -> ExecutionResult<()> {
        let Some(channel) = self.channel() else {
            return Ok(());
        };
        let req = DeleteInstanceRequest {
            instance_id: instance_id.to_string(),
            task_id: task_id.to_string(),
            deleted_at_ms: chrono::Utc::now().timestamp_millis(),
        };
        let per_attempt = self.timeout_window();
        let mut client = InstanceRegistryServiceClient::new(channel);
        timeout(per_attempt, client.delete_instance(req))
            .await
            .map_err(|_| ExecutionError::RuntimeError {
                message: format!("DeleteInstance timed out for instance {}", instance_id),
            })?
            .map_err(|e| ExecutionError::RuntimeError {
                message: format!(
                    "DeleteInstance RPC failed for instance {}: {}",
                    instance_id, e
                ),
            })?;
        Ok(())
    }

    /// Report execution state to SMS asynchronously.
    /// 异步向 SMS 上报执行状态。
    pub(super) fn report_execution(
        &self,
        invocation_id: String,
        task_id: String,
        function_name: String,
        instance_id: String,
        execution_id: String,
        status: i32,
        started_at_ms: i64,
        completed_at_ms: i64,
        metadata: HashMap<String, String>,
    ) {
        let execution = self.build_execution_report(
            invocation_id,
            task_id,
            function_name,
            instance_id,
            execution_id,
            status,
            started_at_ms,
            completed_at_ms,
            metadata,
        );
        self.spawn_background_call(move |channel, per_attempt| async move {
            let mut client = ExecutionRegistryServiceClient::new(channel);
            let _ = timeout(per_attempt, client.report_execution(execution)).await;
        });
    }

    /// Publish task lifecycle status to SMS.
    /// 向 SMS 发布任务生命周期状态。
    pub(super) fn update_task_status(
        &self,
        task_id: &str,
        status: TaskStatus,
        reason: Option<String>,
    ) {
        let request = self.build_update_task_status_request(task_id, status, reason);
        self.spawn_background_call(move |channel, per_attempt| async move {
            let mut client = TaskServiceClient::new(channel);
            let _ = timeout(per_attempt, client.update_task_status(request)).await;
        });
    }

    /// Publish task result summary to SMS.
    /// 向 SMS 发布任务结果摘要。
    pub(super) fn update_task_result(
        &self,
        task_id: &str,
        result_uri: String,
        result_status: String,
        completed_at: i64,
        result_metadata: HashMap<String, String>,
    ) {
        let request = self.build_update_task_result_request(
            task_id,
            result_uri,
            result_status,
            completed_at,
            result_metadata,
        );
        self.spawn_background_call(move |channel, per_attempt| async move {
            let mut client = TaskServiceClient::new(channel);
            let _ = timeout(per_attempt, client.update_task_result(request)).await;
        });
    }

    /// Notify SMS that runtime cleanup for a task has completed.
    /// 通知 SMS 某个任务的运行态清理已经完成。
    pub(super) fn acknowledge_task_deletion(&self, task_id: String) {
        let request = self.build_complete_task_deletion_request(task_id);
        self.spawn_background_call(move |channel, per_attempt| async move {
            let mut client = TaskServiceClient::new(channel);
            let _ = timeout(per_attempt, client.complete_task_deletion(request)).await;
        });
    }

    /// Append execution logs to SMS in sequence order.
    /// 按顺序向 SMS 追加执行日志。
    pub(super) async fn append_execution_logs(
        &self,
        execution_id: &str,
        next_seq: &mut u64,
        lines: Vec<SmsAppendLogLine>,
    ) -> ExecutionResult<()> {
        if execution_id.trim().is_empty() || lines.is_empty() {
            return Ok(());
        }
        let Some(channel) = self.channel() else {
            return Ok(());
        };
        let mut client = ExecutionLogIngestServiceClient::new(channel);

        let mut output_lines = Vec::with_capacity(lines.len());
        for line in lines {
            let seq = (*next_seq).max(1);
            *next_seq = (*next_seq).saturating_add(1);
            output_lines.push(ExecutionLogLine {
                ts_ms: line
                    .ts_ms
                    .unwrap_or_else(|| chrono::Utc::now().timestamp_millis() as u64),
                seq,
                stream: line.stream.unwrap_or_else(|| "stdout".to_string()),
                level: line.level.unwrap_or_else(|| "info".to_string()),
                message: line.message,
            });
        }

        let request = tonic::Request::new(crate::proto::sms::AppendExecutionLogsRequest {
            execution_id: execution_id.to_string(),
            lines: output_lines,
        });
        let response = timeout(self.timeout_window(), client.append_execution_logs(request))
            .await
            .map_err(|_| ExecutionError::RuntimeError {
                message: "sms append_execution_logs timeout".to_string(),
            })?
            .map_err(|error| ExecutionError::RuntimeError {
                message: error.to_string(),
            })?
            .into_inner();
        if response.next_seq > 0 {
            *next_seq = (*next_seq).max(response.next_seq);
        }
        Ok(())
    }

    /// Finalize execution logs in SMS.
    /// 在 SMS 中完成执行日志封口。
    pub(super) async fn finalize_execution_logs(&self, execution_id: &str) -> ExecutionResult<()> {
        if execution_id.trim().is_empty() {
            return Ok(());
        }
        let Some(channel) = self.channel() else {
            return Ok(());
        };
        let mut client = ExecutionLogIngestServiceClient::new(channel);
        let request = tonic::Request::new(FinalizeExecutionLogsRequest {
            execution_id: execution_id.to_string(),
        });
        let _ = timeout(
            self.timeout_window(),
            client.finalize_execution_logs(request),
        )
        .await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn reporter() -> SmsReporter {
        SmsReporter::new(Arc::new(SpearletConfig::default()), None)
    }

    #[test]
    fn build_execution_report_prefers_completion_time_for_updated_at() {
        let report = reporter().build_execution_report(
            "inv-1".to_string(),
            "task-1".to_string(),
            "main".to_string(),
            "inst-1".to_string(),
            "exec-1".to_string(),
            3,
            100,
            250,
            HashMap::new(),
        );

        assert_eq!(report.updated_at_ms, 250);
        assert_eq!(report.completed_at_ms, 250);
        assert_eq!(report.started_at_ms, 100);
    }

    #[test]
    fn build_task_status_request_includes_reason_and_node_uuid() {
        let request = reporter().build_update_task_status_request(
            "task-1",
            TaskStatus::Active,
            Some("instance initialized".to_string()),
        );

        assert_eq!(request.task_id, "task-1");
        assert_eq!(request.status, TaskStatus::Active as i32);
        assert_eq!(request.reason, "instance initialized");
        assert!(!request.node_uuid.is_empty());
    }

    #[test]
    fn build_complete_task_deletion_request_uses_task_only() {
        let request = reporter().build_complete_task_deletion_request("task-1".to_string());

        assert_eq!(request.task_id, "task-1");
    }
}
