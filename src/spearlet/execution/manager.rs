//! Task Execution Manager
//! 任务执行管理器
//!
//! This module provides the central task execution management system that coordinates
//! artifacts, tasks, instances, and runtime execution.
//! 该模块提供中央任务执行管理系统，协调 artifact、任务、实例和运行时执行。

use super::runtime::RuntimeType;
use super::{
    artifact::{Artifact, ArtifactId},
    execution_finalize::{
        build_completion_log_line, enrich_final_metadata, stringify_runtime_metadata,
        FinalExecutionState,
    },
    execution_status::ExecutionPublicStatus,
    instance::{HealthStatus, InstanceId, InstanceStatus, TaskInstance},
    runtime::{
        ExecutionCompletionEvent, ExecutionContext, ExecutionStatus as RuntimeExecutionStatus,
        RuntimeManager,
    },
    scheduler::{InstanceScheduler, SchedulingPolicy},
    sms_status_adapter::{
        local_instance_status_to_sms, local_task_status_to_sms, observed_instance_status_to_sms,
        runtime_execution_status_to_sms,
    },
    sms_reporter::{SmsAppendLogLine, SmsReporter},
    task_materializer::{
        fetch_sms_task, materialize_local_artifact_from_sms_task,
        materialize_local_task_from_sms_task, materialize_sms_task,
    },
    task_runtime_cleanup::finalize_local_task_removal,
    task::{Task, TaskId},
    ExecutionError, ExecutionResult, DEFAULT_ENTRY_FUNCTION_NAME,
};
use crate::proto::spearlet::{ExecutionMode as ProtoExecutionMode, InvokeRequest};
use crate::spearlet::sms_connector::sms_channel_lazy;
use dashmap::{DashMap, DashSet};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::{mpsc, oneshot, Mutex, Semaphore};
use tokio::time::timeout;
use tonic::transport::Channel;
use tracing::{debug, info, warn};

#[derive(Debug, Clone)]
struct InflightAsyncExecution {
    invocation_id: String,
    task_id: String,
    function_name: String,
    instance_id: String,
    started_at_ms: i64,
    log_next_seq: u64,
    wasm_last_seq: u64,
}

/// Task execution manager configuration / 任务执行管理器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskExecutionManagerConfig {
    /// Maximum concurrent executions / 最大并发执行数
    pub max_concurrent_executions: usize,
    /// Maximum artifacts / 最大 artifact 数
    pub max_artifacts: usize,
    /// Maximum tasks per artifact / 每个 artifact 的最大任务数
    pub max_tasks_per_artifact: usize,
    /// Maximum instances per task / 每个任务的最大实例数
    pub max_instances_per_task: usize,
    /// Instance creation timeout / 实例创建超时
    pub instance_creation_timeout_ms: u64,
    /// Health check interval / 健康检查间隔
    pub health_check_interval_ms: u64,
    /// Metrics collection interval / 指标收集间隔
    pub metrics_collection_interval_ms: u64,
    /// Instance heartbeat interval to SMS / Instance 心跳上报间隔（给 SMS 刷新 last_seen）
    pub instance_heartbeat_interval_ms: u64,
    /// Cleanup interval / 清理间隔
    pub cleanup_interval_ms: u64,
    /// Artifact idle timeout / Artifact 空闲超时
    pub artifact_idle_timeout_ms: u64,
    /// Task idle timeout / 任务空闲超时
    pub task_idle_timeout_ms: u64,
    /// Instance idle timeout / 实例空闲超时
    pub instance_idle_timeout_ms: u64,
}

impl Default for TaskExecutionManagerConfig {
    fn default() -> Self {
        Self {
            max_concurrent_executions: 1000,
            max_artifacts: 100,
            max_tasks_per_artifact: 10,
            max_instances_per_task: 50,
            instance_creation_timeout_ms: 30000,
            health_check_interval_ms: 10000,
            metrics_collection_interval_ms: 5000,
            instance_heartbeat_interval_ms: 30000,
            cleanup_interval_ms: 60000,
            artifact_idle_timeout_ms: 300000, // 5 minutes
            task_idle_timeout_ms: 180000,     // 3 minutes
            instance_idle_timeout_ms: 120000, // 2 minutes
        }
    }
}

#[derive(Debug)]
struct ExecutionWorkItem {
    /// Execution ID / 执行 ID
    pub execution_id: String,
    /// Invocation ID / 调用 ID
    pub invocation_id: String,
    /// Task ID / 任务 ID
    pub task_id: String,
    /// Execution context / 执行上下文
    pub execution_context: ExecutionContext,
    /// Response sender / 响应发送器
    pub response_sender: oneshot::Sender<ExecutionResult<super::ExecutionResponse>>,
}

/// Execution statistics / 执行统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionStatistics {
    /// Total executions / 总执行次数
    pub total_executions: u64,
    /// Successful executions / 成功执行次数
    pub successful_executions: u64,
    /// Failed executions / 失败执行次数
    pub failed_executions: u64,
    /// Average execution time / 平均执行时间
    pub average_execution_time_ms: f64,
    /// Total execution time / 总执行时间
    pub total_execution_time_ms: u64,
    /// Active artifacts / 活跃 artifact 数
    pub active_artifacts: u64,
    /// Active tasks / 活跃任务数
    pub active_tasks: u64,
    /// Active instances / 活跃实例数
    pub active_instances: u64,
    /// Queue size, derived from executions still in public pending state.
    /// 队列大小，来源于仍处于 public pending 状态的执行数量。
    pub queue_size: u64,
    /// Running executions / 正在运行的执行数量
    pub running_executions: u64,
    /// Pending executions, currently the same count as queue_size but exposed as a domain metric.
    /// 等待执行数量；当前与 queue_size 数值相同，但作为领域语义指标对外暴露。
    pub pending_executions: u64,
    /// Completed executions / 已完成的执行数量
    pub completed_executions: u64,
    /// Success rate percentage / 成功率百分比
    pub success_rate_percent: f64,
}

impl Default for ExecutionStatistics {
    fn default() -> Self {
        Self {
            total_executions: 0,
            successful_executions: 0,
            failed_executions: 0,
            average_execution_time_ms: 0.0,
            total_execution_time_ms: 0,
            active_artifacts: 0,
            active_tasks: 0,
            active_instances: 0,
            queue_size: 0,
            running_executions: 0,
            pending_executions: 0,
            completed_executions: 0,
            success_rate_percent: 0.0,
        }
    }
}

/// Task execution manager / 任务执行管理器
pub struct TaskExecutionManager {
    /// Configuration / 配置
    config: TaskExecutionManagerConfig,
    /// Spearlet application configuration / SPEARlet应用配置
    spearlet_config: Arc<crate::spearlet::config::SpearletConfig>,
    /// Runtime manager / 运行时管理器
    runtime_manager: Arc<RuntimeManager>,
    /// Instance scheduler / 实例调度器
    scheduler: Arc<InstanceScheduler>,
    /// Artifacts storage / Artifact 存储
    artifacts: Arc<DashMap<ArtifactId, Arc<Artifact>>>,
    /// Tasks storage / 任务存储
    tasks: Arc<DashMap<TaskId, Arc<Task>>>,
    /// Instances storage / 实例存储
    instances: Arc<DashMap<InstanceId, Arc<TaskInstance>>>,
    /// Desired replica counts last observed from assignment reconciliation
    /// 最近一次从 assignment 收敛得到的目标副本数
    desired_task_instances: Arc<DashMap<TaskId, u32>>,
    /// Serialize replica reconciliation per task to avoid duplicate instance creation
    /// 按 task 串行化副本收敛，避免并发补副本导致重复创建实例
    task_reconcile_locks: Arc<DashMap<TaskId, Arc<Mutex<()>>>>,
    /// Execution response index / 执行响应索引
    execution_responses: Arc<DashMap<String, super::ExecutionResponse>>,
    /// Active execution registry by instance / 按实例维度索引的活跃 execution 注册表
    active_instance_executions: Arc<DashMap<InstanceId, Arc<DashSet<String>>>>,
    /// Execution semaphore / 执行信号量
    execution_semaphore: Arc<Semaphore>,
    /// Statistics / 统计信息
    statistics: Arc<RwLock<ExecutionStatistics>>,
    /// Request counter / 请求计数器
    request_counter: AtomicU64,
    /// Execution request sender / 执行请求发送器
    work_sender: mpsc::UnboundedSender<ExecutionWorkItem>,
    completion_sender: mpsc::UnboundedSender<ExecutionCompletionEvent>,
    inflight_async_executions: Arc<DashMap<String, InflightAsyncExecution>>,
    sms_channel: Option<Channel>,
    sms_reporter: SmsReporter,
    /// Shutdown signal / 关闭信号
    shutdown_sender: Option<oneshot::Sender<()>>,
}

impl TaskExecutionManager {
    fn upsert_running_execution_response(
        &self,
        execution_id: &str,
        invocation_id: &str,
        task_id: &str,
        function_name: &str,
        instance_id: &str,
    ) {
        self.execution_responses
            .entry(execution_id.to_string())
            .and_modify(|entry| {
                entry.status = ExecutionPublicStatus::Running.as_public_str().to_string();
                entry.instance_id = instance_id.to_string();
                entry.timestamp = SystemTime::now();
            })
            .or_insert_with(|| super::ExecutionResponse {
                execution_id: execution_id.to_string(),
                invocation_id: invocation_id.to_string(),
                task_id: task_id.to_string(),
                function_name: function_name.to_string(),
                instance_id: instance_id.to_string(),
                output_data: Vec::new(),
                status: ExecutionPublicStatus::Running.as_public_str().to_string(),
                error_message: None,
                execution_time_ms: 0,
                metadata: std::collections::HashMap::new(),
                timestamp: SystemTime::now(),
            });
    }

