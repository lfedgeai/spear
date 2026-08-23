//! Shared task API mapping helpers for SMS.
//! SMS 的任务 API 共享映射辅助模块。
//!
//! This module centralizes task request construction and task presentation
//! mapping so HTTP handlers and admin endpoints can stay focused on transport.
//! 此模块集中处理任务请求构造与任务展示映射，让 HTTP 处理器和管理端接口专注于传输层职责。

use serde::Serialize;
use std::collections::HashMap;

use crate::proto::sms::{
    ExecutableType, RegisterTaskRequest, Task, TaskExecutable, TaskSchedulingStrategy,
};
use crate::sms::{task_priority_to_public_str, task_status_to_public_str};

fn executable_type_from_public_str(value: &str) -> i32 {
    match value.trim().to_ascii_lowercase().as_str() {
        "binary" => ExecutableType::Binary as i32,
        "script" => ExecutableType::Script as i32,
        "container" => ExecutableType::Container as i32,
        "wasm" => ExecutableType::Wasm as i32,
        "process" => ExecutableType::Process as i32,
        _ => ExecutableType::Unknown as i32,
    }
}

fn executable_type_to_public_str(value: i32) -> &'static str {
    match ExecutableType::try_from(value).unwrap_or(ExecutableType::Unknown) {
        ExecutableType::Binary => "binary",
        ExecutableType::Script => "script",
        ExecutableType::Container => "container",
        ExecutableType::Wasm => "wasm",
        ExecutableType::Process => "process",
        _ => "unknown",
    }
}

fn scheduling_strategy_from_public_str(value: Option<&str>) -> i32 {
    match value.map(|v| v.trim().to_ascii_lowercase()) {
        Some(v) if v == "spread" => TaskSchedulingStrategy::Spread as i32,
        None => TaskSchedulingStrategy::Spread as i32,
        _ => TaskSchedulingStrategy::Unknown as i32,
    }
}

fn scheduling_strategy_to_public_str(value: i32) -> &'static str {
    match TaskSchedulingStrategy::try_from(value).unwrap_or(TaskSchedulingStrategy::Spread) {
        TaskSchedulingStrategy::Spread => "spread",
        TaskSchedulingStrategy::Unknown => "unknown",
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TaskExecutableSpec {
    pub(crate) executable_type: String,
    pub(crate) uri: String,
    pub(crate) name: Option<String>,
    pub(crate) checksum_sha256: Option<String>,
    pub(crate) args: Option<Vec<String>>,
    pub(crate) env: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone)]
pub(crate) struct RegisterTaskSpec {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) priority: i32,
    pub(crate) desired_replicas: u32,
    pub(crate) scheduling_strategy: Option<String>,
    pub(crate) endpoint: String,
    pub(crate) version: String,
    pub(crate) capabilities: Vec<String>,
    pub(crate) metadata: HashMap<String, String>,
    pub(crate) config: HashMap<String, String>,
    pub(crate) executable: Option<TaskExecutableSpec>,
}

pub(crate) fn build_register_task_request(spec: RegisterTaskSpec) -> RegisterTaskRequest {
    RegisterTaskRequest {
        name: spec.name,
        description: spec.description,
        priority: spec.priority,
        endpoint: spec.endpoint,
        version: spec.version,
        capabilities: spec.capabilities,
        metadata: spec.metadata,
        config: spec.config,
        executable: spec.executable.map(|exec| TaskExecutable {
            r#type: executable_type_from_public_str(&exec.executable_type),
            uri: exec.uri,
            name: exec.name.unwrap_or_default(),
            checksum_sha256: exec.checksum_sha256.unwrap_or_default(),
            args: exec.args.unwrap_or_default(),
            env: exec.env.unwrap_or_default(),
        }),
        desired_replicas: spec.desired_replicas,
        scheduling_strategy: scheduling_strategy_from_public_str(
            spec.scheduling_strategy.as_deref(),
        ),
    }
}

#[derive(Debug, Clone, Default)]
struct TaskExecutableView {
    executable_type: String,
    executable_uri: String,
    executable_name: String,
    executable_checksum: String,
    executable_args: Vec<String>,
    executable_env: HashMap<String, String>,
}

