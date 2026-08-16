//! Instance Scheduler
//! 实例调度器
//!
//! This module provides intelligent instance scheduling and load balancing
//! for task execution across multiple runtime instances.
//! 该模块为跨多个运行时实例的任务执行提供智能实例调度和负载均衡。

use super::{
    instance::{InstanceId, InstanceStatus, TaskInstance},
    task::{Task, TaskId},
    ExecutionResult,
};
use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Instant, SystemTime};
use tokio::sync::RwLock as AsyncRwLock;
use tracing::{debug, info};

/// Scheduling policy / 调度策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SchedulingPolicy {
    /// Round robin scheduling / 轮询调度
    RoundRobin,
    /// Least connections scheduling / 最少连接调度
    #[default]
    LeastConnections,
    /// Least response time scheduling / 最短响应时间调度
    LeastResponseTime,
    /// Weighted round robin scheduling / 加权轮询调度
    WeightedRoundRobin,
    /// Resource-based scheduling / 基于资源的调度
    ResourceBased,
    /// Random scheduling / 随机调度
    Random,
}

/// Scheduling decision / 调度决策
#[derive(Debug, Clone)]
pub struct SchedulingDecision {
    /// Selected instance / 选中的实例
    pub instance: Arc<TaskInstance>,
    /// Decision reason / 决策原因
    pub reason: String,
    /// Decision score / 决策分数
    pub score: f64,
    /// Decision timestamp / 决策时间戳
    pub timestamp: SystemTime,
}

/// Instance pool for a specific task / 特定任务的实例池
#[derive(Debug)]
struct TaskInstancePool {
    /// Task ID / 任务 ID
    #[allow(dead_code)]
    task_id: TaskId,
    /// Available instances / 可用实例
    instances: Vec<Arc<TaskInstance>>,
    /// Round robin index / 轮询索引
    round_robin_index: AtomicU64,
    /// Last access time / 最后访问时间
    last_access: SystemTime,
}

impl TaskInstancePool {
    fn new(task_id: TaskId) -> Self {
        Self {
            task_id,
            instances: Vec::new(),
            round_robin_index: AtomicU64::new(0),
            last_access: SystemTime::now(),
        }
    }

    fn add_instance(&mut self, instance: Arc<TaskInstance>) {
        self.instances.push(instance);
        self.last_access = SystemTime::now();
    }

    fn remove_instance(&mut self, instance_id: &InstanceId) -> bool {
        let initial_len = self.instances.len();
        self.instances.retain(|inst| inst.id() != instance_id);
        let removed = self.instances.len() != initial_len;
        if removed {
            self.last_access = SystemTime::now();
        }
        removed
    }

    fn get_instances(&self) -> &[Arc<TaskInstance>] {
        &self.instances
    }

    fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }

    #[allow(dead_code)]
    fn len(&self) -> usize {
        self.instances.len()
    }
}

/// Scheduling metrics / 调度指标
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulingMetrics {
    /// Total scheduling decisions / 总调度决策数
    pub total_decisions: u64,
    /// Successful decisions / 成功决策数
    pub successful_decisions: u64,
    /// Failed decisions / 失败决策数
    pub failed_decisions: u64,
    /// Average decision time / 平均决策时间
    pub average_decision_time_ms: f64,
    /// Total decision time / 总决策时间
    pub total_decision_time_ms: u64,
    /// Active instance pools / 活跃实例池数
    pub active_pools: u64,
    /// Total instances / 总实例数
    pub total_instances: u64,
    /// Available instances / 可用实例数
    pub available_instances: u64,
    /// Busy instances / 忙碌实例数
    pub busy_instances: u64,
}

impl Default for SchedulingMetrics {
    fn default() -> Self {
        Self {
            total_decisions: 0,
            successful_decisions: 0,
            failed_decisions: 0,
            average_decision_time_ms: 0.0,
            total_decision_time_ms: 0,
            active_pools: 0,
            total_instances: 0,
            available_instances: 0,
            busy_instances: 0,
        }
    }
}