    fn set_execution_response_cancelled(&self, execution_id: &str, reason: Option<&str>) {
        let Some(mut entry) = self.execution_responses.get_mut(execution_id) else {
            return;
        };
        if ExecutionPublicStatus::from_public_str(&entry.status).is_terminal() {
            return;
        }
        entry.status = ExecutionPublicStatus::Cancelled.as_public_str().to_string();
        entry.error_message = reason
            .map(|message| message.to_string())
            .or_else(|| Some("execution terminated".to_string()));
        entry.timestamp = SystemTime::now();
    }

    fn apply_execution_termination_request(
        &self,
        execution_id: &str,
        reason: Option<String>,
    ) -> ExecutionResult<()> {
        if !self.execution_responses.contains_key(execution_id) {
            return Err(ExecutionError::InvalidRequest {
                message: format!("execution not found: {}", execution_id),
            });
        }
        self.set_execution_response_cancelled(execution_id, reason.as_deref());
        crate::spearlet::execution::host_api::termination::register_execution_termination_request(
            execution_id,
            -libc::ECANCELED,
            reason,
        );
        Ok(())
    }

    fn bind_execution_to_instance(&self, instance_id: &str, execution_id: &str) {
        let entry = self
            .active_instance_executions
            .entry(instance_id.to_string())
            .or_insert_with(|| Arc::new(DashSet::new()));
        entry.value().insert(execution_id.to_string());
    }

    fn unbind_execution_from_instance(&self, instance_id: &str, execution_id: &str) {
        let Some(entry) = self.active_instance_executions.get(instance_id) else {
            return;
        };
        let executions = entry.value().clone();
        drop(entry);

        executions.remove(execution_id);
        if executions.is_empty() {
            self.active_instance_executions.remove(instance_id);
        }
    }