impl From<Option<TaskExecutable>> for TaskExecutableView {
    fn from(executable: Option<TaskExecutable>) -> Self {
        let Some(exec) = executable else {
            return Self::default();
        };

        Self {
            executable_type: executable_type_to_public_str(exec.r#type).to_string(),
            executable_uri: exec.uri,
            executable_name: exec.name,
            executable_checksum: exec.checksum_sha256,
            executable_args: exec.args,
            executable_env: exec.env,
        }
    }
}

#[derive(Debug, Clone)]
struct TaskPresentation {
    task_id: String,
    name: String,
    description: String,
    status: String,
    priority: String,
    desired_replicas: u32,
    scheduling_strategy: String,
    endpoint: String,
    version: String,
    capabilities: Vec<String>,
    registered_at: i64,
    last_heartbeat: i64,
    metadata: HashMap<String, String>,
    config: HashMap<String, String>,
    result_uris: Vec<String>,
    last_result_uri: String,
    last_result_status: String,
    last_completed_at: i64,
    last_result_metadata: HashMap<String, String>,
    deletion_requested_at: i64,
    deletion_reason: String,
    executable: TaskExecutableView,
}

impl From<Task> for TaskPresentation {
    fn from(task: Task) -> Self {
        Self {
            task_id: task.task_id,
            name: task.name,
            description: task.description,
            status: task_status_to_public_str(task.status).to_string(),
            priority: task_priority_to_public_str(task.priority).to_string(),
            desired_replicas: task.desired_replicas,
            scheduling_strategy: scheduling_strategy_to_public_str(task.scheduling_strategy)
                .to_string(),
            endpoint: task.endpoint,
            version: task.version,
            capabilities: task.capabilities,
            registered_at: task.registered_at,
            last_heartbeat: task.last_heartbeat,
            metadata: task.metadata,
            config: task.config,
            result_uris: task.result_uris,
            last_result_uri: task.last_result_uri,
            last_result_status: task.last_result_status,
            last_completed_at: task.last_completed_at,
            last_result_metadata: task.last_result_metadata,
            deletion_requested_at: task.deletion_requested_at,
            deletion_reason: task.deletion_reason,
            executable: task.executable.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PublicTaskResponse {
    pub task_id: String,
    pub name: String,
    pub description: String,
    pub status: String,
    pub priority: String,
    pub desired_replicas: u32,
    pub scheduling_strategy: String,
    pub endpoint: String,
    pub version: String,
    pub capabilities: Vec<String>,
    pub registered_at: i64,
    pub last_heartbeat: i64,
    pub metadata: HashMap<String, String>,
    pub config: HashMap<String, String>,
    pub result_uris: Vec<String>,
    pub last_result_uri: String,
    pub last_result_status: String,
    pub last_completed_at: i64,
    pub last_result_metadata: HashMap<String, String>,
}

pub(crate) fn task_to_public_response(task: Task) -> PublicTaskResponse {
    let presentation = TaskPresentation::from(task);
    PublicTaskResponse {
        task_id: presentation.task_id,
        name: presentation.name,
        description: presentation.description,
        status: presentation.status,
        priority: presentation.priority,
        desired_replicas: presentation.desired_replicas,
        scheduling_strategy: presentation.scheduling_strategy,
        endpoint: presentation.endpoint,
        version: presentation.version,
        capabilities: presentation.capabilities,
        registered_at: presentation.registered_at,
        last_heartbeat: presentation.last_heartbeat,
        metadata: presentation.metadata,
        config: presentation.config,
        result_uris: presentation.result_uris,
        last_result_uri: presentation.last_result_uri,
        last_result_status: presentation.last_result_status,
        last_completed_at: presentation.last_completed_at,
        last_result_metadata: presentation.last_result_metadata,
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminTaskSummaryResponse {
    pub(crate) task_id: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) status: String,
    pub(crate) priority: String,
    pub(crate) desired_replicas: u32,
    pub(crate) active_instances: usize,
    pub(crate) ready_instances: usize,
    pub(crate) underprovisioned: bool,
    pub(crate) reconciling: bool,
    pub(crate) scheduling_strategy: String,
    pub(crate) endpoint: String,
    pub(crate) version: String,
    pub(crate) capabilities: Vec<String>,
    pub(crate) registered_at: i64,
    pub(crate) last_heartbeat: i64,
    pub(crate) metadata: HashMap<String, String>,
    pub(crate) config: HashMap<String, String>,
    pub(crate) executable_type: String,
    pub(crate) executable_uri: String,
    pub(crate) executable_name: String,
    pub(crate) result_uris: Vec<String>,
    pub(crate) last_result_uri: String,
    pub(crate) last_result_status: String,
    pub(crate) last_completed_at: i64,
    pub(crate) last_result_metadata: HashMap<String, String>,
    pub(crate) deletion_requested_at: i64,
    pub(crate) deletion_reason: String,
}

pub(crate) fn task_to_admin_summary_response(
    task: Task,
    active_instances: usize,
    ready_instances: usize,
) -> AdminTaskSummaryResponse {
    let presentation = TaskPresentation::from(task);
    let underprovisioned = active_instances < presentation.desired_replicas as usize;
    AdminTaskSummaryResponse {
        task_id: presentation.task_id,
        name: presentation.name,
        description: presentation.description,
        status: presentation.status,
        priority: presentation.priority,
        desired_replicas: presentation.desired_replicas,
        active_instances,
        ready_instances,
        underprovisioned,
        reconciling: underprovisioned,
        scheduling_strategy: presentation.scheduling_strategy,
        endpoint: presentation.endpoint,
        version: presentation.version,
        capabilities: presentation.capabilities,
        registered_at: presentation.registered_at,
        last_heartbeat: presentation.last_heartbeat,
        metadata: presentation.metadata,
        config: presentation.config,
        executable_type: presentation.executable.executable_type,
        executable_uri: presentation.executable.executable_uri,
        executable_name: presentation.executable.executable_name,
        result_uris: presentation.result_uris,
        last_result_uri: presentation.last_result_uri,
        last_result_status: presentation.last_result_status,
        last_completed_at: presentation.last_completed_at,
        last_result_metadata: presentation.last_result_metadata,
        deletion_requested_at: presentation.deletion_requested_at,
        deletion_reason: presentation.deletion_reason,
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminTaskDetailResponse {
    pub(crate) task_id: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) status: String,
    pub(crate) priority: String,
    pub(crate) desired_replicas: u32,
    pub(crate) scheduling_strategy: String,
    pub(crate) endpoint: String,
    pub(crate) version: String,
    pub(crate) capabilities: Vec<String>,
    pub(crate) registered_at: i64,
    pub(crate) last_heartbeat: i64,
    pub(crate) metadata: HashMap<String, String>,
    pub(crate) config: HashMap<String, String>,
    pub(crate) executable_type: String,
    pub(crate) executable_uri: String,
    pub(crate) executable_name: String,
    pub(crate) executable_checksum: String,
    pub(crate) executable_args: Vec<String>,
    pub(crate) executable_env: HashMap<String, String>,
    pub(crate) result_uris: Vec<String>,
    pub(crate) last_result_uri: String,
    pub(crate) last_result_status: String,
    pub(crate) last_completed_at: i64,
    pub(crate) last_result_metadata: HashMap<String, String>,
    pub(crate) deletion_requested_at: i64,
    pub(crate) deletion_reason: String,
}

pub(crate) fn task_to_admin_detail_response(task: Task) -> AdminTaskDetailResponse {
    let presentation = TaskPresentation::from(task);
    AdminTaskDetailResponse {
        task_id: presentation.task_id,
        name: presentation.name,
        description: presentation.description,
        status: presentation.status,
        priority: presentation.priority,
        desired_replicas: presentation.desired_replicas,
        scheduling_strategy: presentation.scheduling_strategy,
        endpoint: presentation.endpoint,
        version: presentation.version,
        capabilities: presentation.capabilities,
        registered_at: presentation.registered_at,
        last_heartbeat: presentation.last_heartbeat,
        metadata: presentation.metadata,
        config: presentation.config,
        executable_type: presentation.executable.executable_type,
        executable_uri: presentation.executable.executable_uri,
        executable_name: presentation.executable.executable_name,
        executable_checksum: presentation.executable.executable_checksum,
        executable_args: presentation.executable.executable_args,
        executable_env: presentation.executable.executable_env,
        result_uris: presentation.result_uris,
        last_result_uri: presentation.last_result_uri,
        last_result_status: presentation.last_result_status,
        last_completed_at: presentation.last_completed_at,
        last_result_metadata: presentation.last_result_metadata,
        deletion_requested_at: presentation.deletion_requested_at,
        deletion_reason: presentation.deletion_reason,
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminDeletedTaskSnapshot {
    pub(crate) task_id: String,
    pub(crate) status: String,
    pub(crate) deletion_requested_at: i64,
    pub(crate) deletion_reason: String,
}

pub(crate) fn task_to_deleted_snapshot(task: Task) -> AdminDeletedTaskSnapshot {
    let presentation = TaskPresentation::from(task);
    AdminDeletedTaskSnapshot {
        task_id: presentation.task_id,
        status: presentation.status,
        deletion_requested_at: presentation.deletion_requested_at,
        deletion_reason: presentation.deletion_reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::sms::{TaskPriority, TaskStatus};

    fn sample_task() -> Task {
        Task {
            task_id: "task-1".to_string(),
            name: "demo".to_string(),
            description: "demo task".to_string(),
            status: TaskStatus::Active as i32,
            priority: TaskPriority::High as i32,
            endpoint: "http://localhost:8080".to_string(),
            version: "1.0.0".to_string(),
            capabilities: vec!["cpu".to_string()],
            registered_at: 10,
            last_heartbeat: 20,
            metadata: HashMap::from([("owner".to_string(), "team-a".to_string())]),
            config: HashMap::from([("timeout".to_string(), "30".to_string())]),
            executable: Some(TaskExecutable {
                r#type: ExecutableType::Wasm as i32,
                uri: "file:///tmp/demo.wasm".to_string(),
                name: "demo.wasm".to_string(),
                checksum_sha256: "abc".to_string(),
                args: vec!["--verbose".to_string()],
                env: HashMap::from([("RUST_LOG".to_string(), "debug".to_string())]),
            }),
            result_uris: vec!["s3://bucket/result".to_string()],
            last_result_uri: "s3://bucket/result".to_string(),
            last_result_status: "completed".to_string(),
            last_completed_at: 30,
            last_result_metadata: HashMap::from([("size".to_string(), "1".to_string())]),
            deletion_requested_at: 40,
            deletion_reason: "cleanup".to_string(),
            desired_replicas: 2,
            scheduling_strategy: TaskSchedulingStrategy::Spread as i32,
        }
    }

    #[test]
    fn build_register_task_request_maps_public_fields() {
        let request = build_register_task_request(RegisterTaskSpec {
            name: "demo".to_string(),
            description: "demo task".to_string(),
            priority: TaskPriority::Urgent as i32,
            desired_replicas: 3,
            scheduling_strategy: Some("spread".to_string()),
            endpoint: "http://localhost:8080".to_string(),
            version: "1.0.0".to_string(),
            capabilities: vec!["cpu".to_string()],
            metadata: HashMap::from([("owner".to_string(), "team-a".to_string())]),
            config: HashMap::new(),
            executable: Some(TaskExecutableSpec {
                executable_type: "wasm".to_string(),
                uri: "file:///tmp/demo.wasm".to_string(),
                name: Some("demo.wasm".to_string()),
                checksum_sha256: Some("abc".to_string()),
                args: Some(vec!["--verbose".to_string()]),
                env: Some(HashMap::from([(
                    "RUST_LOG".to_string(),
                    "debug".to_string(),
                )])),
            }),
        });

        assert_eq!(request.priority, TaskPriority::Urgent as i32);
        assert_eq!(
            request.scheduling_strategy,
            TaskSchedulingStrategy::Spread as i32
        );
        let executable = request.executable.expect("executable should exist");
        assert_eq!(executable.r#type, ExecutableType::Wasm as i32);
        assert_eq!(executable.uri, "file:///tmp/demo.wasm");
    }

    #[test]
    fn task_public_response_uses_public_strings() {
        let response = task_to_public_response(sample_task());
        assert_eq!(response.status, "active");
        assert_eq!(response.priority, "high");
        assert_eq!(response.scheduling_strategy, "spread");
        assert_eq!(response.last_result_uri, "s3://bucket/result");
    }

    #[test]
    fn admin_detail_response_keeps_executable_fields() {
        let response = task_to_admin_detail_response(sample_task());
        assert_eq!(response.executable_type, "wasm");
        assert_eq!(response.executable_name, "demo.wasm");
        assert_eq!(response.executable_checksum, "abc");
        assert_eq!(response.executable_args, vec!["--verbose".to_string()]);
        assert_eq!(
            response.executable_env.get("RUST_LOG").map(String::as_str),
            Some("debug")
        );
    }
}
