//! Task Implementation
//! Task 实现
//!
//! A Task represents a logical execution unit within an artifact.
//! Task 表示 artifact 内的逻辑执行单元。

use super::artifact::InvocationType;
use super::{ArtifactId, ExecutionError, ExecutionResult, InstanceId, RuntimeType};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use uuid::Uuid;

/// Unique identifier for a task / Task 的唯一标识符
pub type TaskId = String;

/// Task type enumeration / Task 类型枚举
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskType {
    /// HTTP endpoint handler / HTTP 端点处理器
    HttpHandler,
    /// Background job processor / 后台作业处理器
    BackgroundJob,
    /// Stream processor / 流处理器
    StreamProcessor,
    /// Event handler / 事件处理器
    EventHandler,
    /// Scheduled task / 定时任务
    ScheduledTask,
    /// Custom task type / 自定义任务类型
    Custom(String),
}

/// Task status enumeration / Task 状态枚举
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    /// Task is being initialized / 正在初始化 Task
    Initializing,
    /// Task is ready for execution / Task 准备就绪可执行
    Ready,
    /// Task is currently running / Task 正在运行
    Running,
    /// Task is paused / Task 已暂停
    Paused,
    /// Task is being scaled / 正在扩缩容 Task
    Scaling,
    /// Task is being stopped / 正在停止 Task
    Stopping,
    /// Task has stopped / Task 已停止
    Stopped,
    /// Task encountered an error / Task 遇到错误
    Error(String),
}

/// Task specification / Task 规格
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSpec {
    /// Task name / Task 名称
    pub name: String,
    /// Task type / Task 类型
    pub task_type: TaskType,
    /// Runtime type / 运行时类型
    pub runtime_type: RuntimeType,
    /// Entry point (e.g., function name, main class) / 入口点（如函数名、主类）
    pub entry_point: String,
    /// Handler configuration / 处理器配置
    pub handler_config: HashMap<String, serde_json::Value>,
    pub task_config: HashMap<String, String>,
    /// Environment variables / 环境变量
    pub environment: HashMap<String, String>,
    /// Invocation type / 调用类型
    pub invocation_type: InvocationType,
    /// Minimum number of instances / 最小实例数
    pub min_instances: u32,
    /// Maximum number of instances / 最大实例数
    pub max_instances: u32,
    /// Target concurrency per instance / 每个实例的目标并发数
    pub target_concurrency: u32,
    /// Scaling configuration / 扩缩容配置
    pub scaling_config: ScalingConfig,
    /// Health check configuration / 健康检查配置
    pub health_check: HealthCheckConfig,
    /// Timeout configuration / 超时配置
    pub timeout_config: TimeoutConfig,
}

/// Scaling configuration / 扩缩容配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScalingConfig {
    /// Scale up threshold (CPU percentage) / 扩容阈值（CPU 百分比）
    pub scale_up_cpu_threshold: f64,
    /// Scale down threshold (CPU percentage) / 缩容阈值（CPU 百分比）
    pub scale_down_cpu_threshold: f64,
    /// Scale up threshold (memory percentage) / 扩容阈值（内存百分比）
    pub scale_up_memory_threshold: f64,
    /// Scale down threshold (memory percentage) / 缩容阈值（内存百分比）
    pub scale_down_memory_threshold: f64,
    /// Scale up threshold (request queue length) / 扩容阈值（请求队列长度）
    pub scale_up_queue_threshold: u32,
    /// Scale down threshold (request queue length) / 缩容阈值（请求队列长度）
    pub scale_down_queue_threshold: u32,
    /// Cooldown period between scaling operations / 扩缩容操作之间的冷却期
    pub cooldown_period_ms: u64,
}

impl Default for ScalingConfig {
    fn default() -> Self {
        Self {
            scale_up_cpu_threshold: 70.0,
            scale_down_cpu_threshold: 30.0,
            scale_up_memory_threshold: 80.0,
            scale_down_memory_threshold: 40.0,
            scale_up_queue_threshold: 10,
            scale_down_queue_threshold: 2,
            cooldown_period_ms: 60000, // 1 minute
        }
    }
}

