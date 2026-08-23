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
    fn validate_config(&self, _config: &instance::InstanceConfig) -> super::ExecutionResult<()> {
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
    fn validate_config(&self, _config: &instance::InstanceConfig) -> super::ExecutionResult<()> {
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
    let task = manager.get_task_by_id("task-long").unwrap();
    manager.create_instance_for_task(&task).await.unwrap();

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
            if let Some(s) = manager.get_execution_status("exec-long-1") {
                let status =
                    crate::spearlet::execution::execution_status::ExecutionPublicStatus::from_public_str(
                        &s.status,
                    );
                if matches!(
                    status,
                    crate::spearlet::execution::execution_status::ExecutionPublicStatus::Pending
                        | crate::spearlet::execution::execution_status::ExecutionPublicStatus::Running
                ) {
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

    let stored = manager.get_execution_status("exec-long-1").unwrap();
    assert_eq!(stored.status, "completed");
}

#[tokio::test]
async fn test_invocation_requires_preprovisioned_instance() {
    let mut rm = RuntimeManager::new();
    let _ = rm.register_runtime(
        RuntimeType::Process,
        Box::new(DummyRuntime {
            ty: RuntimeType::Process,
        }),
    );
    let manager = TaskExecutionManager::new(
        TaskExecutionManagerConfig::default(),
        Arc::new(rm),
        Arc::new(crate::spearlet::config::SpearletConfig::default()),
        None,
    )
    .await
    .unwrap();

    let artifact = manager
        .ensure_artifact_with_id(
            "artifact-preprovision".to_string(),
            crate::spearlet::execution::artifact::ArtifactSpec {
                name: "artifact-preprovision".to_string(),
                version: "1.0.0".to_string(),
                description: None,
                runtime_type: RuntimeType::Process,
                runtime_config: StdHashMap::new(),
                location: None,
                checksum_sha256: None,
                environment: StdHashMap::new(),
                resource_limits: Default::default(),
                invocation_type: crate::spearlet::execution::artifact::InvocationType::ExistingTask,
                max_execution_timeout_ms: 30_000,
                labels: StdHashMap::new(),
            },
        )
        .unwrap();
    manager
        .ensure_task_with_id(
            "task-preprovision".to_string(),
            &artifact,
            crate::spearlet::execution::task::TaskSpec {
                name: "task-preprovision".to_string(),
                task_type: crate::spearlet::execution::task::TaskType::HttpHandler,
                runtime_type: RuntimeType::Process,
                entry_point: "main".to_string(),
                handler_config: StdHashMap::new(),
                task_config: StdHashMap::new(),
                environment: StdHashMap::new(),
                invocation_type: crate::spearlet::execution::artifact::InvocationType::ExistingTask,
                min_instances: 0,
                max_instances: 10,
                target_concurrency: 100,
                scaling_config: crate::spearlet::execution::task::ScalingConfig::default(),
                health_check: crate::spearlet::execution::task::HealthCheckConfig::default(),
                timeout_config: crate::spearlet::execution::task::TimeoutConfig::default(),
            },
        )
        .unwrap();

    let error = manager
        .submit_invocation(crate::proto::spearlet::InvokeRequest {
            invocation_id: "inv-preprovision".to_string(),
            execution_id: "exec-preprovision".to_string(),
            task_id: "task-preprovision".to_string(),
            function_name: crate::spearlet::execution::DEFAULT_ENTRY_FUNCTION_NAME.to_string(),
            input: None,
            headers: StdHashMap::new(),
            environment: StdHashMap::new(),
            timeout_ms: 0,
            session_id: String::new(),
            mode: crate::proto::spearlet::ExecutionMode::Sync as i32,
            force_new_instance: false,
            metadata: StdHashMap::new(),
        })
        .await
        .unwrap_err();
    assert!(matches!(error, ExecutionError::SchedulingError { .. }));
}

#[tokio::test]
async fn test_reconcile_task_replica_assignment_creates_instances() {
    let mut rm = RuntimeManager::new();
    let _ = rm.register_runtime(
        RuntimeType::Process,
        Box::new(DummyRuntime {
            ty: RuntimeType::Process,
        }),
    );
    let manager = TaskExecutionManager::new(
        TaskExecutionManagerConfig::default(),
        Arc::new(rm),
        Arc::new(crate::spearlet::config::SpearletConfig::default()),
        None,
    )
    .await
    .unwrap();
    let artifact = manager
        .ensure_artifact_with_id(
            "artifact-reconcile".to_string(),
            crate::spearlet::execution::artifact::ArtifactSpec {
                name: "artifact-reconcile".to_string(),
                version: "1.0.0".to_string(),
                description: None,
                runtime_type: RuntimeType::Process,
                runtime_config: StdHashMap::new(),
                location: None,
                checksum_sha256: None,
                environment: StdHashMap::new(),
                resource_limits: Default::default(),
                invocation_type: crate::spearlet::execution::artifact::InvocationType::ExistingTask,
                max_execution_timeout_ms: 30_000,
                labels: StdHashMap::new(),
            },
        )
        .unwrap();
    manager
        .ensure_task_with_id(
            "task-reconcile".to_string(),
            &artifact,
            crate::spearlet::execution::task::TaskSpec {
                name: "task-reconcile".to_string(),
                task_type: crate::spearlet::execution::task::TaskType::HttpHandler,
                runtime_type: RuntimeType::Process,
                entry_point: "main".to_string(),
                handler_config: StdHashMap::new(),
                task_config: StdHashMap::new(),
                environment: StdHashMap::new(),
                invocation_type: crate::spearlet::execution::artifact::InvocationType::ExistingTask,
                min_instances: 0,
                max_instances: 10,
                target_concurrency: 100,
                scaling_config: crate::spearlet::execution::task::ScalingConfig::default(),
                health_check: crate::spearlet::execution::task::HealthCheckConfig::default(),
                timeout_config: crate::spearlet::execution::task::TimeoutConfig::default(),
            },
        )
        .unwrap();

    manager
        .reconcile_task_replica_assignment("task-reconcile", 2)
        .await
        .unwrap();

    let task = manager.get_task_by_id("task-reconcile").unwrap();
    assert_eq!(task.instance_count(), 2);
    assert_eq!(manager.list_instances().len(), 2);
}

#[tokio::test]
async fn test_underprovisioned_task_is_reconciled_back_to_desired_replicas() {
    let mut rm = RuntimeManager::new();
    let _ = rm.register_runtime(
        RuntimeType::Process,
        Box::new(DummyRuntime {
            ty: RuntimeType::Process,
        }),
    );
    let manager = TaskExecutionManager::new(
        TaskExecutionManagerConfig::default(),
        Arc::new(rm),
        Arc::new(crate::spearlet::config::SpearletConfig::default()),
        None,
    )
    .await
    .unwrap();
    let artifact = manager
        .ensure_artifact_with_id(
            "artifact-self-heal".to_string(),
            crate::spearlet::execution::artifact::ArtifactSpec {
                name: "artifact-self-heal".to_string(),
                version: "1.0.0".to_string(),
                description: None,
                runtime_type: RuntimeType::Process,
                runtime_config: StdHashMap::new(),
                location: None,
                checksum_sha256: None,
                environment: StdHashMap::new(),
                resource_limits: Default::default(),
                invocation_type: crate::spearlet::execution::artifact::InvocationType::ExistingTask,
                max_execution_timeout_ms: 30_000,
                labels: StdHashMap::new(),
            },
        )
        .unwrap();
    manager
        .ensure_task_with_id(
            "task-self-heal".to_string(),
            &artifact,
            crate::spearlet::execution::task::TaskSpec {
                name: "task-self-heal".to_string(),
                task_type: crate::spearlet::execution::task::TaskType::HttpHandler,
                runtime_type: RuntimeType::Process,
                entry_point: "main".to_string(),
                handler_config: StdHashMap::new(),
                task_config: StdHashMap::new(),
                environment: StdHashMap::new(),
                invocation_type: crate::spearlet::execution::artifact::InvocationType::ExistingTask,
                min_instances: 0,
                max_instances: 10,
                target_concurrency: 100,
                scaling_config: crate::spearlet::execution::task::ScalingConfig::default(),
                health_check: crate::spearlet::execution::task::HealthCheckConfig::default(),
                timeout_config: crate::spearlet::execution::task::TimeoutConfig::default(),
            },
        )
        .unwrap();

    manager
        .reconcile_task_replica_assignment("task-self-heal", 2)
        .await
        .unwrap();
    let initial_instances: Vec<_> = manager
        .list_instances()
        .into_iter()
        .map(|instance| instance.id().to_string())
        .collect();
    assert_eq!(initial_instances.len(), 2);

    manager
        .drain_and_destroy_instance(&initial_instances[0], Some("manual delete".to_string()))
        .await
        .unwrap();
    manager
        .ensure_task_meets_desired_replicas("task-self-heal")
        .await
        .unwrap();

    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let task = manager.get_task_by_id("task-self-heal").unwrap();
            if task.instance_count() == 2 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();

    let task = manager.get_task_by_id("task-self-heal").unwrap();
    let current_instances: Vec<_> = task
        .instances
        .iter()
        .map(|entry| entry.key().clone())
        .collect();
    assert_eq!(current_instances.len(), 2);
    assert!(!current_instances
        .iter()
        .any(|id| id == &initial_instances[0]));
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
    let instance = manager.create_instance_for_task(&task).await.unwrap();

    assert_eq!(task.instance_count(), 1);
    assert!(manager.get_instance(&instance.id().to_string()).is_some());

    manager
        .stop_and_unregister_instance(&instance)
        .await
        .unwrap();

    assert_eq!(task.instance_count(), 0);
    assert!(task.get_instance(instance.id()).is_none());
    assert!(manager.get_instance(&instance.id().to_string()).is_none());
}

#[tokio::test]
async fn test_destroy_instance_clears_active_execution_registry() {
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

    let artifact = manager
        .ensure_artifact_with_id(
            "artifact-destroy".to_string(),
            crate::spearlet::execution::artifact::ArtifactSpec {
                name: "artifact-destroy".to_string(),
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
            },
        )
        .unwrap();

    use crate::spearlet::execution::task::{
        HealthCheckConfig, ScalingConfig, TaskSpec, TaskType, TimeoutConfig,
    };
    let task = manager
        .ensure_task_with_id(
            "task-destroy".to_string(),
            &artifact,
            TaskSpec {
                name: "task-destroy".to_string(),
                task_type: TaskType::HttpHandler,
                runtime_type: RuntimeType::Process,
                entry_point: "main".to_string(),
                handler_config: StdHashMap::new(),
                task_config: StdHashMap::new(),
                environment: StdHashMap::new(),
                invocation_type: crate::spearlet::execution::artifact::InvocationType::ExistingTask,
                min_instances: 1,
                max_instances: 1,
                target_concurrency: 1,
                scaling_config: ScalingConfig::default(),
                health_check: HealthCheckConfig::default(),
                timeout_config: TimeoutConfig::default(),
            },
        )
        .unwrap();

    let instance = manager.create_instance_for_task(&task).await.unwrap();
    instance.set_status(InstanceStatus::Running);
    let started_at_ms = chrono::Utc::now().timestamp_millis();
    let mut log_next_seq = 1;
    manager
        .begin_execution_tracking(
            &instance,
            "inv-destroy-1",
            "main",
            "exec-destroy-1",
            started_at_ms,
            &mut log_next_seq,
        )
        .await
        .unwrap();

    assert_eq!(
        manager.active_execution_ids_for_instance(instance.id()),
        vec!["exec-destroy-1".to_string()]
    );

    manager
        .drain_and_destroy_instance(instance.id(), Some("test destroy".to_string()))
        .await
        .unwrap();

    let execution = manager.get_execution_status("exec-destroy-1").unwrap();
    assert_eq!(execution.status, "cancelled");
    assert_eq!(
        execution.error_message.as_deref(),
        Some("instance destroyed: test destroy")
    );
    assert!(manager.get_instance(&instance.id().to_string()).is_none());
    assert!(manager
        .active_execution_ids_for_instance(instance.id())
        .is_empty());
}

#[tokio::test]
async fn test_destroy_instance_marks_instance_stopping_before_runtime_stop() {
    use crate::spearlet::execution::instance::InstanceStatus;
    use std::sync::atomic::{AtomicU8, Ordering};

    struct StopStatusRuntime {
        ty: RuntimeType,
        observed_status: Arc<AtomicU8>,
    }

    #[async_trait]
    impl Runtime for StopStatusRuntime {
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
            instance: &Arc<instance::TaskInstance>,
        ) -> super::ExecutionResult<()> {
            let code = match instance.status() {
                InstanceStatus::Stopping => 1,
                InstanceStatus::Running => 2,
                InstanceStatus::Stopped => 3,
                _ => 0,
            };
            self.observed_status.store(code, Ordering::SeqCst);
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

    let observed_status = Arc::new(AtomicU8::new(0));
    let mut rm = RuntimeManager::new();
    rm.register_runtime(
        RuntimeType::Process,
        Box::new(StopStatusRuntime {
            ty: RuntimeType::Process,
            observed_status: observed_status.clone(),
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

    let artifact = manager
        .ensure_artifact_with_id(
            "artifact-destroy-status".to_string(),
            crate::spearlet::execution::artifact::ArtifactSpec {
                name: "artifact-destroy-status".to_string(),
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
            },
        )
        .unwrap();

    use crate::spearlet::execution::task::{
        HealthCheckConfig, ScalingConfig, TaskSpec, TaskType, TimeoutConfig,
    };
    let task = manager
        .ensure_task_with_id(
            "task-destroy-status".to_string(),
            &artifact,
            TaskSpec {
                name: "task-destroy-status".to_string(),
                task_type: TaskType::HttpHandler,
                runtime_type: RuntimeType::Process,
                entry_point: "main".to_string(),
                handler_config: StdHashMap::new(),
                task_config: StdHashMap::new(),
                environment: StdHashMap::new(),
                invocation_type: crate::spearlet::execution::artifact::InvocationType::ExistingTask,
                min_instances: 1,
                max_instances: 1,
                target_concurrency: 1,
                scaling_config: ScalingConfig::default(),
                health_check: HealthCheckConfig::default(),
                timeout_config: TimeoutConfig::default(),
            },
        )
        .unwrap();

    let instance = manager.create_instance_for_task(&task).await.unwrap();
    instance.set_status(InstanceStatus::Running);

    manager
        .drain_and_destroy_instance(instance.id(), Some("status transition".to_string()))
        .await
        .unwrap();

    assert_eq!(observed_status.load(Ordering::SeqCst), 1);
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

    assert!(manager.get_task_by_id(&task_id).is_none());
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
    assert!(manager.get_task_by_id(&desired).is_some());
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
    let instance = manager.create_instance_for_task(&task).await.unwrap();

    fail_flag.store(true, Ordering::SeqCst);
    manager.process_health_checks_once().await;
    manager.process_health_checks_once().await;
    manager.process_health_checks_once().await;

    assert!(manager.get_instance(&instance.id().to_string()).is_none());
    assert!(task.get_instance(instance.id()).is_none());
}
