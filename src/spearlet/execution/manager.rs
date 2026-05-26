//! Task Execution Manager
//! 任务执行管理器
//!
//! This module provides the central task execution management system that coordinates
//! artifacts, tasks, instances, and runtime execution.
//! 该模块提供中央任务执行管理系统，协调 artifact、任务、实例和运行时执行。

use super::runtime::RuntimeType;
use super::{
    artifact::{Artifact, ArtifactId},
    instance::{InstanceId, InstanceStatus, TaskInstance},
    runtime::{ExecutionCompletionEvent, ExecutionContext, RuntimeManager},
    scheduler::{InstanceScheduler, SchedulingPolicy},
    task::{Task, TaskId},
    ExecutionError, ExecutionResult, DEFAULT_ENTRY_FUNCTION_NAME,
};
use crate::proto::spearlet::{ExecutionMode as ProtoExecutionMode, InvokeRequest};
use crate::spearlet::sms_connector::sms_channel_lazy;
use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::{mpsc, oneshot, Semaphore};
use tokio::time::timeout;
use tonic::transport::Channel;
use tracing::{debug, info, warn};

#[derive(Debug, Clone)]
struct PendingAsyncExecution {
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

#[derive(Debug, Clone, Serialize)]
struct SmsAppendLogLine {
    ts_ms: Option<u64>,
    stream: Option<String>,
    level: Option<String>,
    message: String,
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
    /// Queue size / 队列大小
    pub queue_size: u64,
    /// Running executions / 正在运行的执行数量
    pub running_executions: u64,
    /// Pending executions / 等待执行的数量
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
    /// Execution status storage / 执行状态存储
    executions: Arc<DashMap<String, super::ExecutionResponse>>,
    /// Execution semaphore / 执行信号量
    execution_semaphore: Arc<Semaphore>,
    /// Statistics / 统计信息
    statistics: Arc<RwLock<ExecutionStatistics>>,
    /// Request counter / 请求计数器
    request_counter: AtomicU64,
    /// Execution request sender / 执行请求发送器
    work_sender: mpsc::UnboundedSender<ExecutionWorkItem>,
    completion_sender: mpsc::UnboundedSender<ExecutionCompletionEvent>,
    pending_async_executions: Arc<DashMap<String, PendingAsyncExecution>>,
    sms_channel: Option<Channel>,
    /// Shutdown signal / 关闭信号
    shutdown_sender: Option<oneshot::Sender<()>>,
}

impl TaskExecutionManager {
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
            spearlet_config,
            runtime_manager,
            scheduler,
            artifacts: Arc::new(DashMap::new()),
            tasks: Arc::new(DashMap::new()),
            instances: Arc::new(DashMap::new()),
            executions: Arc::new(DashMap::new()),
            execution_semaphore,
            statistics: Arc::new(RwLock::new(ExecutionStatistics::default())),
            request_counter: AtomicU64::new(0),
            work_sender,
            completion_sender,
            pending_async_executions: Arc::new(DashMap::new()),
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