/// Health check configuration / 健康检查配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    /// Health check endpoint / 健康检查端点
    pub endpoint: Option<String>,
    /// Health check interval in milliseconds / 健康检查间隔（毫秒）
    pub interval_ms: u64,
    /// Health check timeout in milliseconds / 健康检查超时（毫秒）
    pub timeout_ms: u64,
    /// Number of consecutive failures before marking unhealthy / 标记为不健康前的连续失败次数
    pub failure_threshold: u32,
    /// Number of consecutive successes before marking healthy / 标记为健康前的连续成功次数
    pub success_threshold: u32,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            endpoint: None,
            interval_ms: 30000, // 30 seconds
            timeout_ms: 5000,   // 5 seconds
            failure_threshold: 3,
            success_threshold: 1,
        }
    }
}

/// Timeout configuration / 超时配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutConfig {
    /// Initialization timeout in milliseconds / 初始化超时（毫秒）
    pub init_timeout_ms: u64,
    /// Execution timeout in milliseconds / 执行超时（毫秒）
    pub execution_timeout_ms: u64,
    /// Shutdown timeout in milliseconds / 关闭超时（毫秒）
    pub shutdown_timeout_ms: u64,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            init_timeout_ms: 30000,       // 30 seconds
            execution_timeout_ms: 300000, // 5 minutes
            shutdown_timeout_ms: 10000,   // 10 seconds
        }
    }
}

/// Task metrics / Task 指标
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMetrics {
    /// Total number of executions / 总执行次数
    pub total_executions: u64,
    /// Number of successful executions / 成功执行次数
    pub successful_executions: u64,
    /// Number of failed executions / 失败执行次数
    pub failed_executions: u64,
    /// Current number of active instances / 当前活跃实例数
    pub active_instances: u32,
    /// Current queue length / 当前队列长度
    pub queue_length: u32,
    /// Average execution time in milliseconds / 平均执行时间（毫秒）
    pub avg_execution_time_ms: f64,
    /// 95th percentile execution time in milliseconds / 95分位执行时间（毫秒）
    pub p95_execution_time_ms: f64,
    /// Current CPU usage percentage / 当前 CPU 使用率百分比
    pub cpu_usage_percent: f64,
    /// Current memory usage in bytes / 当前内存使用量（字节）
    pub memory_usage_bytes: u64,
    /// Last execution timestamp / 最后执行时间戳
    pub last_execution_time: Option<SystemTime>,
    /// Last scaling operation timestamp / 最后扩缩容操作时间戳
    pub last_scaling_time: Option<SystemTime>,
}

impl Default for TaskMetrics {
    fn default() -> Self {
        Self {
            total_executions: 0,
            successful_executions: 0,
            failed_executions: 0,
            active_instances: 0,
            queue_length: 0,
            avg_execution_time_ms: 0.0,
            p95_execution_time_ms: 0.0,
            cpu_usage_percent: 0.0,
            memory_usage_bytes: 0,
            last_execution_time: None,
            last_scaling_time: None,
        }
    }
}

/// Task implementation / Task 实现
#[derive(Debug)]
pub struct Task {
    /// Unique identifier / 唯一标识符
    pub id: TaskId,
    /// Parent artifact ID / 父 Artifact ID
    pub artifact_id: ArtifactId,
    /// Task specification / Task 规格
    pub spec: TaskSpec,
    /// Current status / 当前状态
    pub status: Arc<parking_lot::RwLock<TaskStatus>>,
    /// Associated instances / 关联的实例
    pub instances: Arc<DashMap<InstanceId, Arc<super::TaskInstance>>>,
    /// Task metrics / Task 指标
    pub metrics: Arc<parking_lot::RwLock<TaskMetrics>>,
    /// Creation timestamp / 创建时间戳
    pub created_at: SystemTime,
    /// Last updated timestamp / 最后更新时间戳
    pub updated_at: Arc<parking_lot::RwLock<SystemTime>>,
    /// Instance counter for generating unique instance IDs / 实例计数器用于生成唯一实例ID
    instance_counter: AtomicU64,
}