/// Instance scheduler configuration / 实例调度器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerConfig {
    /// Scheduling policy / 调度策略
    pub policy: SchedulingPolicy,
    /// Health check enabled / 启用健康检查
    pub health_check_enabled: bool,
    /// Health check interval / 健康检查间隔
    pub health_check_interval_ms: u64,
    /// Instance timeout / 实例超时
    pub instance_timeout_ms: u64,
    /// Max retries for scheduling / 调度最大重试次数
    pub max_scheduling_retries: u32,
    /// Load balancing threshold / 负载均衡阈值
    pub load_balancing_threshold: f64,
    /// Instance warmup time / 实例预热时间
    pub instance_warmup_time_ms: u64,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            policy: SchedulingPolicy::default(),
            health_check_enabled: true,
            health_check_interval_ms: 10000,
            instance_timeout_ms: 30000,
            max_scheduling_retries: 3,
            load_balancing_threshold: 0.8,
            instance_warmup_time_ms: 5000,
        }
    }
}

/// Instance scheduler / 实例调度器
pub struct InstanceScheduler {
    /// Configuration / 配置
    config: SchedulerConfig,
    /// Instance pools by task ID / 按任务 ID 分组的实例池
    pools: Arc<AsyncRwLock<HashMap<TaskId, TaskInstancePool>>>,
    /// Global instance registry / 全局实例注册表
    instances: Arc<DashMap<InstanceId, Arc<TaskInstance>>>,
    /// Scheduling metrics / 调度指标
    metrics: Arc<RwLock<SchedulingMetrics>>,
    /// Decision counter / 决策计数器
    decision_counter: AtomicU64,
}

impl InstanceScheduler {
    /// Create a new instance scheduler / 创建新的实例调度器
    pub fn new(policy: SchedulingPolicy) -> Self {
        let config = SchedulerConfig {
            policy,
            ..Default::default()
        };

        Self {
            config,
            pools: Arc::new(AsyncRwLock::new(HashMap::new())),
            instances: Arc::new(DashMap::new()),
            metrics: Arc::new(RwLock::new(SchedulingMetrics::default())),
            decision_counter: AtomicU64::new(0),
        }
    }

    /// Create with custom configuration / 使用自定义配置创建
    pub fn with_config(config: SchedulerConfig) -> Self {
        Self {
            config,
            pools: Arc::new(AsyncRwLock::new(HashMap::new())),
            instances: Arc::new(DashMap::new()),
            metrics: Arc::new(RwLock::new(SchedulingMetrics::default())),
            decision_counter: AtomicU64::new(0),
        }
    }

    /// Add instance to scheduler / 向调度器添加实例
    pub async fn add_instance(&self, instance: Arc<TaskInstance>) -> ExecutionResult<()> {
        let instance_id = instance.id().to_string();
        let task_id = instance.task_id().to_string();

        // Add to global registry / 添加到全局注册表
        self.instances.insert(instance_id.clone(), instance.clone());

        // Add to task pool / 添加到任务池
        {
            let mut pools = self.pools.write().await;
            let pool = pools
                .entry(task_id.clone())
                .or_insert_with(|| TaskInstancePool::new(task_id));
            pool.add_instance(instance);
        } // 释放 pools 锁 / Release pools lock

        // Update metrics / 更新指标
        self.update_metrics().await;

        info!("Added instance {} to scheduler", instance_id);
        Ok(())
    }