        self.executions.insert(
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

    /// Get artifact by ID / 根据 ID 获取 artifact
    pub fn get_artifact(&self, artifact_id: &ArtifactId) -> Option<Arc<Artifact>> {
        self.artifacts.get(artifact_id).map(|entry| entry.clone())
    }

    /// Get task by ID / 根据 ID 获取任务
    pub fn get_task(&self, task_id: &TaskId) -> Option<Arc<Task>> {
        self.tasks.get(task_id).map(|entry| entry.clone())
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

    /// Get execution statistics / 获取执行统计信息
    pub fn get_statistics(&self) -> ExecutionStatistics {
        let mut stats = self.statistics.read().clone();

        let mut pending = 0u64;
        let mut running = 0u64;
        for entry in self.executions.iter() {
            match entry.value().status.as_str() {
                "pending" => pending += 1,
                "running" => running += 1,
                _ => {}
            }
        }

        stats.queue_size = pending;
        stats.pending_executions = pending;
        stats.running_executions = running;
        stats
    }

    /// Get execution status by execution ID / 根据执行ID获取执行状态
    pub async fn get_execution_status(
        &self,
        execution_id: &str,
    ) -> ExecutionResult<Option<super::ExecutionResponse>> {
        Ok(self
            .executions
            .get(execution_id)
            .map(|entry| entry.value().clone()))
    }

    pub async fn terminate_execution(
        &self,
        execution_id: &str,
        reason: Option<String>,
    ) -> ExecutionResult<()> {
        if !self.executions.contains_key(execution_id) {
            return Err(ExecutionError::InvalidRequest {
                message: format!("execution not found: {}", execution_id),
            });
        }
        crate::spearlet::execution::host_api::termination::mark_execution_terminated(
            execution_id,
            -libc::ECANCELED,
            reason,
        );
        Ok(())
    }

    pub async fn destroy_instance(
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

        crate::spearlet::execution::host_api::termination::mark_instance_destroyed(
            instance_id,
            -libc::ECANCELED,
            reason.clone(),
        );

        let running_exec_ids: Vec<String> = self
            .executions
            .iter()
            .filter(|e| e.value().instance_id == instance_id && e.value().status == "running")
            .map(|e| e.key().clone())
            .collect();

        for execution_id in running_exec_ids {
            crate::spearlet::execution::host_api::termination::mark_execution_terminated(
                &execution_id,
                -libc::ECANCELED,
                reason
                    .clone()
                    .map(|r| format!("instance destroyed: {}", r))
                    .or_else(|| Some("instance destroyed".to_string())),
            );
        }

        self.stop_instance(&instance).await?;
        self.instances.remove(instance_id);
        Ok(())
    }

    pub fn list_executions(
        &self,
        task_id: Option<&str>,
        invocation_id: Option<&str>,
        limit: usize,
    ) -> Vec<super::ExecutionResponse> {
        let mut items: Vec<super::ExecutionResponse> =
            self.executions.iter().map(|e| e.value().clone()).collect();

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
            if let Err(e) = self.stop_instance(instance).await {
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

        self.executions
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
                self.executions.insert(execution_id.clone(), resp.clone());
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
                    .executions
                    .get(&execution_id)
                    .map(|entry| entry.value().instance_id.clone())
                    .unwrap_or_default();
                self.executions.insert(
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
                    self.publish_task_result(
                        &task_id,
                        "".to_string(),
                        result_status,
                        completed_at,
                        meta,
                    )
                    .await;
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
                    self.publish_task_result(&task_id, "".to_string(), status, completed_at, meta)
                        .await;
                }
            }
            _ => {}
        }

        // Send response / 发送响应
        let _ = request.response_sender.send(result);
    }

    /// Execute an invocation against an existing task / 执行一次对已有 task 的调用
    async fn execute_existing_task_invocation(
        &self,
        invocation_id: String,
        desired_task_id: Option<String>,
        execution_context: ExecutionContext,
    ) -> ExecutionResult<super::ExecutionResponse> {
        let task = if let Some(id) = &desired_task_id {
            if let Some(t) = self.tasks.get(id) {
                t.clone()
            } else {
                self.fetch_and_materialize_task_from_sms(id).await?
            }
        } else {
            return Err(ExecutionError::NotSupported {
                operation: "new_task_invocation_via_execute_request_disabled".to_string(),
            });
        };

        // Get or create instance / 获取或创建实例
        let instance = self.get_or_create_instance(&task).await?;

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

        self.report_instance_to_sms(
            instance.task_id.clone(),
            instance.id().to_string(),
            execution_id.clone(),
            started_at_ms,
            crate::proto::sms::InstanceStatus::Running as i32,
        );
        instance.set_current_execution_id(Some(execution_id.clone()));
        self.report_execution_to_sms(
            invocation_id.clone(),
            instance.task_id.clone(),
            function_name.clone(),
            instance.id().to_string(),
            execution_id.clone(),
            crate::proto::sms::ExecutionStatus::Running as i32,
            started_at_ms,
            0,
            std::collections::HashMap::new(),
        );
        let _ = self
            .append_execution_logs_to_sms(
                &execution_id,
                &mut log_next_seq,
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

        let runtime_response = match runtime.execute(&instance, execution_context).await {
            Ok(r) => r,
            Err(e) => {
                let completed_at_ms = chrono::Utc::now().timestamp_millis();
                let msg = e.to_string();
                let _ = self
                    .append_execution_logs_to_sms(
                        &execution_id,
                        &mut log_next_seq,
                        vec![SmsAppendLogLine {
                            ts_ms: Some(completed_at_ms as u64),
                            stream: Some("system".to_string()),
                            level: Some("error".to_string()),
                            message: format!("execution_failed error={}", msg),
                        }],
                    )
                    .await;

                if instance.config.runtime_type == super::RuntimeType::Wasm {
                    let _ = self
                        .flush_wasm_logs_to_sms(
                            &execution_id,
                            &mut log_next_seq,
                            &mut wasm_last_seq,
                        )
                        .await;
                }
                let _ = self.finalize_execution_logs_to_sms(&execution_id).await;

                let mut meta = std::collections::HashMap::new();
                meta.insert("error_message".to_string(), msg);
                self.report_execution_to_sms(
                    invocation_id.clone(),
                    instance.task_id.clone(),
                    function_name.clone(),
                    instance.id().to_string(),
                    execution_id.clone(),
                    crate::proto::sms::ExecutionStatus::Failed as i32,
                    started_at_ms,
                    completed_at_ms,
                    meta,
                );
                self.report_instance_to_sms(
                    instance.task_id.clone(),
                    instance.id().to_string(),
                    execution_id.clone(),
                    completed_at_ms,
                    crate::proto::sms::InstanceStatus::Running as i32,
                );
                instance.set_current_execution_id(None);
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
        let metadata: std::collections::HashMap<String, String> = runtime_response
            .metadata
            .into_iter()
            .map(|(k, v)| (k, v.to_string()))
            .collect();
        let data = runtime_response.data;

        if is_running {
            self.pending_async_executions.insert(
                execution_id.clone(),
                PendingAsyncExecution {
                    invocation_id: invocation_id.clone(),
                    task_id: instance.task_id.clone(),
                    function_name: function_name.clone(),
                    instance_id: instance.id().to_string(),
                    started_at_ms,
                    log_next_seq,
                    wasm_last_seq: 0,
                },
            );

            return Ok(super::ExecutionResponse {
                execution_id,
                invocation_id,
                task_id: desired_task_id.unwrap_or_else(|| instance.task_id.clone()),
                function_name,
                instance_id: instance.id().to_string(),
                status: "running".to_string(),
                output_data: Vec::new(),
                execution_time_ms: 0,
                error_message: None,
                metadata,
                timestamp: SystemTime::now(),
            });
        }

        let completed_at_ms = chrono::Utc::now().timestamp_millis();
        let (final_status, final_status_str) = if is_successful {
            (
                crate::proto::sms::ExecutionStatus::Completed as i32,
                "completed",
            )
        } else if has_failed {
            (crate::proto::sms::ExecutionStatus::Failed as i32, "failed")
        } else {
            (
                crate::proto::sms::ExecutionStatus::Pending as i32,
                "pending",
            )
        };
        let mut final_meta = metadata.clone();
        final_meta.insert("execution_time_ms".to_string(), duration_ms.to_string());
        if let Some(err) = &error_message {
            final_meta.insert("error_message".to_string(), err.clone());
        }
        self.report_execution_to_sms(
            invocation_id.clone(),
            instance.task_id.clone(),
            function_name.clone(),
            instance.id().to_string(),
            execution_id.clone(),
            final_status,
            started_at_ms,
            completed_at_ms,
            final_meta,
        );
        self.report_instance_to_sms(
            instance.task_id.clone(),
            instance.id().to_string(),
            execution_id.clone(),
            completed_at_ms,
            crate::proto::sms::InstanceStatus::Running as i32,
        );
        instance.set_current_execution_id(None);

        if instance.config.runtime_type == super::RuntimeType::Wasm {
            debug!(
                execution_id = %execution_id,
                invocation_id = %invocation_id,
                instance_id = %instance.id(),
                runtime_execution_status = ?runtime_response.execution_status,
                "manager.wasm.flush.begin"
            );
            let _ = self
                .flush_wasm_logs_to_sms(&execution_id, &mut log_next_seq, &mut wasm_last_seq)
                .await;
        }
        debug!(
            execution_id = %execution_id,
            invocation_id = %invocation_id,
            instance_id = %instance.id(),
            final_status = final_status,
            "manager.logs.finalize.plan"
        );
        let _ = self
            .append_execution_logs_to_sms(
                &execution_id,
                &mut log_next_seq,
                vec![SmsAppendLogLine {
                    ts_ms: Some(completed_at_ms as u64),
                    stream: Some("system".to_string()),
                    level: Some(if is_successful { "info" } else { "warn" }.to_string()),
                    message: format!(
                        "execution_completed status={} duration_ms={}",
                        final_status_str, duration_ms
                    ),
                }],
            )
            .await;
        let _ = self.finalize_execution_logs_to_sms(&execution_id).await;

        Ok(super::ExecutionResponse {
            execution_id,
            invocation_id,
            task_id: desired_task_id.unwrap_or_else(|| instance.task_id.clone()),
            function_name,
            instance_id: instance.id().to_string(),
            output_data: data,
            status: final_status_str.to_string(),
            error_message,
            execution_time_ms: duration_ms,
            metadata,
            timestamp: SystemTime::now(),
        })
    }

    async fn append_execution_logs_to_sms(
        &self,
        execution_id: &str,
        next_seq: &mut u64,
        lines: Vec<SmsAppendLogLine>,
    ) -> ExecutionResult<()> {
        if execution_id.trim().is_empty() || lines.is_empty() {
            return Ok(());
        }
        let channel = match self.sms_channel.clone() {
            Some(c) => c,
            None => return Ok(()),
        };
        let mut client = crate::proto::sms::execution_log_ingest_service_client::ExecutionLogIngestServiceClient::new(channel);

        let mut out_lines = Vec::with_capacity(lines.len());
        for l in lines {
            let seq = (*next_seq).max(1);
            *next_seq = (*next_seq).saturating_add(1);
            out_lines.push(crate::proto::sms::ExecutionLogLine {
                ts_ms: l
                    .ts_ms
                    .unwrap_or_else(|| chrono::Utc::now().timestamp_millis() as u64),
                seq,
                stream: l.stream.unwrap_or_else(|| "stdout".to_string()),
                level: l.level.unwrap_or_else(|| "info".to_string()),
                message: l.message,
            });
        }

        let req = tonic::Request::new(crate::proto::sms::AppendExecutionLogsRequest {
            execution_id: execution_id.to_string(),
            lines: out_lines,
        });
        let per_attempt = Duration::from_millis(self.spearlet_config.sms_connect_timeout_ms)
            .min(Duration::from_secs(5))
            .max(Duration::from_millis(1));
        let resp = timeout(per_attempt, client.append_execution_logs(req))
            .await
            .map_err(|_| ExecutionError::RuntimeError {
                message: "sms append_execution_logs timeout".to_string(),
            })?
            .map_err(|e| ExecutionError::RuntimeError {
                message: e.to_string(),
            })?
            .into_inner();
        if resp.next_seq > 0 {
            *next_seq = (*next_seq).max(resp.next_seq);
        }
        Ok(())
    }

    async fn finalize_execution_logs_to_sms(&self, execution_id: &str) -> ExecutionResult<()> {
        if execution_id.trim().is_empty() {
            return Ok(());
        }
        let channel = match self.sms_channel.clone() {
            Some(c) => c,
            None => return Ok(()),
        };
        let mut client =
            crate::proto::sms::execution_log_ingest_service_client::ExecutionLogIngestServiceClient::new(channel);
        let req = tonic::Request::new(crate::proto::sms::FinalizeExecutionLogsRequest {
            execution_id: execution_id.to_string(),
        });
        let per_attempt = Duration::from_millis(self.spearlet_config.sms_connect_timeout_ms)
            .min(Duration::from_secs(5))
            .max(Duration::from_millis(1));
        let _ = timeout(per_attempt, client.finalize_execution_logs(req)).await;
        Ok(())
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
                    .append_execution_logs_to_sms(
                        execution_id,
                        next_seq,
                        std::mem::take(&mut batch),
                    )
                    .await;
            }
        }
        if !batch.is_empty() {
            let _ = self
                .append_execution_logs_to_sms(execution_id, next_seq, batch)
                .await;
        }
        Ok(())
    }

    pub fn get_artifact_by_id(&self, artifact_id: &str) -> Option<Arc<Artifact>> {
        self.artifacts.get(artifact_id).map(|a| a.clone())
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

    /// Ensure artifact exists from SMS Task / 从 SMS Task 确保 Artifact 存在
    pub async fn ensure_artifact_from_sms(
        &self,
        sms_task: &crate::proto::sms::Task,
    ) -> ExecutionResult<Arc<Artifact>> {
        let (runtime_type, location_opt, checksum_opt, env) = if let Some(ex) = &sms_task.executable
        {
            let rt = match ex.r#type {
                3 => super::RuntimeType::Kubernetes,
                4 => super::RuntimeType::Wasm,
                _ => super::RuntimeType::Process,
            };
            let loc = if ex.uri.is_empty() {
                None
            } else {
                Some(ex.uri.clone())
            };
            let chk = if ex.checksum_sha256.is_empty() {
                None
            } else {
                Some(ex.checksum_sha256.clone())
            };
            (rt, loc, chk, ex.env.clone())
        } else {
            (
                super::RuntimeType::Process,
                None,
                None,
                std::collections::HashMap::new(),
            )
        };

        let artifact_id = if let Some(chk) = &checksum_opt {
            chk.clone()
        } else if let Some(loc) = &location_opt {
            use sha2::Digest;
            let d = sha2::Sha256::digest(loc.as_bytes());
            d.iter().map(|b| format!("{:02x}", b)).collect()
        } else {
            uuid::Uuid::new_v4().to_string()
        };

        use super::artifact::{ArtifactSpec, InvocationType, ResourceLimits};
        let spec = ArtifactSpec {
            name: artifact_id.clone(),
            version: sms_task.version.clone(),
            description: None,
            runtime_type,
            runtime_config: std::collections::HashMap::new(),
            location: location_opt,
            checksum_sha256: checksum_opt,
            environment: env,
            resource_limits: ResourceLimits::default(),
            invocation_type: InvocationType::ExistingTask,
            max_execution_timeout_ms: 30000,
            labels: sms_task.metadata.clone(),
        };
        self.ensure_artifact_with_id(artifact_id, spec)
    }

    /// Task helpers / 任务相关辅助方法
    pub fn get_task_by_id(&self, task_id: &str) -> Option<Arc<Task>> {
        self.tasks.get(task_id).map(|t| t.clone())
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

    /// Ensure task exists from SMS Task using provided artifact / 使用提供的Artifact从 SMS Task 确保 Task 存在
    pub async fn ensure_task_from_sms(
        &self,
        sms_task: &crate::proto::sms::Task,
        artifact: &Arc<Artifact>,
    ) -> ExecutionResult<Arc<Task>> {
        // Convert SMS task model into Spearlet TaskSpec.
        // 将 SMS 的 task 模型转换成 Spearlet 侧的 TaskSpec。
        use super::task::{HealthCheckConfig, ScalingConfig, TaskSpec, TimeoutConfig};
        use std::collections::HashMap;
        let env = if let Some(ex) = &sms_task.executable {
            ex.env.clone()
        } else {
            std::collections::HashMap::new()
        };
        let runtime_type = artifact.spec.runtime_type;
        let task_spec = TaskSpec {
            name: sms_task.name.clone(),
            task_type: super::task::TaskType::HttpHandler,
            runtime_type,
            entry_point: "main".to_string(),
            handler_config: HashMap::new(),
            task_config: sms_task.config.clone(),
            environment: env,
            invocation_type: super::artifact::InvocationType::ExistingTask,
            min_instances: 1,
            max_instances: 10,
            target_concurrency: 100,
            scaling_config: ScalingConfig::default(),
            health_check: HealthCheckConfig::default(),
            timeout_config: TimeoutConfig::default(),
        };
        self.ensure_task_with_id(sms_task.task_id.clone(), artifact, task_spec)
    }

    async fn fetch_and_materialize_task_from_sms(
        &self,
        task_id: &str,
    ) -> ExecutionResult<Arc<Task>> {
        // When an invocation lands on a node that doesn't have the task yet,
        // fetch task metadata from SMS and materialize it locally.
        //
        // 当调用落到一个尚未持有该 task 的节点时，从 SMS 拉取 task 元数据并在本地补齐。
        let channel = self
            .sms_channel
            .clone()
            .ok_or_else(|| ExecutionError::RuntimeError {
                message: "sms_grpc_addr is empty".to_string(),
            })?;
        let deadline =
            Instant::now() + Duration::from_millis(self.spearlet_config.sms_connect_timeout_ms);
        let mut last_err: Option<String> = None;
        let resp = loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break Err(ExecutionError::RuntimeError {
                    message: last_err.unwrap_or_else(|| "connect sms timeout".to_string()),
                });
            }
            let per_attempt = remaining
                .min(Duration::from_secs(5))
                .max(Duration::from_millis(1));
            let mut client =
                crate::proto::sms::task_service_client::TaskServiceClient::new(channel.clone());
            let fut = client.get_task(crate::proto::sms::GetTaskRequest {
                task_id: task_id.to_string(),
            });
            match timeout(per_attempt, fut).await {
                Ok(Ok(r)) => break Ok(r.into_inner()),
                Ok(Err(e)) => last_err = Some(e.to_string()),
                Err(_) => last_err = Some("sms get_task timeout".to_string()),
            }
            tokio::time::sleep(Duration::from_millis(
                self.spearlet_config.sms_connect_retry_ms,
            ))
            .await;
        }?;
        // Query SMS for the task definition.
        // 向 SMS 查询 task 定义。
        if !resp.found {
            return Err(ExecutionError::TaskNotFound {
                id: task_id.to_string(),
            });
        }
        let sms_task = resp.task.ok_or_else(|| ExecutionError::TaskNotFound {
            id: task_id.to_string(),
        })?;
        // Ensure artifact/task are present locally before execution.
        // 执行前确保本地已有 artifact/task。
        let artifact = self.ensure_artifact_from_sms(&sms_task).await?;
        self.ensure_task_from_sms(&sms_task, &artifact).await
    }

    /// Get or create instance / 获取或创建实例
    async fn get_or_create_instance(&self, task: &Arc<Task>) -> ExecutionResult<Arc<TaskInstance>> {
        // Try to find an available instance / 尝试找到可用实例
        if let Some(instance) = self.scheduler.select_instance(task).await? {
            return Ok(instance);
        }

        // Check instance limit / 检查实例限制
        if task.instance_count() >= self.config.max_instances_per_task {
            return Err(ExecutionError::ResourceExhausted {
                message: format!(
                    "Maximum instances per task limit reached: {}",
                    self.config.max_instances_per_task
                ),
            });
        }

        // Create new instance / 创建新实例
        let instance_id = task.generate_instance_id();
        let runtime = self
            .runtime_manager
            .get_runtime(&task.spec.runtime_type)
            .ok_or_else(|| ExecutionError::RuntimeError {
                message: format!("Runtime not found for type: {:?}", task.spec.runtime_type),
            })?;

        let mut instance_config = task.create_instance_config();
        // Inject ArtifactSnapshot into InstanceConfig / 在实例配置中注入 ArtifactSnapshot
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
        let instance = timeout(
            Duration::from_millis(self.config.instance_creation_timeout_ms),
            runtime.create_instance(&instance_config),
        )
        .await
        .map_err(|_| ExecutionError::ExecutionTimeout {
            timeout_ms: self.config.instance_creation_timeout_ms,
        })??;

        // Start the instance / 启动实例
        runtime.start_instance(&instance).await?;

        // Register instance / 注册实例
        self.instances
            .insert(instance.id().to_string(), instance.clone());
        task.add_instance(instance.clone())?;
        self.scheduler.add_instance(instance.clone()).await?;

        // Report ACTIVE status / 上报ACTIVE状态
        self.publish_task_status(
            task.id(),
            crate::proto::sms::TaskStatus::Active,
            Some("instance initialized".to_string()),
        )
        .await;

        // Update statistics / 更新统计信息
        {
            let mut stats = self.statistics.write();
            stats.active_instances = self.instances.len() as u64;
        }

        info!("Created new instance: {}", instance_id);
        Ok(instance)
    }

    /// Stop instance / 停止实例
    async fn stop_instance(&self, instance: &Arc<TaskInstance>) -> ExecutionResult<()> {
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
        self.report_instance_to_sms(
            instance.task_id().to_string(),
            instance.id().to_string(),
            current_execution_id,
            ts_ms,
            crate::proto::sms::InstanceStatus::Terminated as i32,
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
                self.publish_task_status(
                    &task_id,
                    crate::proto::sms::TaskStatus::Inactive,
                    Some("no instances".to_string()),
                )
                .await;
            }
        }

        // Update statistics / 更新统计信息
        {
            let mut stats = self.statistics.write();
            stats.active_instances = self.instances.len() as u64;
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
            let _ = self.handle_async_completion(ev).await;
        }
    }