impl Task {
    /// Create a new task / 创建新的 Task
    pub fn new(artifact_id: ArtifactId, spec: TaskSpec) -> Self {
        let id = format!("task-{}", Uuid::new_v4());
        let now = SystemTime::now();

        Self {
            id,
            artifact_id,
            spec,
            status: Arc::new(parking_lot::RwLock::new(TaskStatus::Initializing)),
            instances: Arc::new(DashMap::new()),
            metrics: Arc::new(parking_lot::RwLock::new(TaskMetrics::default())),
            created_at: now,
            updated_at: Arc::new(parking_lot::RwLock::new(now)),
            instance_counter: AtomicU64::new(0),
        }
    }

    /// Create a new task with fixed ID / 使用固定ID创建新的 Task
    pub fn new_with_id(id: TaskId, artifact_id: ArtifactId, spec: TaskSpec) -> Self {
        let now = SystemTime::now();
        Self {
            id,
            artifact_id,
            spec,
            status: Arc::new(parking_lot::RwLock::new(TaskStatus::Initializing)),
            instances: Arc::new(DashMap::new()),
            metrics: Arc::new(parking_lot::RwLock::new(TaskMetrics::default())),
            created_at: now,
            updated_at: Arc::new(parking_lot::RwLock::new(now)),
            instance_counter: AtomicU64::new(0),
        }
    }

    /// Get task ID / 获取 Task ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get parent artifact ID / 获取父 Artifact ID
    pub fn artifact_id(&self) -> &str {
        &self.artifact_id
    }

    /// Get current status / 获取当前状态
    pub fn status(&self) -> TaskStatus {
        self.status.read().clone()
    }

    /// Update status / 更新状态
    pub fn set_status(&self, status: TaskStatus) {
        *self.status.write() = status;
        *self.updated_at.write() = SystemTime::now();
    }

    /// Mark the task as stopping when runtime teardown begins.
    /// 当运行态拆除开始时将 task 标记为 stopping。
    pub fn mark_stopping(&self) {
        self.set_status(TaskStatus::Stopping);
    }

    /// Mark the task as fully stopped after teardown completes.
    /// 在拆除完成后将 task 标记为 stopped。
    pub fn mark_stopped(&self) {
        self.set_status(TaskStatus::Stopped);
    }

    /// Check whether the task is already draining or fully stopped.
    /// 检查 task 是否已经进入 draining 或完全停止。
    pub fn is_stopping_or_stopped(&self) -> bool {
        matches!(self.status(), TaskStatus::Stopping | TaskStatus::Stopped)
    }

    /// Check whether the task should be retained while idle.
    /// 检查 task 在空闲时是否应继续保留。
    pub fn is_retained_when_idle(&self) -> bool {
        matches!(self.status(), TaskStatus::Ready | TaskStatus::Running)
    }

    /// Reconcile inventory-driven lifecycle states from current instance count.
    /// 根据当前实例数量对齐由实例库存驱动的生命周期状态。
    fn sync_status_from_instance_inventory(&self) {
        let next_status = {
            let current = self.status();
            let instance_count = self.instance_count();
            match current {
                TaskStatus::Stopping | TaskStatus::Stopped | TaskStatus::Error(_) => None,
                TaskStatus::Paused | TaskStatus::Scaling => None,
                TaskStatus::Initializing => {
                    if instance_count > 0 {
                        Some(TaskStatus::Running)
                    } else {
                        None
                    }
                }
                TaskStatus::Ready => {
                    if instance_count > 0 {
                        Some(TaskStatus::Running)
                    } else {
                        None
                    }
                }
                TaskStatus::Running => {
                    if instance_count == 0 {
                        Some(TaskStatus::Ready)
                    } else {
                        None
                    }
                }
            }
        };
        if let Some(next_status) = next_status {
            self.set_status(next_status);
        }
    }

    /// Add an instance to this task / 向此 Task 添加实例
    pub fn add_instance(&self, instance: Arc<super::TaskInstance>) -> ExecutionResult<()> {
        // Verify instance belongs to this task / 验证实例属于此任务
        if instance.task_id != self.id {
            return Err(ExecutionError::InvalidConfiguration {
                message: format!(
                    "Instance task ID {} does not match task ID {}",
                    instance.task_id, self.id
                ),
            });
        }

        self.instances.insert(instance.id.clone(), instance);
        *self.updated_at.write() = SystemTime::now();

        // Update active instances count / 更新活跃实例数
        self.update_metrics(|metrics| {
            metrics.active_instances = self.instances.len() as u32;
        });
        self.sync_status_from_instance_inventory();

        Ok(())
    }