    /// Remove instance from scheduler / 从调度器移除实例
    pub async fn remove_instance(&self, instance_id: &InstanceId) -> ExecutionResult<()> {
        // Remove from global registry / 从全局注册表移除
        let instance = self.instances.remove(instance_id).map(|(_, v)| v);

        if let Some(instance) = instance {
            let task_id = instance.task_id();

            // Remove from task pool / 从任务池移除
            {
                let mut pools = self.pools.write().await;
                if let Some(pool) = pools.get_mut(task_id) {
                    pool.remove_instance(instance_id);

                    // Remove empty pools / 移除空池
                    if pool.is_empty() {
                        pools.remove(task_id);
                    }
                }
            } // 释放 pools 锁 / Release pools lock

            // Update metrics / 更新指标
            self.update_metrics().await;

            info!("Removed instance {} from scheduler", instance_id);
        }

        Ok(())
    }

    /// Select best instance for task execution / 为任务执行选择最佳实例
    pub async fn select_instance(
        &self,
        task: &Arc<Task>,
    ) -> ExecutionResult<Option<Arc<TaskInstance>>> {
        let start_time = Instant::now();
        let decision_id = self.decision_counter.fetch_add(1, Ordering::SeqCst);

        let result = self.select_instance_internal(task).await;

        let decision_time = start_time.elapsed();
        let decision_time_ms = decision_time.as_millis() as u64;

        // Update metrics / 更新指标
        {
            let mut metrics = self.metrics.write();
            metrics.total_decisions += 1;
            metrics.total_decision_time_ms += decision_time_ms;
            metrics.average_decision_time_ms =
                metrics.total_decision_time_ms as f64 / metrics.total_decisions as f64;

            match &result {
                Ok(Some(_)) => metrics.successful_decisions += 1,
                Ok(None) => {} // No available instances / 没有可用实例
                Err(_) => metrics.failed_decisions += 1,
            }
        }

        debug!(
            "Scheduling decision {} for task {} took {}ms: {:?}",
            decision_id,
            task.id(),
            decision_time_ms,
            result
                .as_ref()
                .map(|opt| opt.as_ref().map(|inst| inst.id()))
        );

        result
    }

    /// Internal instance selection logic / 内部实例选择逻辑
    async fn select_instance_internal(
        &self,
        task: &Arc<Task>,
    ) -> ExecutionResult<Option<Arc<TaskInstance>>> {
        let pools = self.pools.read().await;
        let pool = match pools.get(task.id()) {
            Some(pool) => pool,
            None => return Ok(None), // No instances available / 没有可用实例
        };

        let instances = pool.get_instances();
        if instances.is_empty() {
            return Ok(None);
        }

        // Filter healthy and ready instances / 过滤健康且准备就绪的实例
        let available_instances: Vec<_> = instances
            .iter()
            .filter(|instance| {
                instance.status() == InstanceStatus::Running
                    && instance.is_healthy()
                    && instance.is_ready()
                    && !instance.is_at_capacity()
            })
            .cloned()
            .collect();

        if available_instances.is_empty() {
            return Ok(None);
        }

        // Apply scheduling policy / 应用调度策略
        let selected = match self.config.policy {
            SchedulingPolicy::RoundRobin => {
                self.select_round_robin(&available_instances, pool).await
            }
            SchedulingPolicy::LeastConnections => {
                self.select_least_connections(&available_instances).await
            }
            SchedulingPolicy::LeastResponseTime => {
                self.select_least_response_time(&available_instances).await
            }
            SchedulingPolicy::WeightedRoundRobin => {
                self.select_weighted_round_robin(&available_instances, pool)
                    .await
            }
            SchedulingPolicy::ResourceBased => {
                self.select_resource_based(&available_instances).await
            }
            SchedulingPolicy::Random => self.select_random(&available_instances).await,
        };

        Ok(selected)
    }

    /// Round robin selection / 轮询选择
    async fn select_round_robin(
        &self,
        instances: &[Arc<TaskInstance>],
        pool: &TaskInstancePool,
    ) -> Option<Arc<TaskInstance>> {
        if instances.is_empty() {
            return None;
        }

        let index = pool.round_robin_index.fetch_add(1, Ordering::SeqCst) as usize;
        let selected_index = index % instances.len();
        Some(instances[selected_index].clone())
    }