    async fn handle_async_completion(&self, ev: ExecutionCompletionEvent) -> ExecutionResult<()> {
        let Some((_, pending)) = self.pending_async_executions.remove(&ev.execution_id) else {
            return Ok(());
        };

        crate::spearlet::execution::host_api::user_stream::map_ws_close_to_channels(
            &ev.execution_id,
        );

        if let Some(inst) = self.instances.get(&pending.instance_id) {
            if inst.value().config.runtime_type == super::RuntimeType::Wasm {
                if let Some(wasm_handle) = inst
                    .value()
                    .get_runtime_handle::<crate::spearlet::execution::runtime::wasm::WasmInstanceHandle>(
                    )
                {
                    let mut st = wasm_handle.state.lock().await;
                    st.is_running = false;
                    st.current_function = None;
                }
            }
            inst.value().set_current_execution_id(None);
        }

        let completed_at_ms = ev.completed_at_ms;
        let mut log_next_seq = pending.log_next_seq;
        let mut wasm_last_seq = pending.wasm_last_seq;
        if self
            .instances
            .get(&pending.instance_id)
            .map(|inst| inst.value().config.runtime_type == super::RuntimeType::Wasm)
            .unwrap_or(false)
        {
            let _ = self
                .flush_wasm_logs_to_sms(&ev.execution_id, &mut log_next_seq, &mut wasm_last_seq)
                .await;
        }

        let is_successful = matches!(
            ev.execution_status,
            crate::spearlet::execution::runtime::ExecutionStatus::Completed
        );
        let has_failed = matches!(
            ev.execution_status,
            crate::spearlet::execution::runtime::ExecutionStatus::Failed
        );

        if let Some(inst) = self.instances.get(&pending.instance_id) {
            inst.value()
                .record_request_completion(is_successful, ev.duration_ms as f64);
        }

        let _ = self
            .append_execution_logs_to_sms(
                &ev.execution_id,
                &mut log_next_seq,
                vec![SmsAppendLogLine {
                    ts_ms: Some(completed_at_ms as u64),
                    stream: Some("system".to_string()),
                    level: Some(if is_successful { "info" } else { "warn" }.to_string()),
                    message: format!(
                        "execution_completed status={} duration_ms={}",
                        if is_successful {
                            "completed"
                        } else if has_failed {
                            "failed"
                        } else {
                            "pending"
                        },
                        ev.duration_ms
                    ),
                }],
            )
            .await;
        let _ = self.finalize_execution_logs_to_sms(&ev.execution_id).await;

        let final_status = if is_successful {
            crate::proto::sms::ExecutionStatus::Completed as i32
        } else if has_failed {
            crate::proto::sms::ExecutionStatus::Failed as i32
        } else {
            crate::proto::sms::ExecutionStatus::Pending as i32
        };

        let mut meta: std::collections::HashMap<String, String> = ev
            .runtime_metadata
            .into_iter()
            .map(|(k, v)| (k, v.to_string()))
            .collect();
        meta.insert("execution_time_ms".to_string(), ev.duration_ms.to_string());
        if let Some(err) = ev.error_message.as_ref() {
            meta.insert("error_message".to_string(), err.clone());
        }

        self.report_execution_to_sms(
            pending.invocation_id.clone(),
            pending.task_id.clone(),
            pending.function_name.clone(),
            pending.instance_id.clone(),
            ev.execution_id.clone(),
            final_status,
            pending.started_at_ms,
            completed_at_ms,
            meta.clone(),
        );
        self.report_instance_to_sms(
            pending.task_id.clone(),
            pending.instance_id.clone(),
            ev.execution_id.clone(),
            completed_at_ms,
            crate::proto::sms::InstanceStatus::Running as i32,
        );
        crate::spearlet::execution::host_api::clear_wasm_logs_by_execution(&ev.execution_id);

        self.executions.insert(
            ev.execution_id.clone(),
            super::ExecutionResponse {
                execution_id: ev.execution_id.clone(),
                invocation_id: pending.invocation_id,
                task_id: pending.task_id,
                function_name: pending.function_name,
                instance_id: pending.instance_id,
                output_data: ev.output,
                status: if is_successful {
                    "completed".to_string()
                } else if has_failed {
                    "failed".to_string()
                } else {
                    "pending".to_string()
                },
                error_message: ev.error_message,
                execution_time_ms: ev.duration_ms,
                metadata: meta,
                timestamp: SystemTime::now(),
            },
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
                            let _ = self.stop_instance(&instance).await;
                        }
                    }
                }
            }
        }
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
                self.report_instance_to_sms(
                    instance.task_id.clone(),
                    instance.id().to_string(),
                    current_execution_id,
                    ts_ms,
                    crate::proto::sms::InstanceStatus::Running as i32,
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
                if let Err(e) = self.stop_instance(&instance).await {
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
                    let not_active = !matches!(
                        task.status(),
                        super::task::TaskStatus::Ready | super::task::TaskStatus::Running
                    );
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
                    self.publish_task_status(
                        task.id(),
                        crate::proto::sms::TaskStatus::Inactive,
                        Some("cleanup".to_string()),
                    )
                    .await;
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
            for entry in self.executions.iter() {
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
                self.executions.remove(&execution_id);
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

    fn report_instance_to_sms(
        &self,
        task_id: String,
        instance_id: String,
        current_execution_id: String,
        ts_ms: i64,
        status: i32,
    ) {
        let channel = match self.sms_channel.clone() {
            Some(c) => c,
            None => return,
        };
        let per_attempt = Duration::from_millis(self.spearlet_config.sms_connect_timeout_ms)
            .min(Duration::from_secs(5))
            .max(Duration::from_millis(1));
        let node_uuid = self.spearlet_config.compute_node_uuid();
        let inst = crate::proto::sms::Instance {
            instance_id,
            task_id,
            node_uuid,
            status,
            created_at_ms: ts_ms,
            updated_at_ms: ts_ms,
            last_seen_ms: ts_ms,
            current_execution_id,
            metadata: std::collections::HashMap::new(),
        };
        tokio::spawn(async move {
            let mut client =
                crate::proto::sms::instance_registry_service_client::InstanceRegistryServiceClient::new(
                    channel,
                );
            let _ = timeout(per_attempt, client.report_instance(inst)).await;
        });
    }

    fn report_execution_to_sms(
        &self,
        invocation_id: String,
        task_id: String,
        function_name: String,
        instance_id: String,
        execution_id: String,
        status: i32,
        started_at_ms: i64,
        completed_at_ms: i64,
        metadata: std::collections::HashMap<String, String>,
    ) {
        let channel = match self.sms_channel.clone() {
            Some(c) => c,
            None => return,
        };
        let per_attempt = Duration::from_millis(self.spearlet_config.sms_connect_timeout_ms)
            .min(Duration::from_secs(5))
            .max(Duration::from_millis(1));
        let node_uuid = self.spearlet_config.compute_node_uuid();
        let updated_at_ms = if completed_at_ms > 0 {
            completed_at_ms
        } else {
            started_at_ms
        };
        let exe = crate::proto::sms::Execution {
            execution_id,
            invocation_id,
            task_id,
            function_name,
            node_uuid,
            instance_id,
            status,
            started_at_ms,
            completed_at_ms,
            log_ref: None,
            metadata,
            updated_at_ms,
        };
        tokio::spawn(async move {
            let mut client =
                crate::proto::sms::execution_registry_service_client::ExecutionRegistryServiceClient::new(
                    channel,
                );
            let _ = timeout(per_attempt, client.report_execution(exe)).await;
        });
    }

    async fn publish_task_status(
        &self,
        task_id: &str,
        status: crate::proto::sms::TaskStatus,
        reason: Option<String>,
    ) {
        let channel = match self.sms_channel.clone() {
            Some(c) => c,
            None => return,
        };
        let per_attempt = Duration::from_millis(self.spearlet_config.sms_connect_timeout_ms)
            .min(Duration::from_secs(5))
            .max(Duration::from_millis(1));
        let node_uuid = {
            let cfg = &self.spearlet_config;
            let base = format!(
                "{}:{}:{}",
                cfg.grpc.addr.ip(),
                cfg.grpc.addr.port(),
                cfg.node_name
            );
            uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, base.as_bytes()).to_string()
        };
        let req = crate::proto::sms::UpdateTaskStatusRequest {
            task_id: task_id.to_string(),
            status: status as i32,
            node_uuid,
            status_version: 0,
            updated_at: chrono::Utc::now().timestamp(),
            reason: reason.unwrap_or_default(),
        };
        tokio::spawn(async move {
            let mut client =
                crate::proto::sms::task_service_client::TaskServiceClient::new(channel);
            let _ = timeout(per_attempt, client.update_task_status(req)).await;
        });
    }

    async fn publish_task_result(
        &self,
        task_id: &str,
        result_uri: String,
        result_status: String,
        completed_at: i64,
        result_metadata: std::collections::HashMap<String, String>,
    ) {
        let channel = match self.sms_channel.clone() {
            Some(c) => c,
            None => return,
        };
        let per_attempt = Duration::from_millis(self.spearlet_config.sms_connect_timeout_ms)
            .min(Duration::from_secs(5))
            .max(Duration::from_millis(1));
        let req = crate::proto::sms::UpdateTaskResultRequest {
            task_id: task_id.to_string(),
            result_uri,
            result_status,
            completed_at,
            result_metadata,
        };
        tokio::spawn(async move {
            let mut client =
                crate::proto::sms::task_service_client::TaskServiceClient::new(channel);
            let _ = timeout(per_attempt, client.update_task_result(req)).await;
        });
    }

    /// Extract error message from RuntimeExecutionError enum / 从RuntimeExecutionError枚举中提取错误消息
    fn extract_error_message(error: &super::runtime::RuntimeExecutionError) -> String {
        use super::runtime::RuntimeExecutionError;
        match error {
            RuntimeExecutionError::InstanceNotFound { instance_id } => {
                format!("Instance not found: {}", instance_id)
            }
            RuntimeExecutionError::InstanceNotReady { instance_id } => {
                format!("Instance not ready: {}", instance_id)
            }
            RuntimeExecutionError::ExecutionTimeout { timeout_ms } => {
                format!("Execution timeout after {} ms", timeout_ms)
            }
            RuntimeExecutionError::ResourceLimitExceeded { resource, limit } => {
                format!("Resource limit exceeded: {} (limit: {})", resource, limit)
            }
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
            executions: self.executions.clone(),
            execution_semaphore: self.execution_semaphore.clone(),
            statistics: self.statistics.clone(),
            request_counter: AtomicU64::new(self.request_counter.load(Ordering::SeqCst)),
            work_sender: self.work_sender.clone(),
            completion_sender: self.completion_sender.clone(),
            pending_async_executions: self.pending_async_executions.clone(),
            sms_channel: self.sms_channel.clone(),
            shutdown_sender: None, // Clone doesn't get shutdown sender / 克隆不获取关闭发送器
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spearlet::execution::instance;
    use crate::spearlet::execution::runtime;
    use crate::spearlet::execution::runtime::{Runtime, RuntimeCapabilities, RuntimeType};
    use async_trait::async_trait;
    use std::collections::HashMap as StdHashMap;
    use tokio::time::sleep;

    struct DummyRuntime {
        ty: RuntimeType,
    }

    #[async_trait]
    impl Runtime for DummyRuntime {
        fn runtime_type(&self) -> RuntimeType {
            self.ty
        }
        async fn create_instance(
            &self,
            config: &instance::InstanceConfig,
        ) -> super::ExecutionResult<Arc<instance::TaskInstance>> {
            Ok(Arc::new(instance::TaskInstance::new(
                config.task_id.clone(),
                config.clone(),
            )))
        }
        async fn start_instance(
            &self,
            _instance: &Arc<instance::TaskInstance>,
        ) -> super::ExecutionResult<()> {
            Ok(())
        }
        async fn stop_instance(
            &self,
            _instance: &Arc<instance::TaskInstance>,
        ) -> super::ExecutionResult<()> {
            Ok(())
        }
        async fn execute(
            &self,
            _instance: &Arc<instance::TaskInstance>,
            _context: runtime::ExecutionContext,
        ) -> super::ExecutionResult<runtime::RuntimeExecutionResponse> {
            Ok(runtime::RuntimeExecutionResponse::default())
        }
        async fn health_check(
            &self,
            _instance: &Arc<instance::TaskInstance>,
        ) -> super::ExecutionResult<bool> {
            Ok(true)
        }
        async fn get_metrics(
            &self,
            _instance: &Arc<instance::TaskInstance>,
        ) -> super::ExecutionResult<StdHashMap<String, serde_json::Value>> {
            Ok(StdHashMap::new())
        }
        async fn scale_instance(
            &self,
            _instance: &Arc<instance::TaskInstance>,
            _new_limits: &instance::InstanceResourceLimits,
        ) -> super::ExecutionResult<()> {
            Ok(())
        }
        async fn cleanup_instance(
            &self,
            _instance: &Arc<instance::TaskInstance>,
        ) -> super::ExecutionResult<()> {
            Ok(())
        }
        fn validate_config(
            &self,
            _config: &instance::InstanceConfig,
        ) -> super::ExecutionResult<()> {
            Ok(())
        }
        fn get_capabilities(&self) -> RuntimeCapabilities {
            RuntimeCapabilities::default()
        }
    }

    struct DelayedRuntime {
        ty: RuntimeType,
        delay_ms: u64,
        payload: Vec<u8>,
    }

    #[async_trait]
    impl Runtime for DelayedRuntime {
        fn runtime_type(&self) -> RuntimeType {
            self.ty
        }
        async fn create_instance(
            &self,
            config: &instance::InstanceConfig,
        ) -> super::ExecutionResult<Arc<instance::TaskInstance>> {
            Ok(Arc::new(instance::TaskInstance::new(
                config.task_id.clone(),
                config.clone(),
            )))
        }
        async fn start_instance(
            &self,
            _instance: &Arc<instance::TaskInstance>,
        ) -> super::ExecutionResult<()> {
            Ok(())
        }
        async fn stop_instance(
            &self,
            _instance: &Arc<instance::TaskInstance>,
        ) -> super::ExecutionResult<()> {
            Ok(())
        }
        async fn execute(
            &self,
            _instance: &Arc<instance::TaskInstance>,
            context: runtime::ExecutionContext,
        ) -> super::ExecutionResult<runtime::RuntimeExecutionResponse> {
            sleep(Duration::from_millis(self.delay_ms)).await;
            Ok(runtime::RuntimeExecutionResponse::new_sync(
                context.execution_id,
                self.payload.clone(),
                self.delay_ms,
            ))
        }
        async fn health_check(
            &self,
            _instance: &Arc<instance::TaskInstance>,
        ) -> super::ExecutionResult<bool> {
            Ok(true)
        }
        async fn get_metrics(
            &self,
            _instance: &Arc<instance::TaskInstance>,
        ) -> super::ExecutionResult<StdHashMap<String, serde_json::Value>> {
            Ok(StdHashMap::new())
        }
        async fn scale_instance(
            &self,
            _instance: &Arc<instance::TaskInstance>,
            _new_limits: &instance::InstanceResourceLimits,
        ) -> super::ExecutionResult<()> {
            Ok(())
        }
        async fn cleanup_instance(
            &self,
            _instance: &Arc<instance::TaskInstance>,
        ) -> super::ExecutionResult<()> {
            Ok(())
        }
        fn validate_config(
            &self,
            _config: &instance::InstanceConfig,
        ) -> super::ExecutionResult<()> {
            Ok(())
        }
        fn get_capabilities(&self) -> RuntimeCapabilities {
            RuntimeCapabilities::default()
        }
    }

    #[tokio::test]
    async fn test_task_execution_manager_creation() {
        let config = TaskExecutionManagerConfig::default();
        let runtime_manager = Arc::new(RuntimeManager::new());

        let manager = TaskExecutionManager::new(
            config,
            runtime_manager,
            Arc::new(crate::spearlet::config::SpearletConfig::default()),
            None,
        )
        .await;
        assert!(manager.is_ok());
    }

    #[tokio::test]
    async fn test_execution_statistics() {
        let mut stats = ExecutionStatistics::default();
        assert_eq!(stats.total_executions, 0);
        assert_eq!(stats.successful_executions, 0);
        assert_eq!(stats.failed_executions, 0);

        stats.total_executions = 10;
        stats.successful_executions = 8;
        stats.failed_executions = 2;
        stats.total_execution_time_ms = 5000;
        stats.average_execution_time_ms = 500.0;

        assert_eq!(stats.total_executions, 10);
        assert_eq!(stats.successful_executions, 8);
        assert_eq!(stats.failed_executions, 2);
    }

    #[test]
    fn test_task_execution_manager_config() {
        let config = TaskExecutionManagerConfig::default();
        assert_eq!(config.max_concurrent_executions, 1000);
        assert_eq!(config.max_artifacts, 100);
        assert_eq!(config.max_tasks_per_artifact, 10);
        assert_eq!(config.max_instances_per_task, 50);
    }

    #[tokio::test]
    async fn test_execution_status_tracking() {
        let mut rm = RuntimeManager::new();
        rm.register_runtime(
            RuntimeType::Process,
            Box::new(DelayedRuntime {
                ty: RuntimeType::Process,
                delay_ms: 200,
                payload: b"ok".to_vec(),
            }),
        )
        .unwrap();
        let rm = Arc::new(rm);

        let cfg = TaskExecutionManagerConfig {
            max_concurrent_executions: 1,
            ..Default::default()
        };
        let manager = TaskExecutionManager::new(
            cfg,
            rm,
            Arc::new(crate::spearlet::config::SpearletConfig::default()),
            None,
        )
        .await
        .unwrap();

        let spec_local = crate::spearlet::execution::artifact::ArtifactSpec {
            name: "artifact-long".to_string(),
            version: "1.0.0".to_string(),
            description: None,
            runtime_type: RuntimeType::Process,
            runtime_config: StdHashMap::new(),
            location: None,
            checksum_sha256: None,
            environment: StdHashMap::new(),
            resource_limits: Default::default(),
            invocation_type: crate::spearlet::execution::artifact::InvocationType::ExistingTask,
            max_execution_timeout_ms: 30000,
            labels: StdHashMap::new(),
        };
        let artifact = manager
            .ensure_artifact_with_id("artifact-long".to_string(), spec_local)
            .unwrap();

        use crate::spearlet::execution::task::{
            HealthCheckConfig, ScalingConfig, TaskSpec, TaskType, TimeoutConfig,
        };
        let task_spec = TaskSpec {
            name: "task-long".to_string(),
            task_type: TaskType::HttpHandler,
            runtime_type: artifact.spec.runtime_type,
            entry_point: "main".to_string(),
            handler_config: StdHashMap::new(),
            task_config: StdHashMap::new(),
            environment: artifact.spec.environment.clone(),
            invocation_type: artifact.spec.invocation_type.clone(),
            min_instances: 1,
            max_instances: 10,
            target_concurrency: 100,
            scaling_config: ScalingConfig::default(),
            health_check: HealthCheckConfig::default(),
            timeout_config: TimeoutConfig::default(),
        };
        manager
            .ensure_task_with_id("task-long".to_string(), &artifact, task_spec)
            .unwrap();

        let req = crate::proto::spearlet::InvokeRequest {
            invocation_id: "inv-long-1".to_string(),
            execution_id: "exec-long-1".to_string(),
            task_id: "task-long".to_string(),
            function_name: crate::spearlet::execution::DEFAULT_ENTRY_FUNCTION_NAME.to_string(),
            input: Some(crate::proto::spearlet::Payload {
                content_type: "application/octet-stream".to_string(),
                data: Vec::new(),
            }),
            headers: StdHashMap::new(),
            environment: StdHashMap::new(),
            timeout_ms: 0,
            session_id: String::new(),
            mode: crate::proto::spearlet::ExecutionMode::Async as i32,
            force_new_instance: false,
            metadata: StdHashMap::new(),
        };

        let mgr2 = manager.clone();
        let h = tokio::spawn(async move { mgr2.submit_invocation(req).await });

        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if let Ok(Some(s)) = manager.get_execution_status("exec-long-1").await {
                    if s.status == "pending" || s.status == "running" {
                        break;
                    }
                }
                sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();

        let final_resp = h.await.unwrap().unwrap();
        assert_eq!(final_resp.execution_id, "exec-long-1");
        assert_eq!(final_resp.status, "completed");
        assert_eq!(final_resp.output_data, b"ok".to_vec());

        let stored = manager
            .get_execution_status("exec-long-1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.status, "completed");
    }

    #[tokio::test]
    async fn test_stop_instance_removes_from_task_and_manager() {
        let mut rm = RuntimeManager::new();
        rm.register_runtime(
            RuntimeType::Process,
            Box::new(DummyRuntime {
                ty: RuntimeType::Process,
            }),
        )
        .unwrap();
        let rm = Arc::new(rm);

        let config = TaskExecutionManagerConfig::default();
        let manager = TaskExecutionManager::new(
            config,
            rm,
            Arc::new(crate::spearlet::config::SpearletConfig::default()),
            None,
        )
        .await
        .unwrap();

        let spec_local = crate::spearlet::execution::artifact::ArtifactSpec {
            name: "artifact-test".to_string(),
            version: "1.0.0".to_string(),
            description: None,
            runtime_type: RuntimeType::Process,
            runtime_config: StdHashMap::new(),
            location: None,
            checksum_sha256: None,
            environment: StdHashMap::new(),
            resource_limits: Default::default(),
            invocation_type: crate::spearlet::execution::artifact::InvocationType::ExistingTask,
            max_execution_timeout_ms: 30000,
            labels: StdHashMap::new(),
        };
        let artifact = manager
            .ensure_artifact_with_id("artifact-test".to_string(), spec_local)
            .unwrap();
        use crate::spearlet::execution::task::{
            HealthCheckConfig, ScalingConfig, TaskSpec, TaskType, TimeoutConfig,
        };
        let task_spec = TaskSpec {
            name: "task-test".to_string(),
            task_type: TaskType::HttpHandler,
            runtime_type: artifact.spec.runtime_type,
            entry_point: "main".to_string(),
            handler_config: StdHashMap::new(),
            task_config: StdHashMap::new(),
            environment: artifact.spec.environment.clone(),
            invocation_type: artifact.spec.invocation_type.clone(),
            min_instances: 1,
            max_instances: 10,
            target_concurrency: 100,
            scaling_config: ScalingConfig::default(),
            health_check: HealthCheckConfig::default(),
            timeout_config: TimeoutConfig::default(),
        };
        let task = manager
            .ensure_task_with_id("task-test".to_string(), &artifact, task_spec)
            .unwrap();
        let instance = manager.get_or_create_instance(&task).await.unwrap();

        assert_eq!(task.instance_count(), 1);
        assert!(manager.get_instance(&instance.id().to_string()).is_some());

        manager.stop_instance(&instance).await.unwrap();

        assert_eq!(task.instance_count(), 0);
        assert!(task.get_instance(instance.id()).is_none());
        assert!(manager.get_instance(&instance.id().to_string()).is_none());
    }

    #[tokio::test]
    async fn test_cleanup_loop_removes_task_from_artifact() {
        let mut rm = RuntimeManager::new();
        rm.register_runtime(
            RuntimeType::Process,
            Box::new(DummyRuntime {
                ty: RuntimeType::Process,
            }),
        )
        .unwrap();
        let rm = Arc::new(rm);

        let cfg = TaskExecutionManagerConfig {
            cleanup_interval_ms: 10,
            task_idle_timeout_ms: 1,
            ..Default::default()
        };

        let manager = TaskExecutionManager::new(
            cfg,
            rm,
            Arc::new(crate::spearlet::config::SpearletConfig::default()),
            None,
        )
        .await
        .unwrap();

        let spec_local = crate::spearlet::execution::artifact::ArtifactSpec {
            name: "artifact-cleanup".to_string(),
            version: "1.0.0".to_string(),
            description: None,
            runtime_type: RuntimeType::Process,
            runtime_config: StdHashMap::new(),
            location: None,
            checksum_sha256: None,
            environment: StdHashMap::new(),
            resource_limits: Default::default(),
            invocation_type: crate::spearlet::execution::artifact::InvocationType::ExistingTask,
            max_execution_timeout_ms: 30000,
            labels: StdHashMap::new(),
        };
        let artifact = manager
            .ensure_artifact_with_id("artifact-cleanup".to_string(), spec_local)
            .unwrap();
        use crate::spearlet::execution::task::{
            HealthCheckConfig, ScalingConfig, TaskSpec, TaskType, TimeoutConfig,
        };
        let task_spec = TaskSpec {
            name: "task-cleanup".to_string(),
            task_type: TaskType::HttpHandler,
            runtime_type: artifact.spec.runtime_type,
            entry_point: "main".to_string(),
            handler_config: StdHashMap::new(),
            task_config: StdHashMap::new(),
            environment: artifact.spec.environment.clone(),
            invocation_type: artifact.spec.invocation_type.clone(),
            min_instances: 1,
            max_instances: 10,
            target_concurrency: 100,
            scaling_config: ScalingConfig::default(),
            health_check: HealthCheckConfig::default(),
            timeout_config: TimeoutConfig::default(),
        };
        let task = manager
            .ensure_task_with_id("task-cleanup".to_string(), &artifact, task_spec)
            .unwrap();
        let task_id = task.id().to_string();

        let handle = {
            let m = manager.clone();
            tokio::spawn(async move { m.run_cleanup_loop().await })
        };

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        handle.abort();

        assert!(manager.get_task(&task_id).is_none());
        assert!(artifact.get_task(&task_id).is_none());
    }

    #[tokio::test]
    async fn test_get_or_create_task_uses_desired_task_id() {
        let mut rm = RuntimeManager::new();
        rm.register_runtime(
            RuntimeType::Process,
            Box::new(DummyRuntime {
                ty: RuntimeType::Process,
            }),
        )
        .unwrap();
        let rm = Arc::new(rm);

        let manager = TaskExecutionManager::new(
            TaskExecutionManagerConfig::default(),
            rm,
            Arc::new(crate::spearlet::config::SpearletConfig::default()),
            None,
        )
        .await
        .unwrap();

        let spec_local = crate::spearlet::execution::artifact::ArtifactSpec {
            name: "artifact-fixed".to_string(),
            version: "1.0.0".to_string(),
            description: None,
            runtime_type: RuntimeType::Process,
            runtime_config: StdHashMap::new(),
            location: Some("file:///bin/foo".to_string()),
            checksum_sha256: None,
            environment: StdHashMap::new(),
            resource_limits: Default::default(),
            invocation_type: crate::spearlet::execution::artifact::InvocationType::ExistingTask,
            max_execution_timeout_ms: 30000,
            labels: StdHashMap::new(),
        };
        let artifact = manager
            .ensure_artifact_with_id("artifact-fixed".to_string(), spec_local)
            .unwrap();
        let desired = "sms-task-123".to_string();
        use crate::spearlet::execution::task::{
            HealthCheckConfig, ScalingConfig, TaskSpec, TaskType, TimeoutConfig,
        };
        let task_spec = TaskSpec {
            name: desired.clone(),
            task_type: TaskType::HttpHandler,
            runtime_type: artifact.spec.runtime_type,
            entry_point: "main".to_string(),
            handler_config: StdHashMap::new(),
            task_config: StdHashMap::new(),
            environment: artifact.spec.environment.clone(),
            invocation_type: artifact.spec.invocation_type.clone(),
            min_instances: 1,
            max_instances: 10,
            target_concurrency: 100,
            scaling_config: ScalingConfig::default(),
            health_check: HealthCheckConfig::default(),
            timeout_config: TimeoutConfig::default(),
        };
        let task = manager
            .ensure_task_with_id(desired.clone(), &artifact, task_spec)
            .unwrap();
        assert_eq!(task.id(), desired);
        assert!(manager.get_task(&desired).is_some());
    }

    #[tokio::test]
    async fn test_health_check_failure_triggers_cascade_removal() {
        use std::sync::atomic::{AtomicBool, Ordering};

        struct FailingRuntime {
            ty: RuntimeType,
            fail: Arc<AtomicBool>,
        }

        #[async_trait]
        impl Runtime for FailingRuntime {
            fn runtime_type(&self) -> RuntimeType {
                self.ty
            }
            async fn create_instance(
                &self,
                config: &instance::InstanceConfig,
            ) -> super::ExecutionResult<Arc<instance::TaskInstance>> {
                Ok(Arc::new(instance::TaskInstance::new(
                    config.task_id.clone(),
                    config.clone(),
                )))
            }
            async fn start_instance(
                &self,
                _instance: &Arc<instance::TaskInstance>,
            ) -> super::ExecutionResult<()> {
                Ok(())
            }
            async fn stop_instance(
                &self,
                _instance: &Arc<instance::TaskInstance>,
            ) -> super::ExecutionResult<()> {
                Ok(())
            }
            async fn execute(
                &self,
                _instance: &Arc<instance::TaskInstance>,
                _context: runtime::ExecutionContext,
            ) -> super::ExecutionResult<runtime::RuntimeExecutionResponse> {
                Ok(runtime::RuntimeExecutionResponse::default())
            }
            async fn health_check(
                &self,
                _instance: &Arc<instance::TaskInstance>,
            ) -> super::ExecutionResult<bool> {
                if self.fail.load(Ordering::SeqCst) {
                    Err(super::ExecutionError::HealthCheckFailed {
                        message: "fail".to_string(),
                    })
                } else {
                    Ok(true)
                }
            }
            async fn get_metrics(
                &self,
                _instance: &Arc<instance::TaskInstance>,
            ) -> super::ExecutionResult<StdHashMap<String, serde_json::Value>> {
                Ok(StdHashMap::new())
            }
            async fn scale_instance(
                &self,
                _instance: &Arc<instance::TaskInstance>,
                _new_limits: &instance::InstanceResourceLimits,
            ) -> super::ExecutionResult<()> {
                Ok(())
            }
            async fn cleanup_instance(
                &self,
                _instance: &Arc<instance::TaskInstance>,
            ) -> super::ExecutionResult<()> {
                Ok(())
            }
            fn validate_config(
                &self,
                _config: &instance::InstanceConfig,
            ) -> super::ExecutionResult<()> {
                Ok(())
            }
            fn get_capabilities(&self) -> RuntimeCapabilities {
                RuntimeCapabilities::default()
            }
        }

        let mut rm = RuntimeManager::new();
        let fail_flag = Arc::new(AtomicBool::new(false));
        rm.register_runtime(
            RuntimeType::Process,
            Box::new(FailingRuntime {
                ty: RuntimeType::Process,
                fail: fail_flag.clone(),
            }),
        )
        .unwrap();
        let rm = Arc::new(rm);

        let cfg = TaskExecutionManagerConfig {
            health_check_interval_ms: 10,
            ..Default::default()
        };

        let manager = TaskExecutionManager::new(
            cfg,
            rm,
            Arc::new(crate::spearlet::config::SpearletConfig::default()),
            None,
        )
        .await
        .unwrap();

        let spec_local = crate::spearlet::execution::artifact::ArtifactSpec {
            name: "artifact-hc".to_string(),
            version: "1.0.0".to_string(),
            description: None,
            runtime_type: RuntimeType::Process,
            runtime_config: StdHashMap::new(),
            location: None,
            checksum_sha256: None,
            environment: StdHashMap::new(),
            resource_limits: Default::default(),
            invocation_type: crate::spearlet::execution::artifact::InvocationType::ExistingTask,
            max_execution_timeout_ms: 30000,
            labels: StdHashMap::new(),
        };
        let artifact = manager
            .ensure_artifact_with_id("artifact-hc".to_string(), spec_local)
            .unwrap();
        use crate::spearlet::execution::task::{
            HealthCheckConfig, ScalingConfig, TaskSpec, TaskType, TimeoutConfig,
        };
        let task_spec = TaskSpec {
            name: "task-hc".to_string(),
            task_type: TaskType::HttpHandler,
            runtime_type: artifact.spec.runtime_type,
            entry_point: "main".to_string(),
            handler_config: StdHashMap::new(),
            task_config: StdHashMap::new(),
            environment: artifact.spec.environment.clone(),
            invocation_type: artifact.spec.invocation_type.clone(),
            min_instances: 1,
            max_instances: 10,
            target_concurrency: 100,
            scaling_config: ScalingConfig::default(),
            health_check: HealthCheckConfig::default(),
            timeout_config: TimeoutConfig::default(),
        };
        let task = manager
            .ensure_task_with_id("task-hc".to_string(), &artifact, task_spec)
            .unwrap();
        let instance = manager.get_or_create_instance(&task).await.unwrap();

        fail_flag.store(true, Ordering::SeqCst);
        manager.process_health_checks_once().await;
        manager.process_health_checks_once().await;
        manager.process_health_checks_once().await;

        assert!(manager.get_instance(&instance.id().to_string()).is_none());
        assert!(task.get_instance(instance.id()).is_none());
    }
}
