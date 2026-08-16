use std::{collections::HashMap, sync::Arc, time::{Duration, Instant}};

use sha2::Digest;
use tokio::time::timeout;
use tonic::transport::Channel;

use crate::proto::sms::Task as SmsTask;
use crate::spearlet::config::SpearletConfig;

use super::{
    artifact::{Artifact, ArtifactSpec, InvocationType, ResourceLimits},
    manager::TaskExecutionManager,
    task::{HealthCheckConfig, ScalingConfig, TaskSpec, TimeoutConfig},
    ExecutionError, ExecutionResult, RuntimeType, Task,
};

/// Build a stable artifact id and spec from an SMS task definition.
/// 从 SMS task 定义构建稳定的 artifact id 和 spec。
fn artifact_spec_from_sms_task(sms_task: &SmsTask) -> (String, ArtifactSpec) {
    let (runtime_type, location_opt, checksum_opt, env) = if let Some(executable) = &sms_task.executable
    {
        let runtime_type = match executable.r#type {
            3 => RuntimeType::Kubernetes,
            4 => RuntimeType::Wasm,
            _ => RuntimeType::Process,
        };
        let location_opt = if executable.uri.is_empty() {
            None
        } else {
            Some(executable.uri.clone())
        };
        let checksum_opt = if executable.checksum_sha256.is_empty() {
            None
        } else {
            Some(executable.checksum_sha256.clone())
        };
        (runtime_type, location_opt, checksum_opt, executable.env.clone())
    } else {
        (RuntimeType::Process, None, None, HashMap::new())
    };

    let artifact_id = if let Some(checksum) = &checksum_opt {
        checksum.clone()
    } else if let Some(location) = &location_opt {
        let digest = sha2::Sha256::digest(location.as_bytes());
        digest.iter().map(|byte| format!("{:02x}", byte)).collect()
    } else {
        uuid::Uuid::new_v4().to_string()
    };

    let spec = ArtifactSpec {
        name: artifact_id.clone(),
        version: sms_task.version.clone(),
        description: None,
        runtime_type,
        runtime_config: HashMap::new(),
        location: location_opt,
        checksum_sha256: checksum_opt,
        environment: env,
        resource_limits: ResourceLimits::default(),
        invocation_type: InvocationType::ExistingTask,
        max_execution_timeout_ms: 30000,
        labels: sms_task.metadata.clone(),
    };
    (artifact_id, spec)
}

/// Build a local task spec from an SMS task definition.
/// 从 SMS task 定义构建本地 task spec。
fn task_spec_from_sms_task(sms_task: &SmsTask, artifact: &Arc<Artifact>) -> TaskSpec {
    let environment = if let Some(executable) = &sms_task.executable {
        executable.env.clone()
    } else {
        HashMap::new()
    };
    let desired_replicas = sms_task.desired_replicas.max(1);
    let mut task_config = sms_task.config.clone();
    task_config.insert(
        "task_scheduling_strategy".to_string(),
        sms_task.scheduling_strategy.to_string(),
    );

    TaskSpec {
        name: sms_task.name.clone(),
        task_type: super::task::TaskType::HttpHandler,
        runtime_type: artifact.spec.runtime_type,
        entry_point: "main".to_string(),
        handler_config: HashMap::new(),
        task_config,
        environment,
        invocation_type: InvocationType::ExistingTask,
        min_instances: 0,
        max_instances: desired_replicas,
        target_concurrency: 100,
        scaling_config: ScalingConfig::default(),
        health_check: HealthCheckConfig::default(),
        timeout_config: TimeoutConfig::default(),
    }
}

/// Ensure a local artifact exists for an SMS task.
/// 为 SMS task 确保本地 artifact 存在。
pub(super) fn materialize_local_artifact_from_sms_task(
    manager: &TaskExecutionManager,
    sms_task: &SmsTask,
) -> ExecutionResult<Arc<Artifact>> {
    let (artifact_id, spec) = artifact_spec_from_sms_task(sms_task);
    manager.ensure_artifact_with_id(artifact_id, spec)
}

/// Ensure a local task exists for an SMS task.
/// 为 SMS task 确保本地 task 存在。
pub(super) fn materialize_local_task_from_sms_task(
    manager: &TaskExecutionManager,
    sms_task: &SmsTask,
    artifact: &Arc<Artifact>,
) -> ExecutionResult<Arc<Task>> {
    let spec = task_spec_from_sms_task(sms_task, artifact);
    manager.ensure_task_with_id(sms_task.task_id.clone(), artifact, spec)
}

/// Materialize an SMS task into local artifact/task objects.
/// 将 SMS task 物化为本地 artifact/task 对象。
pub(super) fn materialize_sms_task(
    manager: &TaskExecutionManager,
    sms_task: &SmsTask,
) -> ExecutionResult<Arc<Task>> {
    let artifact = materialize_local_artifact_from_sms_task(manager, sms_task)?;
    materialize_local_task_from_sms_task(manager, sms_task, &artifact)
}

/// Fetch an SMS task definition with retry and timeout policy.
/// 按重试与超时策略拉取 SMS task 定义。
pub(super) async fn fetch_sms_task(
    channel: Option<Channel>,
    config: &SpearletConfig,
    task_id: &str,
) -> ExecutionResult<SmsTask> {
    let channel = channel.ok_or_else(|| ExecutionError::RuntimeError {
        message: "sms_grpc_addr is empty".to_string(),
    })?;
    let deadline = Instant::now() + Duration::from_millis(config.sms_connect_timeout_ms);
    let mut last_error: Option<String> = None;

    let response = loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break Err(ExecutionError::RuntimeError {
                message: last_error.unwrap_or_else(|| "connect sms timeout".to_string()),
            });
        }
        let per_attempt = remaining.min(Duration::from_secs(5)).max(Duration::from_millis(1));
        let mut client = crate::proto::sms::task_service_client::TaskServiceClient::new(channel.clone());
        let request = client.get_task(crate::proto::sms::GetTaskRequest {
            task_id: task_id.to_string(),
        });
        match timeout(per_attempt, request).await {
            Ok(Ok(response)) => break Ok(response.into_inner()),
            Ok(Err(error)) => last_error = Some(error.to_string()),
            Err(_) => last_error = Some("sms get_task timeout".to_string()),
        }
        tokio::time::sleep(Duration::from_millis(config.sms_connect_retry_ms)).await;
    }?;

    if !response.found {
        return Err(ExecutionError::TaskNotFound {
            id: task_id.to_string(),
        });
    }
    let sms_task = response.task.ok_or_else(|| ExecutionError::TaskNotFound {
        id: task_id.to_string(),
    })?;
    if sms_task.status == crate::proto::sms::TaskStatus::Deleting as i32 {
        return Err(ExecutionError::TaskNotFound {
            id: task_id.to_string(),
        });
    }
    Ok(sms_task)
}