    /// Least connections selection / 最少连接选择
    async fn select_least_connections(
        &self,
        instances: &[Arc<TaskInstance>],
    ) -> Option<Arc<TaskInstance>> {
        instances
            .iter()
            .min_by_key(|instance| instance.get_metrics().active_requests)
            .cloned()
    }

    /// Least response time selection / 最短响应时间选择
    async fn select_least_response_time(
        &self,
        instances: &[Arc<TaskInstance>],
    ) -> Option<Arc<TaskInstance>> {
        instances
            .iter()
            .min_by(|a, b| {
                a.get_metrics()
                    .avg_request_time_ms
                    .partial_cmp(&b.get_metrics().avg_request_time_ms)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .cloned()
    }

    /// Weighted round robin selection / 加权轮询选择
    async fn select_weighted_round_robin(
        &self,
        instances: &[Arc<TaskInstance>],
        pool: &TaskInstancePool,
    ) -> Option<Arc<TaskInstance>> {
        // For simplicity, use capacity as weight / 为简单起见，使用容量作为权重
        let total_weight: u32 = instances
            .iter()
            .map(|instance| instance.config.max_concurrent_requests)
            .sum();

        if total_weight == 0 {
            return self.select_round_robin(instances, pool).await;
        }

        let target_weight =
            (pool.round_robin_index.fetch_add(1, Ordering::SeqCst) as u32) % total_weight;
        let mut current_weight = 0;

        for instance in instances {
            current_weight += instance.config.max_concurrent_requests;
            if current_weight > target_weight {
                return Some(instance.clone());
            }
        }

        instances.first().cloned()
    }

    /// Resource-based selection / 基于资源的选择
    async fn select_resource_based(
        &self,
        instances: &[Arc<TaskInstance>],
    ) -> Option<Arc<TaskInstance>> {
        instances
            .iter()
            .min_by(|a, b| {
                let load_a = a.get_load();
                let load_b = b.get_load();
                load_a
                    .partial_cmp(&load_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .cloned()
    }

    /// Random selection / 随机选择
    async fn select_random(&self, instances: &[Arc<TaskInstance>]) -> Option<Arc<TaskInstance>> {
        use rand::Rng;
        if instances.is_empty() {
            return None;
        }

        let mut rng = rand::thread_rng();
        let index = rng.gen_range(0..instances.len());
        Some(instances[index].clone())
    }

    /// Get instance by ID / 根据 ID 获取实例
    pub fn get_instance(&self, instance_id: &InstanceId) -> Option<Arc<TaskInstance>> {
        self.instances.get(instance_id).map(|entry| entry.clone())
    }

    /// List all instances / 列出所有实例
    pub fn list_instances(&self) -> Vec<Arc<TaskInstance>> {
        self.instances.iter().map(|entry| entry.clone()).collect()
    }

    /// List instances for task / 列出任务的实例
    pub async fn list_instances_for_task(&self, task_id: &TaskId) -> Vec<Arc<TaskInstance>> {
        let pools = self.pools.read().await;
        if let Some(pool) = pools.get(task_id.as_str()) {
            pool.get_instances().to_vec()
        } else {
            Vec::new()
        }
    }

    /// Get scheduling metrics / 获取调度指标
    pub fn get_metrics(&self) -> SchedulingMetrics {
        self.metrics.read().clone()
    }

    /// Update metrics / 更新指标
    async fn update_metrics(&self) {
        // 先收集所有需要的数据，避免长时间持有锁 / Collect all needed data first to avoid holding locks for long time
        let active_pools = {
            let pools = self.pools.read().await;
            pools.len() as u64
        }; // 释放 pools 锁 / Release pools lock

        let total_instances = self.instances.len() as u64;

        let (available_instances, busy_instances) = self
            .instances
            .iter()
            .map(|entry| entry.value().clone())
            .fold((0, 0), |(available, busy), instance| {
                if instance.is_ready() && !instance.is_at_capacity() {
                    (available + 1, busy)
                } else {
                    (available, busy + 1)
                }
            });

        // 最后更新指标，减少锁持有时间 / Update metrics last to reduce lock holding time
        let mut metrics = self.metrics.write();
        metrics.active_pools = active_pools;
        metrics.total_instances = total_instances;
        metrics.available_instances = available_instances;
        metrics.busy_instances = busy_instances;
    }

    /// Get configuration / 获取配置
    pub fn config(&self) -> &SchedulerConfig {
        &self.config
    }

    /// Update configuration / 更新配置
    pub fn update_config(&mut self, config: SchedulerConfig) {
        self.config = config;
        info!("Updated scheduler configuration: {:?}", self.config);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spearlet::execution::{
        artifact::ArtifactSpec,
        instance::{InstanceConfig, InstanceResourceLimits},
        task::{TaskSpec, TaskType},
    };
    use std::collections::HashMap;

    fn create_test_instance(_id: &str, task_id: TaskId) -> Arc<TaskInstance> {
        let config = InstanceConfig {
            task_id: task_id.clone(),
            artifact_id: "artifact-test".to_string(),
            runtime_type: crate::spearlet::execution::runtime::RuntimeType::Process,
            runtime_config: std::collections::HashMap::new(),
            task_config: std::collections::HashMap::new(),
            artifact: None,
            environment: std::collections::HashMap::new(),
            resource_limits: InstanceResourceLimits {
                max_cpu_cores: 1.0,
                max_memory_bytes: 512 * 1024 * 1024, // 512MB in bytes
                max_disk_bytes: 1024 * 1024 * 1024,  // 1GB in bytes
                max_network_bps: 1000000,            // 1Mbps
            },
            network_config: Default::default(),
            max_concurrent_requests: 10,
            request_timeout_ms: 30000,
        };
        // TaskInstance::new automatically generates a unique ID / TaskInstance::new 自动生成唯一ID
        let instance = Arc::new(TaskInstance::new(task_id, config));

        // 设置实例状态为Running且健康，以满足调度器的过滤条件
        // Set instance status to Running and healthy to meet scheduler filtering conditions
        instance.set_status(InstanceStatus::Running);
        instance.set_health_status(crate::spearlet::execution::instance::HealthStatus::Healthy);

        instance
    }

    fn create_test_task() -> Arc<Task> {
        let artifact_spec = ArtifactSpec {
            name: "test-artifact".to_string(),
            version: "1.0.0".to_string(),
            description: Some("Test artifact".to_string()),
            runtime_type: crate::spearlet::execution::runtime::RuntimeType::Process,
            runtime_config: HashMap::new(),
            location: None,
            checksum_sha256: None,
            environment: HashMap::new(),
            resource_limits: Default::default(),
            invocation_type: crate::spearlet::execution::artifact::InvocationType::NewTask,
            max_execution_timeout_ms: 30000,
            labels: HashMap::new(),
        };
        let task_spec = TaskSpec {
            name: artifact_spec.name.clone(),
            task_type: TaskType::HttpHandler,
            runtime_type: artifact_spec.runtime_type,
            entry_point: "main".to_string(),
            handler_config: HashMap::new(),
            task_config: HashMap::new(),
            environment: artifact_spec.environment.clone(),
            invocation_type: artifact_spec.invocation_type,
            min_instances: 1,
            max_instances: 10,
            target_concurrency: 1,
            scaling_config: Default::default(),
            health_check: Default::default(),
            timeout_config: Default::default(),
        };
        let task_id = "test-task-id".to_string();
        Arc::new(Task::new(task_id, task_spec))
    }

    #[tokio::test]
    async fn test_scheduler_creation() {
        let scheduler = InstanceScheduler::new(SchedulingPolicy::RoundRobin);
        assert_eq!(scheduler.config.policy, SchedulingPolicy::RoundRobin);
    }

    #[tokio::test]
    async fn test_add_remove_instance() {
        let scheduler = InstanceScheduler::new(SchedulingPolicy::RoundRobin);
        let task = create_test_task();
        let instance = create_test_instance("test-instance", task.id().to_string());

        // Add instance / 添加实例
        let result = scheduler.add_instance(instance.clone()).await;
        assert!(result.is_ok());

        // Check instance exists / 检查实例存在
        let instance_id = instance.id().to_string();
        let retrieved = scheduler.get_instance(&instance_id);
        assert!(retrieved.is_some());

        // Remove instance / 移除实例
        let result = scheduler.remove_instance(&instance_id).await;
        assert!(result.is_ok());

        // Check instance removed / 检查实例已移除
        let retrieved = scheduler.get_instance(&instance_id);
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_round_robin_selection() {
        let scheduler = InstanceScheduler::new(SchedulingPolicy::RoundRobin);
        let task = create_test_task();

        // Add multiple instances / 添加多个实例
        let instance1 = create_test_instance("instance-1", task.id().to_string());
        let instance2 = create_test_instance("instance-2", task.id().to_string());
        let instance3 = create_test_instance("instance-3", task.id().to_string());

        scheduler.add_instance(instance1.clone()).await.unwrap();
        scheduler.add_instance(instance2.clone()).await.unwrap();
        scheduler.add_instance(instance3.clone()).await.unwrap();

        // Test round robin selection / 测试轮询选择
        let mut selected_instances = Vec::new();
        for _ in 0..6 {
            if let Ok(Some(instance)) = scheduler.select_instance(&task).await {
                selected_instances.push(instance.id().to_string());
            }
        }

        // Should cycle through instances / 应该循环遍历实例
        assert_eq!(selected_instances.len(), 6);
    }

    #[tokio::test]
    async fn test_least_connections_selection() {
        let scheduler = InstanceScheduler::new(SchedulingPolicy::LeastConnections);
        let task = create_test_task();

        let instance1 = create_test_instance("instance-1", task.id().to_string());
        let instance2 = create_test_instance("instance-2", task.id().to_string());

        scheduler.add_instance(instance1.clone()).await.unwrap();
        scheduler.add_instance(instance2.clone()).await.unwrap();

        // Simulate different connection counts / 模拟不同的连接数
        instance1.record_request_start();
        instance1.record_request_start();

        // Should select instance with fewer connections / 应该选择连接数较少的实例
        if let Ok(Some(selected)) = scheduler.select_instance(&task).await {
            assert_eq!(selected.id(), instance2.id());
        }
    }

    #[tokio::test]
    async fn test_scheduler_skips_instances_at_capacity() {
        let scheduler = InstanceScheduler::new(SchedulingPolicy::LeastConnections);
        let task = create_test_task();

        let instance1 = create_test_instance("instance-1", task.id().to_string());
        let instance2 = create_test_instance("instance-2", task.id().to_string());

        for _ in 0..instance1.config.max_concurrent_requests {
            instance1.record_request_start();
        }

        scheduler.add_instance(instance1.clone()).await.unwrap();
        scheduler.add_instance(instance2.clone()).await.unwrap();

        let selected = scheduler.select_instance(&task).await.unwrap().unwrap();
        assert_eq!(selected.id(), instance2.id());
    }

    #[test]
    fn test_scheduling_metrics() {
        let mut metrics = SchedulingMetrics::default();
        assert_eq!(metrics.total_decisions, 0);
        assert_eq!(metrics.successful_decisions, 0);
        assert_eq!(metrics.failed_decisions, 0);

        metrics.total_decisions = 100;
        metrics.successful_decisions = 95;
        metrics.failed_decisions = 5;
        metrics.total_decision_time_ms = 5000;
        metrics.average_decision_time_ms = 50.0;

        assert_eq!(metrics.total_decisions, 100);
        assert_eq!(metrics.successful_decisions, 95);
        assert_eq!(metrics.failed_decisions, 5);
    }
}