    fn active_execution_ids_for_instance(&self, instance_id: &str) -> Vec<String> {
        self.active_instance_executions
            .get(instance_id)
            .map(|entry| {
                entry
                    .value()
                    .iter()
                    .map(|execution_id| execution_id.key().clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn clear_active_instance_executions(&self, instance_id: &str) {
        self.active_instance_executions.remove(instance_id);
    }

    fn report_instance_lifecycle_state_to_sms(&self, instance: &TaskInstance) {
        self.sms_reporter.report_instance(
            instance.task_id().to_string(),
            instance.id().to_string(),
            instance.current_execution_id().unwrap_or_default(),
            chrono::Utc::now().timestamp_millis(),
            local_instance_status_to_sms(&instance.status()) as i32,
        );
    }

    /// Create a new task execution manager / 创建新的任务执行管理器
    pub async fn new(
        config: TaskExecutionManagerConfig,
        runtime_manager: Arc<RuntimeManager>,
        spearlet_config: Arc<crate::spearlet::config::SpearletConfig>,
        sms_channel: Option<Channel>,
    ) -> ExecutionResult<Arc<Self>> {
        let scheduler = Arc::new(InstanceScheduler::new(SchedulingPolicy::RoundRobin));
        let execution_semaphore = Arc::new(Semaphore::new(config.max_concurrent_executions));

        let (work_sender, work_receiver) = mpsc::unbounded_channel();
        let (shutdown_sender, shutdown_receiver) = oneshot::channel();
        let (completion_sender, completion_receiver) = mpsc::unbounded_channel();

        let sms_channel = if let Some(ch) = sms_channel {
            Some(ch)
        } else if spearlet_config.sms_grpc_addr.trim().is_empty() {
            None
        } else {
            Some(
                sms_channel_lazy(&spearlet_config).map_err(|e| ExecutionError::RuntimeError {
                    message: e.to_string(),
                })?,
            )
        };

        let manager = Arc::new(Self {
            config: config.clone(),
            sms_reporter: SmsReporter::new(spearlet_config.clone(), sms_channel.clone()),
            spearlet_config,
            runtime_manager,
            scheduler,
            artifacts: Arc::new(DashMap::new()),
            tasks: Arc::new(DashMap::new()),
            instances: Arc::new(DashMap::new()),
            desired_task_instances: Arc::new(DashMap::new()),
            task_reconcile_locks: Arc::new(DashMap::new()),
            execution_responses: Arc::new(DashMap::new()),
            active_instance_executions: Arc::new(DashMap::new()),
            execution_semaphore,
            statistics: Arc::new(RwLock::new(ExecutionStatistics::default())),
            request_counter: AtomicU64::new(0),
            work_sender,
            completion_sender,
            inflight_async_executions: Arc::new(DashMap::new()),
            sms_channel,
            shutdown_sender: Some(shutdown_sender),
        });

        // Start background tasks / 启动后台任务
        let manager_clone = manager.clone();
        tokio::spawn(async move {
            manager_clone
                .run_execution_work_loop(work_receiver, shutdown_receiver)
                .await;
        });

        let manager_clone = manager.clone();
        tokio::spawn(async move {
            manager_clone.run_health_check_loop().await;
        });

        let manager_clone = manager.clone();
        tokio::spawn(async move {
            manager_clone
                .run_async_completion_loop(completion_receiver)
                .await;
        });

        let manager_clone = manager.clone();
        tokio::spawn(async move {
            manager_clone.run_metrics_collection_loop().await;
        });

        let manager_clone = manager.clone();
        tokio::spawn(async move {
            manager_clone.as_ref().run_instance_heartbeat_loop().await;
        });

        let manager_clone = manager.clone();
        tokio::spawn(async move {
            manager_clone.run_cleanup_loop().await;
        });

        info!("TaskExecutionManager started with config: {:?}", config);
        Ok(manager)
    }

    pub fn list_runtime_types(&self) -> Vec<RuntimeType> {
        self.runtime_manager.list_runtime_types()
    }

    pub async fn submit_invocation(
        &self,
        request: InvokeRequest,
    ) -> ExecutionResult<super::ExecutionResponse> {
        if request.task_id.is_empty() {
            return Err(ExecutionError::InvalidRequest {
                message: "Missing task_id".to_string(),
            });
        }

        let execution_id = if request.execution_id.is_empty() {
            format!(
                "req-{}",
                self.request_counter.fetch_add(1, Ordering::SeqCst)
            )
        } else {
            request.execution_id.clone()
        };

        let invocation_id = if request.invocation_id.is_empty() {
            execution_id.clone()
        } else {
            request.invocation_id.clone()
        };

        let mode =
            ProtoExecutionMode::try_from(request.mode).unwrap_or(ProtoExecutionMode::Unspecified);
        let execution_mode = match mode {
            ProtoExecutionMode::Sync => crate::spearlet::execution::runtime::ExecutionMode::Sync,
            ProtoExecutionMode::Async => crate::spearlet::execution::runtime::ExecutionMode::Async,
            ProtoExecutionMode::Unspecified => {
                crate::spearlet::execution::runtime::ExecutionMode::Sync
            }
        };

        let wait = matches!(
            execution_mode,
            crate::spearlet::execution::runtime::ExecutionMode::Sync
        );

        let input = request.input.clone().unwrap_or_default();
        let function_name = if request.function_name.is_empty() {
            DEFAULT_ENTRY_FUNCTION_NAME.to_string()
        } else {
            request.function_name.clone()
        };

        let mut context_data = std::collections::HashMap::new();
        for (k, v) in request.metadata.iter() {
            context_data.insert(k.clone(), serde_json::Value::String(v.clone()));
        }

        let execution_context = ExecutionContext {
            execution_id: execution_id.clone(),
            function_name: function_name.clone(),
            payload: input.data,
            headers: request.headers.clone(),
            timeout_ms: request.timeout_ms,
            execution_mode,
            wait,
            context_data,
            completion_tx: Some(self.completion_sender.clone()),
        };

        let (response_sender, response_receiver) = oneshot::channel();

        self.execution_responses.insert(
            execution_id.clone(),
            super::ExecutionResponse {
                execution_id: execution_id.clone(),
                invocation_id: invocation_id.clone(),
                task_id: request.task_id.clone(),
                function_name: function_name.clone(),
                instance_id: String::new(),
                output_data: Vec::new(),
                status: "pending".to_string(),
                error_message: None,
                execution_time_ms: 0,
                metadata: std::collections::HashMap::new(),
                timestamp: SystemTime::now(),
            },
        );

        let work_item = ExecutionWorkItem {
            execution_id: execution_id.clone(),
            invocation_id,
            task_id: request.task_id.clone(),
            execution_context,
            response_sender,
        };

        // Send to execution loop / 发送到执行循环
        self.work_sender
            .send(work_item)
            .map_err(|_| ExecutionError::RuntimeError {
                message: "Failed to submit execution request".to_string(),
            })?;

        // Wait for response / 等待响应
        response_receiver
            .await
            .map_err(|_| ExecutionError::RuntimeError {
                message: "Execution request was cancelled".to_string(),
            })?
    }

    /// Get instance by ID / 根据 ID 获取实例
    pub fn get_instance(&self, instance_id: &InstanceId) -> Option<Arc<TaskInstance>> {
        self.instances.get(instance_id).map(|entry| entry.clone())
    }

    /// List all artifacts / 列出所有 artifact
    pub fn list_artifacts(&self) -> Vec<Arc<Artifact>> {
        self.artifacts.iter().map(|entry| entry.clone()).collect()
    }

    /// List all tasks / 列出所有任务
    pub fn list_tasks(&self) -> Vec<Arc<Task>> {
        self.tasks.iter().map(|entry| entry.clone()).collect()
    }

    /// List all instances / 列出所有实例
    pub fn list_instances(&self) -> Vec<Arc<TaskInstance>> {
        self.instances.iter().map(|entry| entry.clone()).collect()
    }

    pub async fn reconcile_node_task_assignments(
        &self,
        desired_by_task: &std::collections::HashMap<String, u32>,
    ) -> ExecutionResult<()> {
        let local_task_ids: Vec<_> = self
            .list_tasks()
            .into_iter()
            .map(|task| task.id().to_string())
            .collect();
        for task_id in &local_task_ids {
            let desired = desired_by_task.get(task_id).copied().unwrap_or(0);
            self.reconcile_task_replica_assignment(task_id, desired).await?;
        }
        for (task_id, desired) in desired_by_task {
            if local_task_ids.iter().any(|local| local == task_id) {
                continue;
            }
            self.reconcile_task_replica_assignment(task_id, *desired).await?;
        }
        Ok(())
    }

    pub async fn reconcile_task_replica_assignment(
        &self,
        task_id: &str,
        desired_instances: u32,
    ) -> ExecutionResult<()> {
        self.desired_task_instances
            .insert(task_id.to_string(), desired_instances);
        let reconcile_lock = self
            .task_reconcile_locks
            .entry(task_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        let _guard = reconcile_lock.lock().await;
        let task = if let Some(task) = self.get_task_by_id(task_id) {
            Some(task)
        } else if desired_instances == 0 {
            None
        } else {
            Some(self.fetch_and_materialize_local_task_by_id(task_id).await?)
        };
        let Some(task) = task else {
            return Ok(());
        };

        let mut instances: Vec<_> = task.instances.iter().map(|entry| entry.clone()).collect();
        let actual_instances = instances.len() as u32;
        if actual_instances < desired_instances {
            for _ in 0..(desired_instances - actual_instances) {
                let _ = self.create_instance_for_task(&task).await?;
            }
            return Ok(());
        }

        if actual_instances <= desired_instances {
            return Ok(());
        }

        instances.sort_by(|a, b| {
            a.current_execution_id()
                .is_some()
                .cmp(&b.current_execution_id().is_some())
                .then_with(|| a.created_at.cmp(&b.created_at))
        });
        for instance in instances
            .into_iter()
            .take((actual_instances - desired_instances) as usize)
        {
            self.drain_and_destroy_instance(instance.id(), None).await?;
        }
        Ok(())
    }

    pub async fn ensure_task_meets_desired_replicas(
        &self,
        task_id: &str,
    ) -> ExecutionResult<()> {
        let Some(task) = self.get_task_by_id(task_id) else {
            return Ok(());
        };
        if task.is_stopping_or_stopped() {
            return Ok(());
        }
        let Some(desired_instances) = self.desired_task_instances.get(task_id).map(|v| *v.value())
        else {
            return Ok(());
        };
        if task.instance_count() >= desired_instances as usize {
            return Ok(());
        }
        self.reconcile_task_replica_assignment(task_id, desired_instances)
            .await
    }

    async fn reconcile_underprovisioned_tasks_once(&self) {
        let task_ids: Vec<_> = self
            .desired_task_instances
            .iter()
            .map(|entry| entry.key().clone())
            .collect();
        for task_id in task_ids {
            if let Err(error) = self.ensure_task_meets_desired_replicas(&task_id).await {
                warn!(
                    task_id = %task_id,
                    error = %error,
                    "Failed to reconcile underprovisioned task replicas"
                );
            }
        }
    }

    pub async fn reconcile_assignments_from_sms(
        &self,
        task_id_filter: Option<&str>,
    ) -> ExecutionResult<()> {
        let channel = self.sms_channel.clone().ok_or_else(|| ExecutionError::RuntimeError {
            message: "sms_grpc_addr is empty".to_string(),
        })?;
        let node_uuid = self.spearlet_config.compute_node_uuid();
        let mut client =
            crate::proto::sms::task_placement_assignment_service_client::TaskPlacementAssignmentServiceClient::new(
                channel,
            );
        let response = client
            .list_node_task_assignments(crate::proto::sms::ListNodeTaskAssignmentsRequest {
                node_uuid,
            })
            .await
            .map_err(|error| ExecutionError::RuntimeError {
                message: format!("list_node_task_assignments failed: {}", error),
            })?
            .into_inner();
        let desired_by_task: std::collections::HashMap<String, u32> = response
            .assignments
            .into_iter()
            .map(|assignment| (assignment.task_id, assignment.desired_instances))
            .collect();

        if let Some(task_id) = task_id_filter {
            let desired = desired_by_task.get(task_id).copied().unwrap_or(0);
            self.reconcile_task_replica_assignment(task_id, desired).await
        } else {
            self.reconcile_node_task_assignments(&desired_by_task).await
        }
    }

    /// Get execution statistics / 获取执行统计信息
    pub fn get_statistics(&self) -> ExecutionStatistics {
        let mut stats = self.statistics.read().clone();

        let mut queued_pending = 0u64;
        let mut running = 0u64;
        for entry in self.execution_responses.iter() {
            match ExecutionPublicStatus::from_public_str(&entry.value().status) {
                ExecutionPublicStatus::Pending => queued_pending += 1,
                ExecutionPublicStatus::Running => running += 1,
                _ => {}
            }
        }

        // Keep both counters in sync while they intentionally represent the same current state
        // through different external naming conventions.
        // 当前这两个指标有意保持一致，只是对外分别承载 queue 与 pending 两种命名语义。
        stats.queue_size = queued_pending;
        stats.pending_executions = queued_pending;
        stats.running_executions = running;
        stats
    }

    /// Get execution status by execution ID / 根据执行ID获取执行状态
    pub fn get_execution_status(&self, execution_id: &str) -> Option<super::ExecutionResponse> {
        self.execution_responses
            .get(execution_id)
            .map(|entry| entry.value().clone())
    }

    pub async fn request_execution_termination(
        &self,
        execution_id: &str,
        reason: Option<String>,
    ) -> ExecutionResult<()> {
        self.apply_execution_termination_request(execution_id, reason)
    }

    /// Compatibility wrapper retained for existing call sites.
    /// 为现有调用点保留的兼容 wrapper。
    pub async fn drain_and_destroy_instance(
        &self,
        instance_id: &str,
        reason: Option<String>,
    ) -> ExecutionResult<()> {
        let instance = self
            .instances
            .get(instance_id)
            .map(|e| e.value().clone())
            .ok_or_else(|| ExecutionError::InstanceNotFound {
                id: instance_id.to_string(),
            })?;

        crate::spearlet::execution::host_api::termination::register_instance_destruction_request(
            instance_id,
            -libc::ECANCELED,
            reason.clone(),
        );

        instance.set_status(super::InstanceStatus::Stopping);
        self.report_instance_lifecycle_state_to_sms(&instance);

        let active_execution_ids = self.active_execution_ids_for_instance(instance_id);

        for execution_id in active_execution_ids {
            self.apply_execution_termination_request(
                &execution_id,
                reason
                    .clone()
                    .map(|r| format!("instance destroyed: {}", r))
                    .or_else(|| Some("instance destroyed".to_string())),
            )?;
        }

        self.stop_and_unregister_instance(&instance).await?;
        self.clear_active_instance_executions(instance_id);
        Ok(())
    }

    /// Delete a task runtime and acknowledge SMS when cleanup finishes / 删除任务运行态并在清理完成后向 SMS 确认
    pub async fn delete_task_runtime(
        &self,
        task_id: &str,
        reason: Option<String>,
    ) -> ExecutionResult<()> {
        if let Some(task) = self.get_task_by_id(task_id) {
            task.mark_stopping();
            let instance_ids: Vec<String> = task
                .instances
                .iter()
                .map(|entry| entry.key().clone())
                .collect();
            for instance_id in instance_ids {
                match self.drain_and_destroy_instance(&instance_id, reason.clone()).await {
                    Ok(_) => {}
                    Err(ExecutionError::InstanceNotFound { .. }) => {}
                    Err(e) => return Err(e),
                }
            }
            finalize_local_task_removal(
                task_id,
                &self.tasks,
                &self.artifacts,
                self.instances.len(),
                &self.statistics,
            )
            .await;
        }
        self.desired_task_instances.remove(task_id);
        self.task_reconcile_locks.remove(task_id);

        self.sms_reporter.acknowledge_task_deletion(task_id.to_string());
        Ok(())
    }

    pub fn list_executions(
        &self,
        task_id: Option<&str>,
        invocation_id: Option<&str>,
        limit: usize,
    ) -> Vec<super::ExecutionResponse> {
        let mut items: Vec<super::ExecutionResponse> = self
            .execution_responses
            .iter()
            .map(|e| e.value().clone())
            .collect();

        if let Some(task_id) = task_id {
            items.retain(|r| r.task_id == task_id);
        }
        if let Some(invocation_id) = invocation_id {
            items.retain(|r| r.invocation_id == invocation_id);
        }

        items.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        if limit > 0 && items.len() > limit {
            items.truncate(limit);
        }
        items
    }

    /// Shutdown the manager / 关闭管理器
    pub async fn shutdown(&mut self) -> ExecutionResult<()> {
        if let Some(sender) = self.shutdown_sender.take() {
            let _ = sender.send(());
        }

        // Stop all instances / 停止所有实例
        for instance_entry in self.instances.iter() {
            let instance = instance_entry.value();
            if let Err(e) = self.stop_and_unregister_instance(instance).await {
                warn!("Failed to stop instance {}: {}", instance.id(), e);
            }
        }

        info!("TaskExecutionManager shutdown completed");
        Ok(())
    }

    /// Main execution loop / 主执行循环
    async fn run_execution_work_loop(
        &self,
        mut work_receiver: mpsc::UnboundedReceiver<ExecutionWorkItem>,
        mut shutdown_receiver: oneshot::Receiver<()>,
    ) {
        info!("Starting execution loop");

        loop {
            tokio::select! {
                Some(request) = work_receiver.recv() => {
                    let manager = self.clone();
                    tokio::spawn(async move { manager.process_execution_work_item(request).await });
                }
                _ = &mut shutdown_receiver => {
                    info!("Execution loop shutting down");
                    break;
                }
            }
        }
    }

    async fn process_execution_work_item(&self, request: ExecutionWorkItem) {
        let start_time = Instant::now();
        let execution_id = request.execution_id.clone();
        let function_name = request.execution_context.function_name.clone();
        debug!(execution_id = %execution_id, "Execution request received");

        // Update statistics / 更新统计信息
        {
            let mut stats = self.statistics.write();
            stats.total_executions += 1;
            stats.running_executions += 1;
        }

        self.execution_responses
            .entry(execution_id.clone())
            .and_modify(|e| {
                e.status = "running".to_string();
                e.timestamp = SystemTime::now();
            })
            .or_insert_with(|| super::ExecutionResponse {
                execution_id: execution_id.clone(),
                invocation_id: request.invocation_id.clone(),
                task_id: request.task_id.clone(),
                function_name: function_name.clone(),
                instance_id: String::new(),
                output_data: Vec::new(),
                status: "running".to_string(),
                error_message: None,
                execution_time_ms: 0,
                metadata: std::collections::HashMap::new(),
                timestamp: SystemTime::now(),
            });

        // Acquire execution permit / 获取执行许可
        let _permit = match self.execution_semaphore.acquire().await {
            Ok(permit) => permit,
            Err(_) => {
                let _ = request
                    .response_sender
                    .send(Err(ExecutionError::RuntimeError {
                        message: "Failed to acquire execution permit".to_string(),
                    }));
                warn!(execution_id = %execution_id, "Failed to acquire execution permit");
                return;
            }
        };
        debug!(execution_id = %execution_id, invocation_id = %request.invocation_id, "Starting execution");
        let result = self
            .execute_existing_task_invocation(
                request.invocation_id.clone(),
                Some(request.task_id.clone()),
                request.execution_context,
            )
            .await;

        let execution_time = start_time.elapsed();
        let execution_time_ms = execution_time.as_millis() as u64;
        match &result {
            Ok(resp) => {
                debug!(execution_id = %execution_id, status = %resp.status, duration_ms = execution_time_ms, "Execution finished");
            }
            Err(e) => {
                warn!(execution_id = %execution_id, error = %e.to_string(), duration_ms = execution_time_ms, "Execution failed");
            }
        }

        // Update statistics / 更新统计信息
        {
            let mut stats = self.statistics.write();
            stats.total_execution_time_ms += execution_time_ms;
            stats.average_execution_time_ms =
                stats.total_execution_time_ms as f64 / stats.total_executions as f64;

            stats.running_executions = stats.running_executions.saturating_sub(1);
            stats.completed_executions += 1;

            match &result {
                Ok(resp) if resp.is_completed() && resp.is_successful() => {
                    stats.successful_executions += 1
                }
                Ok(resp) if resp.is_completed() && !resp.is_successful() => {
                    stats.failed_executions += 1
                }
                Err(_) => stats.failed_executions += 1,
                _ => {}
            }
        }

        match &result {
            Ok(resp) => {
                self.execution_responses
                    .insert(execution_id.clone(), resp.clone());
            }
            Err(e) => {
                let status = match e {
                    ExecutionError::ExecutionTimeout { .. } => "timeout",
                    ExecutionError::ExecutionTerminated { .. } => "terminated",
                    ExecutionError::InstanceDestroyed { .. } => "terminated",
                    _ => "failed",
                }
                .to_string();
                let instance_id = self
                    .execution_responses
                    .get(&execution_id)
                    .map(|entry| entry.value().instance_id.clone())
                    .unwrap_or_default();
                self.execution_responses.insert(
                    execution_id.clone(),
                    super::ExecutionResponse {
                        execution_id: execution_id.clone(),
                        invocation_id: request.invocation_id.clone(),
                        task_id: request.task_id.clone(),
                        function_name,
                        instance_id,
                        output_data: Vec::new(),
                        status,
                        error_message: Some(e.to_string()),
                        execution_time_ms,
                        metadata: std::collections::HashMap::new(),
                        timestamp: SystemTime::now(),
                    },
                );
            }
        }

        // Publish task result to SMS / 将任务结果回写到SMS
        match &result {
            Ok(resp) if resp.is_completed() => {
                let result_status = resp.status.clone();
                let completed_at = chrono::Utc::now().timestamp();
                let mut meta = resp.metadata.clone();
                meta.insert(
                    "execution_time_ms".to_string(),
                    resp.execution_time_ms.to_string(),
                );
                meta.insert("execution_id".to_string(), resp.execution_id.clone());
                if let Some(err) = &resp.error_message {
                    meta.insert("error_message".to_string(), err.clone());
                }
                let task_id = request.task_id.clone();
                if !task_id.is_empty() {
                    self.sms_reporter.update_task_result(
                        &task_id,
                        "".to_string(),
                        result_status,
                        completed_at,
                        meta,
                    );
                }
            }
            Err(e) => {
                let completed_at = chrono::Utc::now().timestamp();
                let mut meta = std::collections::HashMap::new();
                meta.insert(
                    "execution_time_ms".to_string(),
                    execution_time_ms.to_string(),
                );
                meta.insert("execution_id".to_string(), execution_id.clone());
                meta.insert("error_message".to_string(), e.to_string());
                let task_id = request.task_id.clone();
                if !task_id.is_empty() {
                    let status = match e {
                        ExecutionError::ExecutionTimeout { .. } => "timeout",
                        ExecutionError::ExecutionTerminated { .. } => "terminated",
                        ExecutionError::InstanceDestroyed { .. } => "terminated",
                        _ => "failed",
                    }
                    .to_string();
                    self.sms_reporter
                        .update_task_result(&task_id, "".to_string(), status, completed_at, meta);
                }
            }
            _ => {}
        }

        // Send response / 发送响应
        let _ = request.response_sender.send(result);
    }

    /// Execute an invocation against an existing task / 执行一次对已有 task 的调用
    async fn ensure_local_task_for_invocation(
        &self,
        desired_task_id: Option<&str>,
    ) -> ExecutionResult<Arc<Task>> {
        let Some(task_id) = desired_task_id else {
            return Err(ExecutionError::NotSupported {
                operation: "new_task_invocation_via_execute_request_disabled".to_string(),
            });
        };
        if let Some(task) = self.tasks.get(task_id) {
            Ok(task.clone())
        } else {
            self.fetch_and_materialize_local_task_by_id(task_id).await
        }
    }

    /// Mark execution as started in local runtime and SMS. / 在本地运行时与 SMS 中标记执行已开始。
    async fn begin_execution_tracking(
        &self,
        instance: &Arc<TaskInstance>,
        invocation_id: &str,
        function_name: &str,
        execution_id: &str,
        started_at_ms: i64,
        log_next_seq: &mut u64,
    ) -> ExecutionResult<()> {
        if instance.status() != super::InstanceStatus::Running {
            return Err(ExecutionError::RuntimeError {
                message: format!(
                    "Instance {} is not runnable for execution start (status: {:?})",
                    instance.id(),
                    instance.status()
                ),
            });
        }
        self.sms_reporter.report_instance(
            instance.task_id.clone(),
            instance.id().to_string(),
            execution_id.to_string(),
            started_at_ms,
            observed_instance_status_to_sms(&instance.status(), true) as i32,
        );
        instance.set_current_execution_id(Some(execution_id.to_string()));
        self.bind_execution_to_instance(instance.id(), execution_id);
        self.upsert_running_execution_response(
            execution_id,
            invocation_id,
            &instance.task_id,
            function_name,
            instance.id(),
        );
        self.sms_reporter.report_execution(
            invocation_id.to_string(),
            instance.task_id.clone(),
            function_name.to_string(),
            instance.id().to_string(),
            execution_id.to_string(),
            runtime_execution_status_to_sms(RuntimeExecutionStatus::Running) as i32,
            started_at_ms,
            0,
            std::collections::HashMap::new(),
        );
        let _ = self
            .sms_reporter
            .append_execution_logs(
                execution_id,
                log_next_seq,
                vec![SmsAppendLogLine {
                    ts_ms: Some(started_at_ms as u64),
                    stream: Some("system".to_string()),
                    level: Some("info".to_string()),
                    message: format!(
                        "execution_started invocation_id={} task_id={} instance_id={} function_name={}",
                        invocation_id,
                        instance.task_id.clone(),
                        instance.id(),
                        function_name
                    ),
                }],
            )
            .await;
        Ok(())
    }

    /// Flush WASM logs when the backing instance uses the WASM runtime. / 当实例使用 WASM 运行时时刷新日志。
    async fn flush_wasm_logs_if_needed(
        &self,
        instance: &Arc<TaskInstance>,
        execution_id: &str,
        log_next_seq: &mut u64,
        wasm_last_seq: &mut u64,
    ) {
        if instance.config.runtime_type == super::RuntimeType::Wasm {
            let _ = self
                .flush_wasm_logs_to_sms(execution_id, log_next_seq, wasm_last_seq)
                .await;
        }
    }

    /// Write the final completion log line and finalize log ingestion. / 写入最终完成日志并结束日志摄取。
    async fn finalize_execution_logs_after_completion(
        &self,
        execution_id: &str,
        completed_at_ms: i64,
        final_state: FinalExecutionState,
        duration_ms: u64,
        log_next_seq: &mut u64,
    ) {
        let _ = self
            .sms_reporter
            .append_execution_logs(
                execution_id,
                log_next_seq,
                vec![build_completion_log_line(
                    completed_at_ms,
                    final_state,
                    duration_ms,
                )],
            )
            .await;
        let _ = self.sms_reporter.finalize_execution_logs(execution_id).await;
    }

    /// Report the final execution and instance state after completion. / 在完成后上报最终执行态与实例态。
    fn report_final_execution_state(
        &self,
        invocation_id: &str,
        task_id: &str,
        function_name: &str,
        instance_id: &str,
        execution_id: &str,
        final_state: FinalExecutionState,
        started_at_ms: i64,
        completed_at_ms: i64,
        current_execution_id: &str,
        instance_status: i32,
        metadata: std::collections::HashMap<String, String>,
    ) {
        self.sms_reporter.report_execution(
            invocation_id.to_string(),
            task_id.to_string(),
            function_name.to_string(),
            instance_id.to_string(),
            execution_id.to_string(),
            final_state.sms_status,
            started_at_ms,
            completed_at_ms,
            metadata,
        );
        self.sms_reporter.report_instance(
            task_id.to_string(),
            instance_id.to_string(),
            current_execution_id.to_string(),
            completed_at_ms,
            instance_status,
        );
    }

    /// Store an inflight async execution context and return the public running response.
    /// 保存进行中的异步执行上下文，并返回对外 running 响应。
    fn track_inflight_async_execution(
        &self,
        execution_id: String,
        invocation_id: String,
        task_id: String,
        function_name: String,
        instance_id: String,
        started_at_ms: i64,
        log_next_seq: u64,
        metadata: std::collections::HashMap<String, String>,
    ) -> super::ExecutionResponse {
        self.inflight_async_executions.insert(
            execution_id.clone(),
            InflightAsyncExecution {
                invocation_id: invocation_id.clone(),
                task_id: task_id.clone(),
                function_name: function_name.clone(),
                instance_id: instance_id.clone(),
                started_at_ms,
                log_next_seq,
                wasm_last_seq: 0,
            },
        );

        super::ExecutionResponse {
            execution_id,
            invocation_id,
            task_id,
            function_name,
            instance_id,
            status: "running".to_string(),
            output_data: Vec::new(),
            execution_time_ms: 0,
            error_message: None,
            metadata,
            timestamp: SystemTime::now(),
        }
    }

    /// Build a finished execution response returned to the caller. / 构建返回给调用方的已完成执行响应。
    fn build_finished_execution_response(
        &self,
        execution_id: String,
        invocation_id: String,
        task_id: String,
        function_name: String,
        instance_id: String,
        output_data: Vec<u8>,
        final_state: FinalExecutionState,
        error_message: Option<String>,
        execution_time_ms: u64,
        metadata: std::collections::HashMap<String, String>,
    ) -> super::ExecutionResponse {
        super::ExecutionResponse {
            execution_id,
            invocation_id,
            task_id,
            function_name,
            instance_id,
            output_data,
            status: final_state.public_status.to_string(),
            error_message,
            execution_time_ms,
            metadata,
            timestamp: SystemTime::now(),
        }
    }

    /// Handle a runtime execution error after start bookkeeping has already happened. / 处理运行时执行报错，此时启动期记账已经完成。
    async fn handle_started_execution_error(
        &self,
        instance: &Arc<TaskInstance>,
        invocation_id: &str,
        function_name: &str,
        execution_id: &str,
        started_at_ms: i64,
        log_next_seq: &mut u64,
        wasm_last_seq: &mut u64,
        error_message: &str,
    ) {
        let completed_at_ms = chrono::Utc::now().timestamp_millis();
        let _ = self
            .sms_reporter
            .append_execution_logs(
                execution_id,
                log_next_seq,
                vec![SmsAppendLogLine {
                    ts_ms: Some(completed_at_ms as u64),
                    stream: Some("system".to_string()),
                    level: Some("error".to_string()),
                    message: format!("execution_failed error={}", error_message),
                }],
            )
            .await;

        self.flush_wasm_logs_if_needed(instance, execution_id, log_next_seq, wasm_last_seq)
            .await;
        let _ = self.sms_reporter.finalize_execution_logs(execution_id).await;

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("error_message".to_string(), error_message.to_string());
        instance.set_current_execution_id(None);
        self.unbind_execution_from_instance(instance.id(), execution_id);
        self.report_final_execution_state(
            invocation_id,
            &instance.task_id,
            function_name,
            instance.id(),
            execution_id,
            FinalExecutionState::from_result_flags(false, true),
            started_at_ms,
            completed_at_ms,
            "",
            observed_instance_status_to_sms(&instance.status(), false) as i32,
            metadata,
        );
    }

    async fn execute_existing_task_invocation(
        &self,
        invocation_id: String,
        desired_task_id: Option<String>,
        execution_context: ExecutionContext,
    ) -> ExecutionResult<super::ExecutionResponse> {
        let task = self
            .ensure_local_task_for_invocation(desired_task_id.as_deref())
            .await?;

        let instance = self
            .scheduler
            .select_instance(&task)
            .await?
            .ok_or_else(|| ExecutionError::SchedulingError {
                message: format!("No ready instances available for task {}", task.id()),
            })?;

        let function_name = execution_context.function_name.clone();
        let runtime = self
            .runtime_manager
            .get_runtime(&task.spec.runtime_type)
            .ok_or_else(|| ExecutionError::RuntimeError {
                message: format!("Runtime not found for type: {:?}", task.spec.runtime_type),
            })?;
        let execution_id = execution_context.execution_id.clone();
        let started_at_ms = chrono::Utc::now().timestamp_millis();
        let mut log_next_seq: u64 = 1;
        let mut wasm_last_seq: u64 = 0;

        if instance.config.runtime_type == super::RuntimeType::Wasm {
            crate::spearlet::execution::host_api::clear_wasm_logs_by_execution(&execution_id);
        }

        self.begin_execution_tracking(
            &instance,
            &invocation_id,
            &function_name,
            &execution_id,
            started_at_ms,
            &mut log_next_seq,
        )
        .await?;

        let runtime_response = match runtime.execute(&instance, execution_context).await {
            Ok(r) => r,
            Err(e) => {
                let msg = e.to_string();
                self.handle_started_execution_error(
                    &instance,
                    &invocation_id,
                    &function_name,
                    &execution_id,
                    started_at_ms,
                    &mut log_next_seq,
                    &mut wasm_last_seq,
                    &msg,
                )
                    .await;
                return Err(e);
            }
        };

        debug!(
            execution_id = %execution_id,
            invocation_id = %invocation_id,
            instance_id = %instance.id(),
            runtime_type = ?instance.config.runtime_type,
            runtime_execution_mode = ?runtime_response.execution_mode,
            runtime_execution_status = ?runtime_response.execution_status,
            runtime_duration_ms = runtime_response.duration_ms,
            "runtime.execute.returned"
        );

        // Convert RuntimeExecutionResponse to ExecutionResponse / 转换运行时响应到执行响应
        let is_successful = runtime_response.is_successful();
        let has_failed = runtime_response.has_failed();
        let is_running = matches!(
            runtime_response.execution_status,
            crate::spearlet::execution::runtime::ExecutionStatus::Running
        );
        let error_message = runtime_response
            .error
            .as_ref()
            .map(Self::extract_error_message);
        let duration_ms = runtime_response.duration_ms;
        let metadata = stringify_runtime_metadata(runtime_response.metadata);
        let data = runtime_response.data;

        if is_running {
            return Ok(self.track_inflight_async_execution(
                execution_id,
                invocation_id,
                desired_task_id.unwrap_or_else(|| instance.task_id.clone()),
                function_name,
                instance.id().to_string(),
                started_at_ms,
                log_next_seq,
                metadata,
            ));
        }

        let completed_at_ms = chrono::Utc::now().timestamp_millis();
        let final_state = FinalExecutionState::from_result_flags(is_successful, has_failed);
        let final_meta =
            enrich_final_metadata(metadata.clone(), duration_ms, error_message.as_deref());
        instance.set_current_execution_id(None);
        self.unbind_execution_from_instance(instance.id(), &execution_id);
        self.report_final_execution_state(
            &invocation_id,
            &instance.task_id,
            &function_name,
            instance.id(),
            &execution_id,
            final_state,
            started_at_ms,
            completed_at_ms,
            "",
            observed_instance_status_to_sms(&instance.status(), false) as i32,
            final_meta,
        );

        self.flush_wasm_logs_if_needed(&instance, &execution_id, &mut log_next_seq, &mut wasm_last_seq)
            .await;
        debug!(
            execution_id = %execution_id,
            invocation_id = %invocation_id,
            instance_id = %instance.id(),
            final_status = final_state.sms_status,
            "manager.logs.finalize.plan"
        );
        self.finalize_execution_logs_after_completion(
            &execution_id,
            completed_at_ms,
            final_state,
            duration_ms,
            &mut log_next_seq,
        )
            .await;

        Ok(self.build_finished_execution_response(
            execution_id,
            invocation_id,
            desired_task_id.unwrap_or_else(|| instance.task_id.clone()),
            function_name,
            instance.id().to_string(),
            data,
            final_state,
            error_message,
            duration_ms,
            metadata,
        ))
    }

    async fn flush_wasm_logs_to_sms(
        &self,
        execution_id: &str,
        next_seq: &mut u64,
        wasm_last_seq: &mut u64,
    ) -> ExecutionResult<()> {
        let logs = crate::spearlet::execution::host_api::get_wasm_logs_by_execution(
            execution_id,
            Some(*wasm_last_seq),
            4096,
        );
        debug!(
            execution_id = %execution_id,
            count = logs.len(),
            "wasm_logs.flush_to_sms.called"
        );
        if logs.is_empty() {
            return Ok(());
        }

        if let Some(last) = logs.last() {
            *wasm_last_seq = (*wasm_last_seq).max(last.seq);
        }

        debug!(
            execution_id = %execution_id,
            count = logs.len(),
            logs = ?logs,
            "wasm_logs.flush_to_sms"
        );

        let mut batch = Vec::new();
        for e in logs {
            batch.push(SmsAppendLogLine {
                ts_ms: Some(e.ts_ms),
                stream: Some("wasm".to_string()),
                level: Some(e.level),
                message: e.message,
            });
            if batch.len() >= 200 {
                let _ = self
                    .sms_reporter
                    .append_execution_logs(
                        execution_id,
                        next_seq,
                        std::mem::take(&mut batch),
                    )
                    .await;
            }
        }
        if !batch.is_empty() {
            let _ = self.sms_reporter.append_execution_logs(execution_id, next_seq, batch).await;
        }
        Ok(())
    }

    pub fn get_artifact_by_id(&self, artifact_id: &str) -> Option<Arc<Artifact>> {
        self.artifacts.get(artifact_id).map(|a| a.clone())
    }

    /// Compatibility getter retained for external tests and callers.
    /// 为外部测试和调用方保留的兼容查询入口。
    pub fn get_artifact(&self, artifact_id: &ArtifactId) -> Option<Arc<Artifact>> {
        self.get_artifact_by_id(artifact_id)
    }

    pub fn create_artifact_with_id(
        &self,
        artifact_id: String,
        spec: super::artifact::ArtifactSpec,
    ) -> ExecutionResult<Arc<Artifact>> {
        if self.artifacts.len() >= self.config.max_artifacts {
            return Err(ExecutionError::ResourceExhausted {
                message: format!(
                    "Maximum artifacts limit reached: {}",
                    self.config.max_artifacts
                ),
            });
        }
        let artifact = Arc::new(Artifact::new_with_id(artifact_id.clone(), spec));
        self.artifacts.insert(artifact_id.clone(), artifact.clone());
        {
            let mut stats = self.statistics.write();
            stats.active_artifacts = self.artifacts.len() as u64;
        }
        Ok(artifact)
    }

    pub fn ensure_artifact_with_id(
        &self,
        artifact_id: String,
        spec: super::artifact::ArtifactSpec,
    ) -> ExecutionResult<Arc<Artifact>> {
        if let Some(existing) = self.get_artifact_by_id(&artifact_id) {
            return Ok(existing);
        }
        self.create_artifact_with_id(artifact_id, spec)
    }

    /// Task helpers / 任务相关辅助方法
    pub fn get_task_by_id(&self, task_id: &str) -> Option<Arc<Task>> {
        self.tasks.get(task_id).map(|t| t.clone())
    }

    /// Compatibility getter retained for external tests and callers.
    /// 为外部测试和调用方保留的兼容查询入口。
    pub fn get_task(&self, task_id: &TaskId) -> Option<Arc<Task>> {
        self.get_task_by_id(task_id)
    }

    pub fn create_task_with_id(
        &self,
        task_id: String,
        artifact: &Arc<Artifact>,
        spec: super::task::TaskSpec,
    ) -> ExecutionResult<Arc<Task>> {
        if artifact.task_count() >= self.config.max_tasks_per_artifact {
            return Err(ExecutionError::ResourceExhausted {
                message: format!(
                    "Maximum tasks per artifact limit reached: {}",
                    self.config.max_tasks_per_artifact
                ),
            });
        }
        let task = Arc::new(Task::new_with_id(
            task_id.clone(),
            artifact.id().to_string(),
            spec,
        ));
        self.tasks.insert(task_id.clone(), task.clone());
        artifact.add_task(task.clone())?;
        {
            let mut stats = self.statistics.write();
            stats.active_tasks = self.tasks.len() as u64;
        }
        Ok(task)
    }

    pub fn ensure_task_with_id(
        &self,
        task_id: String,
        artifact: &Arc<Artifact>,
        spec: super::task::TaskSpec,
    ) -> ExecutionResult<Arc<Task>> {
        // Fast path: task already exists locally.
        // 快速路径：本地已存在 task 直接返回。
        if let Some(existing) = self.get_task_by_id(&task_id) {
            return Ok(existing);
        }
        // Slow path: create task and attach it under the artifact.
        // 慢路径：创建 task 并挂到 artifact 下。
        self.create_task_with_id(task_id, artifact, spec)
    }

    /// Sync local runtime state from an SMS task snapshot.
    /// 使用 SMS task 快照同步本地运行态 task。
    pub async fn materialize_local_task_from_sms_snapshot(
        &self,
        sms_task: &crate::proto::sms::Task,
    ) -> ExecutionResult<Arc<Task>> {
        materialize_sms_task(self, sms_task)
    }

    pub async fn materialize_local_artifact_from_sms_snapshot(
        &self,
        sms_task: &crate::proto::sms::Task,
    ) -> ExecutionResult<Arc<Artifact>> {
        materialize_local_artifact_from_sms_task(self, sms_task)
    }

    pub async fn materialize_local_task_from_sms_snapshot_with_artifact(
        &self,
        sms_task: &crate::proto::sms::Task,
        artifact: &Arc<Artifact>,
    ) -> ExecutionResult<Arc<Task>> {
        materialize_local_task_from_sms_task(self, sms_task, artifact)
    }

    /// Fetch an SMS task by ID and sync it into local runtime state.
    /// 根据 task_id 从 SMS 拉取 task 并同步到本地运行态。
    pub async fn fetch_and_materialize_local_task_by_id(
        &self,
        task_id: &str,
    ) -> ExecutionResult<Arc<Task>> {
        let sms_task = fetch_sms_task(self.sms_channel.clone(), &self.spearlet_config, task_id).await?;
        self.materialize_local_task_from_sms_snapshot(&sms_task).await
    }

    /// Sync a task from an SMS create event into local runtime state and mark it ready.
    /// 根据 SMS create 事件同步本地 task，并在完成后标记为 ready。
    pub async fn materialize_local_task_from_sms_create_event(
        &self,
        task_id: &str,
    ) -> ExecutionResult<Arc<Task>> {
        let task = self.fetch_and_materialize_local_task_by_id(task_id).await?;
        task.set_status(crate::spearlet::execution::task::TaskStatus::Ready);
        Ok(task)
    }

    /// Get or create instance / 获取或创建实例
    /// Ensure the task can still create a new instance. / 确保该 task 仍允许创建新实例。
    fn ensure_instance_capacity(&self, task: &Arc<Task>) -> ExecutionResult<()> {
        if task.instance_count() >= self.config.max_instances_per_task {
            return Err(ExecutionError::ResourceExhausted {
                message: format!(
                    "Maximum instances per task limit reached: {}",
                    self.config.max_instances_per_task
                ),
            });
        }
        Ok(())
    }

    /// Resolve the runtime implementation for a task. / 为 task 解析对应的 runtime 实现。
    fn resolve_runtime_for_task(
        &self,
        task: &Arc<Task>,
    ) -> ExecutionResult<&dyn super::runtime::Runtime> {
        self.runtime_manager
            .get_runtime(&task.spec.runtime_type)
            .ok_or_else(|| ExecutionError::RuntimeError {
                message: format!("Runtime not found for type: {:?}", task.spec.runtime_type),
            })
    }

    /// Build instance config and inject artifact snapshot when available. / 构建实例配置，并在可用时注入 artifact 快照。
    fn prepare_instance_config(
        &self,
        task: &Arc<Task>,
    ) -> super::instance::InstanceConfig {
        let mut instance_config = task.create_instance_config();
        if let Some(artifact_entry) = self.artifacts.get(task.artifact_id()) {
            let artifact = artifact_entry.value();
            instance_config.artifact = Some(super::instance::ArtifactSnapshot {
                location: artifact.spec.location.clone(),
                checksum_sha256: artifact.spec.checksum_sha256.clone(),
            });
            debug!(
                task_id = %task.id(),
                artifact_id = %task.artifact_id(),
                location = %artifact.spec.location.clone().unwrap_or_default(),
                checksum = %artifact.spec.checksum_sha256.clone().unwrap_or_default(),
                "Injected artifact snapshot into instance config"
            );
        } else {
            debug!(
                task_id = %task.id(),
                artifact_id = %task.artifact_id(),
                "Artifact not found in manager when preparing instance; snapshot injection skipped"
            );
        }
        instance_config
    }

    /// Create and start a new runtime instance with timeout protection. / 带超时保护地创建并启动一个新的 runtime 实例。
    async fn create_and_start_instance(
        &self,
        runtime: &dyn super::runtime::Runtime,
        instance_config: &super::instance::InstanceConfig,
    ) -> ExecutionResult<Arc<TaskInstance>> {
        let instance = timeout(
            Duration::from_millis(self.config.instance_creation_timeout_ms),
            runtime.create_instance(instance_config),
        )
        .await
        .map_err(|_| ExecutionError::ExecutionTimeout {
            timeout_ms: self.config.instance_creation_timeout_ms,
        })??;
        runtime.start_instance(&instance).await?;
        Ok(instance)
    }

    /// Register a started instance into manager, task, scheduler, and metrics. / 将已启动实例注册到 manager、task、scheduler 与指标中。
    async fn register_started_instance(
        &self,
        task: &Arc<Task>,
        instance: Arc<TaskInstance>,
    ) -> ExecutionResult<Arc<TaskInstance>> {
        instance.set_status(InstanceStatus::Running);
        instance.set_health_status(HealthStatus::Healthy);
        self.instances
            .insert(instance.id().to_string(), instance.clone());
        task.add_instance(instance.clone())?;
        self.scheduler.add_instance(instance.clone()).await?;

        self.sms_reporter.update_task_status(
            task.id(),
            self.task_status_to_sms(task),
            Some("instance initialized".to_string()),
        );

        {
            let mut stats = self.statistics.write();
            stats.active_instances = self.instances.len() as u64;
        }

        self.report_instance_lifecycle_state_to_sms(&instance);

        Ok(instance)
    }

    pub async fn create_instance_for_task(
        &self,
        task: &Arc<Task>,
    ) -> ExecutionResult<Arc<TaskInstance>> {
        if task.is_stopping_or_stopped() {
            return Err(ExecutionError::InstanceDestroyed {
                message: format!("task {} is being deleted", task.id()),
            });
        }
        self.ensure_instance_capacity(task)?;
        let runtime = self.resolve_runtime_for_task(task)?;
        let instance_config = self.prepare_instance_config(task);
        let instance = self
            .create_and_start_instance(runtime, &instance_config)
            .await?;
        let instance = self.register_started_instance(task, instance).await?;

        info!("Created new instance: {}", instance.id());
        Ok(instance)
    }

    /// Stop instance / 停止实例
    async fn stop_and_unregister_instance(&self, instance: &Arc<TaskInstance>) -> ExecutionResult<()> {
        let runtime = self
            .runtime_manager
            .get_runtime(&instance.config.runtime_type)
            .ok_or_else(|| ExecutionError::RuntimeError {
                message: format!(
                    "Runtime not found for type: {:?}",
                    instance.config.runtime_type
                ),
            })?;
        runtime.stop_instance(instance).await?;

        let ts_ms = chrono::Utc::now().timestamp_millis();
        let current_execution_id = instance.current_execution_id().unwrap_or_default();
        self.sms_reporter.report_instance(
            instance.task_id().to_string(),
            instance.id().to_string(),
            current_execution_id,
            ts_ms,
            local_instance_status_to_sms(&instance.status()) as i32,
        );

        self.instances.remove(instance.id());
        self.scheduler.remove_instance(&instance.id).await?;

        let task_id = instance.task_id().to_string();
        if let Some(task_entry) = self
            .tasks
            .iter()
            .find(|entry| entry.value().id() == task_id)
        {
            let task = task_entry.value();
            if let Err(e) = task.remove_instance(instance.id()) {
                warn!(
                    "Failed to remove instance {} from task {}: {}",
                    instance.id(),
                    task_id,
                    e
                );
            }
            if task.instance_count() == 0 {
                self.sms_reporter.update_task_status(
                    &task_id,
                    self.task_status_to_sms(task),
                    Some("no instances".to_string()),
                );
            }
        }

        // Update statistics / 更新统计信息
        {
            let mut stats = self.statistics.write();
            stats.active_instances = self.instances.len() as u64;
        }

        if let Err(error) = self
            .sms_reporter
            .delete_instance_via_sms(&task_id, instance.id())
            .await
        {
            warn!(
                instance_id = %instance.id(),
                task_id = %task_id,
                error = %error,
                "Failed to delete instance record from SMS"
            );
        }

        info!("Stopped instance: {}", instance.id());
        Ok(())
    }

    /// Health check loop / 健康检查循环
    async fn run_health_check_loop(&self) {
        let mut interval =
            tokio::time::interval(Duration::from_millis(self.config.health_check_interval_ms));

        loop {
            interval.tick().await;
            self.process_health_checks_once().await;
        }
    }

    async fn run_async_completion_loop(
        &self,
        mut receiver: mpsc::UnboundedReceiver<ExecutionCompletionEvent>,
    ) {
        while let Some(ev) = receiver.recv().await {
            let _ = self.complete_inflight_execution(ev).await;
        }
    }

    /// Release runtime-local async execution state and clear current execution markers. / 释放运行时本地异步执行状态并清理当前执行标记。
    async fn release_inflight_execution_runtime_state(
        &self,
        execution_id: &str,
        inflight_execution: &InflightAsyncExecution,
    ) {
        crate::spearlet::execution::host_api::user_stream::map_ws_close_to_channels(execution_id);

        if let Some(instance) = self.instances.get(&inflight_execution.instance_id) {
            if instance.value().config.runtime_type == super::RuntimeType::Wasm {
                if let Some(wasm_handle) = instance
                    .value()
                    .get_runtime_handle::<crate::spearlet::execution::runtime::wasm::WasmInstanceHandle>(
                    )
                {
                    let mut state = wasm_handle.state.lock().await;
                    state.is_running = false;
                    state.current_function = None;
                }
            }
            instance.value().set_current_execution_id(None);
        }
        self.unbind_execution_from_instance(&inflight_execution.instance_id, execution_id);
    }

    /// Update per-instance completion metrics for async executions. / 为异步执行更新实例级完成指标。
    fn record_inflight_execution_completion_metrics(
        &self,
        inflight_execution: &InflightAsyncExecution,
        final_state: FinalExecutionState,
        duration_ms: u64,
    ) {
        let is_successful =
            ExecutionPublicStatus::from_public_str(final_state.public_status).is_successful();
        if let Some(instance) = self.instances.get(&inflight_execution.instance_id) {
            instance
                .value()
                .record_request_completion(is_successful, duration_ms as f64);
        }
    }

    /// Finalize async execution logs and publish final SMS state. / 完成异步执行日志封口并发布最终 SMS 状态。
    async fn finalize_inflight_execution(
        &self,
        inflight_execution: &InflightAsyncExecution,
        ev: &ExecutionCompletionEvent,
        final_state: FinalExecutionState,
    ) -> std::collections::HashMap<String, String> {
        let completed_at_ms = ev.completed_at_ms;
        let mut log_next_seq = inflight_execution.log_next_seq;
        let mut wasm_last_seq = inflight_execution.wasm_last_seq;
        if self
            .instances
            .get(&inflight_execution.instance_id)
            .map(|instance| instance.value().config.runtime_type == super::RuntimeType::Wasm)
            .unwrap_or(false)
        {
            let _ = self
                .flush_wasm_logs_to_sms(&ev.execution_id, &mut log_next_seq, &mut wasm_last_seq)
                .await;
        }

        self.finalize_execution_logs_after_completion(
            &ev.execution_id,
            completed_at_ms,
            final_state,
            ev.duration_ms,
            &mut log_next_seq,
        )
        .await;

        let metadata = enrich_final_metadata(
            stringify_runtime_metadata(ev.runtime_metadata.clone()),
            ev.duration_ms,
            ev.error_message.as_deref(),
        );
        let instance_status = self
            .instances
            .get(&inflight_execution.instance_id)
            .map(|instance| {
                observed_instance_status_to_sms(&instance.status(), false) as i32
            })
            .unwrap_or(crate::proto::sms::InstanceStatus::Unknown as i32);
        self.report_final_execution_state(
            &inflight_execution.invocation_id,
            &inflight_execution.task_id,
            &inflight_execution.function_name,
            &inflight_execution.instance_id,
            &ev.execution_id,
            final_state,
            inflight_execution.started_at_ms,
            completed_at_ms,
            "",
            instance_status,
            metadata.clone(),
        );
        crate::spearlet::execution::host_api::clear_wasm_logs_by_execution(&ev.execution_id);
        metadata
    }

    /// Persist the final async execution response in the in-memory execution index. / 将最终异步执行响应写入内存执行索引。
    fn persist_completed_async_execution_response(
        &self,
        inflight_execution: InflightAsyncExecution,
        ev: ExecutionCompletionEvent,
        final_state: FinalExecutionState,
        metadata: std::collections::HashMap<String, String>,
    ) {
        self.execution_responses.insert(
            ev.execution_id.clone(),
            super::ExecutionResponse {
                execution_id: ev.execution_id,
                invocation_id: inflight_execution.invocation_id,
                task_id: inflight_execution.task_id,
                function_name: inflight_execution.function_name,
                instance_id: inflight_execution.instance_id,
                output_data: ev.output,
                status: final_state.public_status.to_string(),
                error_message: ev.error_message,
                execution_time_ms: ev.duration_ms,
                metadata,
                timestamp: SystemTime::now(),
            },
        );
    }

    async fn complete_inflight_execution(
        &self,
        ev: ExecutionCompletionEvent,
    ) -> ExecutionResult<()> {
        let Some((_, inflight_execution)) = self.inflight_async_executions.remove(&ev.execution_id) else {
            return Ok(());
        };

        info!(
            execution_id = %ev.execution_id,
            status = ?ev.execution_status,
            duration_ms = ev.duration_ms,
            error_message = %ev.error_message.clone().unwrap_or_default(),
            "async execution completed; closing user stream channels"
        );

        self.release_inflight_execution_runtime_state(&ev.execution_id, &inflight_execution)
            .await;
        let final_state = FinalExecutionState::from_runtime_status(ev.execution_status.clone());
        self.record_inflight_execution_completion_metrics(
            &inflight_execution,
            final_state,
            ev.duration_ms,
        );
        let metadata = self
            .finalize_inflight_execution(&inflight_execution, &ev, final_state)
            .await;
        self.persist_completed_async_execution_response(
            inflight_execution,
            ev,
            final_state,
            metadata,
        );

        Ok(())
    }

    async fn process_health_checks_once(&self) {
        let instances: Vec<Arc<TaskInstance>> =
            self.instances.iter().map(|e| e.value().clone()).collect();
        for instance in instances {
            if let Some(runtime) = self
                .runtime_manager
                .get_runtime(&instance.config.runtime_type)
            {
                match runtime.health_check(&instance).await {
                    Ok(true) => {
                        instance.update_metrics(|m| {
                            m.health_check_successes = m.health_check_successes.saturating_add(1);
                            m.health_check_failures = 0;
                            m.last_health_check_time = Some(SystemTime::now());
                        });
                    }
                    Ok(false) | Err(_) => {
                        instance.set_status(InstanceStatus::Unhealthy);
                        instance.update_metrics(|m| {
                            m.health_check_failures = m.health_check_failures.saturating_add(1);
                            m.last_health_check_time = Some(SystemTime::now());
                        });

                        let threshold = self
                            .tasks
                            .iter()
                            .find(|t| t.value().id() == instance.task_id())
                            .map(|t| t.value().spec.health_check.failure_threshold)
                            .unwrap_or(1);

                        if instance.get_metrics().health_check_failures >= threshold {
                            let _ = self.stop_and_unregister_instance(&instance).await;
                        }
                    }
                }
            }
        }
        self.reconcile_underprovisioned_tasks_once().await;
    }

    /// Metrics collection loop / 指标收集循环
    async fn run_metrics_collection_loop(&self) {
        let mut interval = tokio::time::interval(Duration::from_millis(
            self.config.metrics_collection_interval_ms,
        ));

        loop {
            interval.tick().await;

            let instances: Vec<Arc<TaskInstance>> =
                self.instances.iter().map(|e| e.value().clone()).collect();

            for instance in instances {
                match instance.status() {
                    InstanceStatus::Ready | InstanceStatus::Running | InstanceStatus::Busy => {}
                    _ => continue,
                }

                if let Some(runtime) = self
                    .runtime_manager
                    .get_runtime(&instance.config.runtime_type)
                {
                    if let Ok(metrics) = runtime.get_metrics(&instance).await {
                        debug!(
                            "Collected metrics for instance {}: {:?}",
                            instance.id(),
                            metrics
                        );
                    }
                }
            }
        }
    }

    async fn run_instance_heartbeat_loop(&self) {
        let mut interval = tokio::time::interval(Duration::from_millis(
            self.config.instance_heartbeat_interval_ms.max(1),
        ));

        loop {
            interval.tick().await;
            let ts_ms = chrono::Utc::now().timestamp_millis();
            let instances: Vec<Arc<TaskInstance>> =
                self.instances.iter().map(|e| e.value().clone()).collect();

            for instance in instances {
                match instance.status() {
                    InstanceStatus::Ready | InstanceStatus::Running | InstanceStatus::Busy => {}
                    _ => continue,
                }
                let current_execution_id = instance.current_execution_id().unwrap_or_default();
                self.sms_reporter.report_instance(
                    instance.task_id.clone(),
                    instance.id().to_string(),
                    current_execution_id,
                    ts_ms,
                    observed_instance_status_to_sms(
                        &instance.status(),
                        instance.current_execution_id().is_some(),
                    ) as i32,
                );
            }
        }
    }

    /// Cleanup loop / 清理循环
    async fn run_cleanup_loop(&self) {
        let mut interval =
            tokio::time::interval(Duration::from_millis(self.config.cleanup_interval_ms));

        loop {
            interval.tick().await;

            let now = SystemTime::now();

            // Cleanup idle instances / 清理空闲实例
            let mut instances_to_remove = Vec::new();
            let idle_threshold = Duration::from_millis(self.config.instance_idle_timeout_ms);
            for instance_entry in self.instances.iter() {
                let instance = instance_entry.value();
                if instance.is_idle(idle_threshold) {
                    instances_to_remove.push(instance.clone());
                }
            }

            for instance in instances_to_remove {
                if let Err(e) = self.stop_and_unregister_instance(&instance).await {
                    warn!("Failed to cleanup idle instance {}: {}", instance.id(), e);
                }
            }

            // Cleanup idle tasks / 清理空闲任务
            let mut tasks_to_remove = Vec::new();
            for task_entry in self.tasks.iter() {
                let task_id = task_entry.key().clone();
                let task = task_entry.value();
                if task.instance_count() == 0 {
                    let idle_duration = task.time_since_update();
                    let not_active = !task.is_retained_when_idle();
                    if not_active
                        || idle_duration.as_millis() > self.config.task_idle_timeout_ms as u128
                    {
                        tasks_to_remove.push(task_id);
                    }
                }
            }

            for task_id in tasks_to_remove {
                if let Some((_, task)) = self.tasks.remove(&task_id) {
                    // Publish INACTIVE before removal / 移除前上报INACTIVE状态
                    self.sms_reporter.update_task_status(
                        task.id(),
                        self.task_status_to_sms(&task),
                        Some("cleanup".to_string()),
                    );
                    if let Some(artifact_entry) = self.artifacts.get(task.artifact_id()) {
                        let artifact = artifact_entry.value();
                        if let Err(e) = artifact.remove_task(task.id()) {
                            warn!(
                                "Failed to remove task {} from artifact {}: {}",
                                task_id,
                                task.artifact_id(),
                                e
                            );
                        }
                    }
                    info!("Cleaned up idle task: {}", task_id);
                } else {
                    info!("Cleaned up idle task: {}", task_id);
                }
            }

            // Cleanup idle artifacts / 清理空闲 artifact
            let mut artifacts_to_remove = Vec::new();
            for artifact_entry in self.artifacts.iter() {
                let artifact_id = artifact_entry.key().clone();
                let artifact = artifact_entry.value();
                if artifact.task_count() == 0 {
                    let idle_duration = artifact.time_since_update();
                    if idle_duration.as_millis() > self.config.artifact_idle_timeout_ms as u128 {
                        artifacts_to_remove.push(artifact_id);
                    }
                }
            }

            for artifact_id in artifacts_to_remove {
                if let Some((_, _artifact)) = self.artifacts.remove(&artifact_id) {
                    info!("Cleaned up idle artifact: {}", artifact_id);
                }
            }

            let completed_execution_ttl = Duration::from_millis(self.config.task_idle_timeout_ms);
            let mut executions_to_remove = Vec::new();
            for entry in self.execution_responses.iter() {
                let e = entry.value();
                if !e.is_completed() {
                    continue;
                }
                if let Ok(age) = now.duration_since(e.timestamp) {
                    if age > completed_execution_ttl {
                        executions_to_remove.push(entry.key().clone());
                    }
                }
            }
            for execution_id in executions_to_remove {
                self.execution_responses.remove(&execution_id);
            }

            // Update statistics / 更新统计信息
            {
                let mut stats = self.statistics.write();
                stats.active_artifacts = self.artifacts.len() as u64;
                stats.active_tasks = self.tasks.len() as u64;
                stats.active_instances = self.instances.len() as u64;
            }
        }
    }

    fn task_status_to_sms(&self, task: &Task) -> crate::proto::sms::TaskStatus {
        local_task_status_to_sms(&task.status(), task.instance_count())
    }

    /// Extract error message from RuntimeExecutionError enum / 从RuntimeExecutionError枚举中提取错误消息
    fn extract_error_message(error: &super::runtime::RuntimeExecutionError) -> String {
        use super::runtime::RuntimeExecutionError;
        match error {
            RuntimeExecutionError::InstanceNotFound { instance_id } => {
                format!("Instance not found: {}", instance_id)
            },
            RuntimeExecutionError::InstanceNotReady { instance_id } => {
                format!("Instance not ready: {}", instance_id)
            },
            RuntimeExecutionError::ExecutionTimeout { timeout_ms } => {
                format!("Execution timeout after {} ms", timeout_ms)
            },
            RuntimeExecutionError::ResourceLimitExceeded { resource, limit } => {
                format!("Resource limit exceeded: {} (limit: {})", resource, limit)
            },
            RuntimeExecutionError::ConfigurationError { message } => message.clone(),
            RuntimeExecutionError::RuntimeError { message } => message.clone(),
            RuntimeExecutionError::IoError { message } => message.clone(),
            RuntimeExecutionError::SerializationError { message } => message.clone(),
            RuntimeExecutionError::UnsupportedOperation {
                operation,
                runtime_type,
            } => format!(
                "Unsupported operation: {} for runtime: {}",
                operation, runtime_type
            ),
        }
    }
}

impl Clone for TaskExecutionManager {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            spearlet_config: self.spearlet_config.clone(),
            runtime_manager: self.runtime_manager.clone(),
            scheduler: self.scheduler.clone(),
            artifacts: self.artifacts.clone(),
            tasks: self.tasks.clone(),
            instances: self.instances.clone(),
            desired_task_instances: self.desired_task_instances.clone(),
            task_reconcile_locks: self.task_reconcile_locks.clone(),
            execution_responses: self.execution_responses.clone(),
            active_instance_executions: self.active_instance_executions.clone(),
            execution_semaphore: self.execution_semaphore.clone(),
            statistics: self.statistics.clone(),
            request_counter: AtomicU64::new(self.request_counter.load(Ordering::SeqCst)),
            work_sender: self.work_sender.clone(),
            completion_sender: self.completion_sender.clone(),
            inflight_async_executions: self.inflight_async_executions.clone(),
            sms_channel: self.sms_channel.clone(),
            sms_reporter: self.sms_reporter.clone(),
            shutdown_sender: None, // Clone doesn't get shutdown sender / 克隆不获取关闭发送器
        }
    }
}

#[cfg(test)]
#[path = "manager_tests.rs"]
mod tests;