    /// Remove an instance from this task / 从此 Task 移除实例
    pub fn remove_instance(&self, instance_id: &str) -> ExecutionResult<Arc<super::TaskInstance>> {
        let instance = self
            .instances
            .remove(instance_id)
            .map(|(_, instance)| {
                *self.updated_at.write() = SystemTime::now();
                instance
            })
            .ok_or_else(|| ExecutionError::InstanceNotFound {
                id: instance_id.to_string(),
            })?;

        // Update active instances count / 更新活跃实例数
        self.update_metrics(|metrics| {
            metrics.active_instances = self.instances.len() as u32;
        });
        self.sync_status_from_instance_inventory();

        Ok(instance)
    }

    /// Get an instance by ID / 根据 ID 获取实例
    pub fn get_instance(&self, instance_id: &str) -> Option<Arc<super::TaskInstance>> {
        self.instances
            .get(instance_id)
            .map(|entry| entry.value().clone())
    }

    /// List all instances / 列出所有实例
    pub fn list_instances(&self) -> Vec<Arc<super::TaskInstance>> {
        self.instances
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Get the number of instances in this task / 获取此 Task 中的实例数量
    pub fn instance_count(&self) -> usize {
        self.instances.len()
    }

    /// Create instance configuration for this task / 为此任务创建实例配置
    pub fn create_instance_config(&self) -> super::instance::InstanceConfig {
        use super::instance::{InstanceConfig, InstanceResourceLimits, NetworkConfig};
        use std::collections::HashMap;

        let mut env = self.spec.environment.clone();
        env.insert("TASK_ID".to_string(), self.id.clone());

        InstanceConfig {
            task_id: self.id.clone(),
            artifact_id: self.artifact_id.clone(),
            runtime_type: self.spec.runtime_type,
            runtime_config: HashMap::new(),
            task_config: self.spec.task_config.clone(),
            artifact: None,
            environment: env,
            resource_limits: InstanceResourceLimits {
                max_cpu_cores: 1.0,
                max_memory_bytes: 512 * 1024 * 1024, // 512MB
                max_disk_bytes: 1024 * 1024 * 1024,  // 1GB
                max_network_bps: 100 * 1024 * 1024,  // 100Mbps
            },
            network_config: NetworkConfig::default(),
            max_concurrent_requests: self.spec.target_concurrency,
            request_timeout_ms: self.spec.timeout_config.execution_timeout_ms,
        }
    }

    /// Generate a unique instance ID / 生成唯一的实例 ID
    pub fn generate_instance_id(&self) -> String {
        let counter = self.instance_counter.fetch_add(1, Ordering::SeqCst);
        format!("{}-inst-{}", self.id, counter)
    }

    /// Update metrics / 更新指标
    pub fn update_metrics<F>(&self, updater: F)
    where
        F: FnOnce(&mut TaskMetrics),
    {
        let mut metrics = self.metrics.write();
        updater(&mut metrics);
        *self.updated_at.write() = SystemTime::now();
    }

    /// Get current metrics / 获取当前指标
    pub fn get_metrics(&self) -> TaskMetrics {
        self.metrics.read().clone()
    }

    /// Check if task is ready for execution / 检查 Task 是否准备就绪可执行
    pub fn is_ready(&self) -> bool {
        matches!(self.status(), TaskStatus::Ready | TaskStatus::Running)
    }

    /// Check if task can be scaled / 检查 Task 是否可以扩缩容
    pub fn can_scale(&self) -> bool {
        matches!(self.status(), TaskStatus::Ready | TaskStatus::Running)
    }

    /// Check if scaling is needed / 检查是否需要扩缩容
    pub fn needs_scaling(&self) -> Option<bool> {
        if !self.can_scale() {
            return None;
        }

        let metrics = self.get_metrics();
        let config = &self.spec.scaling_config;

        // Check if we need to scale up / 检查是否需要扩容
        let should_scale_up = (metrics.cpu_usage_percent > config.scale_up_cpu_threshold
            || metrics.memory_usage_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
                > config.scale_up_memory_threshold
            || metrics.queue_length > config.scale_up_queue_threshold)
            && metrics.active_instances < self.spec.max_instances;

        // Check if we need to scale down / 检查是否需要缩容
        let should_scale_down = (metrics.cpu_usage_percent < config.scale_down_cpu_threshold
            && metrics.memory_usage_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
                < config.scale_down_memory_threshold
            && metrics.queue_length < config.scale_down_queue_threshold)
            && metrics.active_instances > self.spec.min_instances;

        if should_scale_up {
            Some(true)
        } else if should_scale_down {
            Some(false)
        } else {
            None
        }
    }

    /// Get task age / 获取 Task 年龄
    pub fn age(&self) -> Duration {
        SystemTime::now()
            .duration_since(self.created_at)
            .unwrap_or_default()
    }

    /// Get time since last update / 获取自上次更新以来的时间
    pub fn time_since_update(&self) -> Duration {
        let last_update = *self.updated_at.read();
        SystemTime::now()
            .duration_since(last_update)
            .unwrap_or_default()
    }

    /// Check if cooldown period has passed since last scaling / 检查自上次扩缩容以来是否已过冷却期
    pub fn is_scaling_cooldown_over(&self) -> bool {
        let metrics = self.metrics.read();
        if let Some(last_scaling) = metrics.last_scaling_time {
            let cooldown_duration =
                Duration::from_millis(self.spec.scaling_config.cooldown_period_ms);
            SystemTime::now()
                .duration_since(last_scaling)
                .unwrap_or_default()
                >= cooldown_duration
        } else {
            true // No previous scaling operation / 没有之前的扩缩容操作
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spearlet::execution::TaskInstance;

    #[test]
    fn test_task_creation() {
        let spec = TaskSpec {
            name: "test-task".to_string(),
            task_type: TaskType::HttpHandler,
            runtime_type: RuntimeType::Kubernetes,
            entry_point: "main".to_string(),
            handler_config: HashMap::new(),
            task_config: HashMap::new(),
            environment: HashMap::new(),
            invocation_type: InvocationType::NewTask,
            min_instances: 1,
            max_instances: 10,
            target_concurrency: 100,
            scaling_config: ScalingConfig::default(),
            health_check: HealthCheckConfig::default(),
            timeout_config: TimeoutConfig::default(),
        };

        let task = Task::new("artifact-123".to_string(), spec);
        assert!(task.id.starts_with("task-"));
        assert_eq!(task.artifact_id, "artifact-123");
        assert_eq!(task.status(), TaskStatus::Initializing);
        assert!(task.instances.is_empty());
    }

    #[test]
    fn test_instance_id_generation() {
        let spec = TaskSpec {
            name: "test-task".to_string(),
            task_type: TaskType::HttpHandler,
            runtime_type: RuntimeType::Kubernetes,
            entry_point: "main".to_string(),
            handler_config: HashMap::new(),
            task_config: HashMap::new(),
            environment: HashMap::new(),
            invocation_type: InvocationType::NewTask,
            min_instances: 1,
            max_instances: 10,
            target_concurrency: 100,
            scaling_config: ScalingConfig::default(),
            health_check: HealthCheckConfig::default(),
            timeout_config: TimeoutConfig::default(),
        };

        let task = Task::new("artifact-123".to_string(), spec);
        let id1 = task.generate_instance_id();
        let id2 = task.generate_instance_id();

        assert_ne!(id1, id2);
        assert!(id1.contains(&task.id));
        assert!(id2.contains(&task.id));
        assert!(id1.contains("-inst-"));
        assert!(id2.contains("-inst-"));
    }

    #[test]
    fn test_scaling_decision() {
        let mut spec = TaskSpec {
            name: "test-task".to_string(),
            task_type: TaskType::HttpHandler,
            runtime_type: RuntimeType::Kubernetes,
            entry_point: "main".to_string(),
            handler_config: HashMap::new(),
            task_config: HashMap::new(),
            environment: HashMap::new(),
            invocation_type: InvocationType::NewTask,
            min_instances: 1,
            max_instances: 10,
            target_concurrency: 100,
            scaling_config: ScalingConfig::default(),
            health_check: HealthCheckConfig::default(),
            timeout_config: TimeoutConfig::default(),
        };

        spec.scaling_config.scale_up_cpu_threshold = 70.0;
        spec.scaling_config.scale_down_cpu_threshold = 30.0;

        let task = Task::new("artifact-123".to_string(), spec);
        task.set_status(TaskStatus::Running);

        // Test scale up condition / 测试扩容条件
        task.update_metrics(|metrics| {
            metrics.cpu_usage_percent = 80.0;
            metrics.active_instances = 2;
        });

        assert_eq!(task.needs_scaling(), Some(true));

        // Test scale down condition / 测试缩容条件
        task.update_metrics(|metrics| {
            metrics.cpu_usage_percent = 20.0;
            metrics.active_instances = 5;
        });

        assert_eq!(task.needs_scaling(), Some(false));

        // Test no scaling needed / 测试不需要扩缩容
        task.update_metrics(|metrics| {
            metrics.cpu_usage_percent = 50.0;
            metrics.active_instances = 3;
        });

        assert_eq!(task.needs_scaling(), None);
    }

    #[test]
    fn test_instance_config_injects_task_id_env() {
        let mut env = HashMap::new();
        env.insert("FOO".to_string(), "bar".to_string());

        let spec = TaskSpec {
            name: "test-task".to_string(),
            task_type: TaskType::HttpHandler,
            runtime_type: RuntimeType::Wasm,
            entry_point: "main".to_string(),
            handler_config: HashMap::new(),
            task_config: HashMap::new(),
            environment: env,
            invocation_type: InvocationType::NewTask,
            min_instances: 1,
            max_instances: 10,
            target_concurrency: 100,
            scaling_config: ScalingConfig::default(),
            health_check: HealthCheckConfig::default(),
            timeout_config: TimeoutConfig::default(),
        };

        let task = Task::new("artifact-123".to_string(), spec);
        let cfg = task.create_instance_config();
        assert_eq!(
            cfg.environment.get("TASK_ID").cloned(),
            Some(task.id().to_string())
        );
        assert_eq!(cfg.environment.get("FOO").cloned(), Some("bar".to_string()));
    }

    #[test]
    fn test_instance_inventory_syncs_task_status() {
        let spec = TaskSpec {
            name: "test-task".to_string(),
            task_type: TaskType::HttpHandler,
            runtime_type: RuntimeType::Wasm,
            entry_point: "main".to_string(),
            handler_config: HashMap::new(),
            task_config: HashMap::new(),
            environment: HashMap::new(),
            invocation_type: InvocationType::NewTask,
            min_instances: 1,
            max_instances: 10,
            target_concurrency: 100,
            scaling_config: ScalingConfig::default(),
            health_check: HealthCheckConfig::default(),
            timeout_config: TimeoutConfig::default(),
        };

        let task = Task::new("artifact-123".to_string(), spec);
        let instance = Arc::new(TaskInstance::new(
            task.id().to_string(),
            task.create_instance_config(),
        ));

        assert_eq!(task.status(), TaskStatus::Initializing);
        task.add_instance(instance.clone()).unwrap();
        assert_eq!(task.status(), TaskStatus::Running);

        task.remove_instance(instance.id()).unwrap();
        assert_eq!(task.status(), TaskStatus::Ready);
    }

    #[test]
    fn test_stopping_status_is_preserved_while_instances_drain() {
        let spec = TaskSpec {
            name: "test-task".to_string(),
            task_type: TaskType::HttpHandler,
            runtime_type: RuntimeType::Wasm,
            entry_point: "main".to_string(),
            handler_config: HashMap::new(),
            task_config: HashMap::new(),
            environment: HashMap::new(),
            invocation_type: InvocationType::NewTask,
            min_instances: 1,
            max_instances: 10,
            target_concurrency: 100,
            scaling_config: ScalingConfig::default(),
            health_check: HealthCheckConfig::default(),
            timeout_config: TimeoutConfig::default(),
        };

        let task = Task::new("artifact-123".to_string(), spec);
        let instance = Arc::new(TaskInstance::new(
            task.id().to_string(),
            task.create_instance_config(),
        ));

        task.add_instance(instance.clone()).unwrap();
        task.mark_stopping();
        task.remove_instance(instance.id()).unwrap();

        assert_eq!(task.status(), TaskStatus::Stopping);
    }
}
