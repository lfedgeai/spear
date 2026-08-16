use axum::{
    extract::{Path, Query},
    Json,
};
use serde_json::json;

use crate::sms::gateway::GatewayState;
use crate::sms::query_support::{collect_task_instances_bounded, instance_is_active_and_fresh};

use super::types::ListQuery;

#[derive(serde::Deserialize)]
pub(crate) struct CreateExecutableBody {
    pub(crate) r#type: Option<String>,
    pub(crate) uri: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) checksum_sha256: Option<String>,
    pub(crate) args: Option<Vec<String>>,
    pub(crate) env: Option<std::collections::HashMap<String, String>>,
}

#[derive(serde::Deserialize)]
pub(crate) struct CreateTaskBody {
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    pub(crate) priority: Option<String>,
    pub(crate) desired_replicas: Option<u32>,
    pub(crate) scheduling_strategy: Option<String>,
    pub(crate) endpoint: String,
    pub(crate) version: String,
    pub(crate) capabilities: Option<Vec<String>>,
    pub(crate) metadata: Option<std::collections::HashMap<String, String>>,
    pub(crate) config: Option<std::collections::HashMap<String, String>>,
    pub(crate) executable: Option<CreateExecutableBody>,
}

#[derive(serde::Deserialize)]
pub(crate) struct DeleteTaskBody {
    pub(crate) reason: Option<String>,
    pub(crate) force: Option<bool>,
}

fn task_execution_fields(
    executable: Option<crate::proto::sms::TaskExecutable>,
) -> (
    String,
    String,
    String,
    String,
    Vec<String>,
    std::collections::HashMap<String, String>,
) {
    if let Some(exec) = executable {
        let executable_type = match exec.r#type {
            1 => "binary",
            2 => "script",
            3 => "container",
            4 => "wasm",
            5 => "process",
            _ => "unknown",
        };
        (
            executable_type.to_string(),
            exec.uri,
            exec.name,
            exec.checksum_sha256,
            exec.args,
            exec.env,
        )
    } else {
        (
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            vec![],
            std::collections::HashMap::new(),
        )
    }
}

async fn task_replica_counts(
    state: &GatewayState,
    task_id: &str,
) -> Result<(usize, usize), tonic::Status> {
    let mut client = state.execution_index_client.clone();
    let mut active_instances = 0usize;
    let mut ready_instances = 0usize;
    let now_ms = chrono::Utc::now().timestamp_millis();

    for instance in collect_task_instances_bounded(&mut client, task_id, 100).await? {
        if !instance_is_active_and_fresh(&state.config, instance.status, instance.last_seen_ms, now_ms)
        {
            continue;
        }
        active_instances += 1;
        if crate::sms::is_instance_routable(instance.status) {
            ready_instances += 1;
        }
    }

    Ok((active_instances, ready_instances))
}

fn task_summary_json(
    task: crate::proto::sms::Task,
    active_instances: usize,
    ready_instances: usize,
) -> serde_json::Value {
    let status = crate::sms::task_status_to_public_str(task.status);
    let priority = crate::sms::task_priority_to_public_str(task.priority);
    let (exec_type, exec_uri, exec_name, _, _, _) = task_execution_fields(task.executable);
    let desired_replicas = task.desired_replicas;

    json!({
        "task_id": task.task_id,
        "name": task.name,
        "description": task.description,
        "status": status,
        "priority": priority,
        "desired_replicas": desired_replicas,
        "active_instances": active_instances,
        "ready_instances": ready_instances,
        "underprovisioned": active_instances < desired_replicas as usize,
        "reconciling": active_instances < desired_replicas as usize,
        "scheduling_strategy": task.scheduling_strategy,
        "endpoint": task.endpoint,
        "version": task.version,
        "capabilities": task.capabilities,
        "registered_at": task.registered_at,
        "last_heartbeat": task.last_heartbeat,
        "metadata": task.metadata,
        "config": task.config,
        "executable_type": exec_type,
        "executable_uri": exec_uri,
        "executable_name": exec_name,
        "result_uris": task.result_uris,
        "last_result_uri": task.last_result_uri,
        "last_result_status": task.last_result_status,
        "last_completed_at": task.last_completed_at,
        "last_result_metadata": task.last_result_metadata,
        "deletion_requested_at": task.deletion_requested_at,
        "deletion_reason": task.deletion_reason,
    })
}

fn task_detail_json(task: crate::proto::sms::Task) -> serde_json::Value {
    let status = crate::sms::task_status_to_public_str(task.status);
    let priority = crate::sms::task_priority_to_public_str(task.priority);
    let (exec_type, exec_uri, exec_name, exec_sum, exec_args, exec_env) =
        task_execution_fields(task.executable);

    json!({
        "task_id": task.task_id,
        "name": task.name,
        "description": task.description,
        "status": status,
        "priority": priority,
        "desired_replicas": task.desired_replicas,
        "scheduling_strategy": task.scheduling_strategy,
        "endpoint": task.endpoint,
        "version": task.version,
        "capabilities": task.capabilities,
        "registered_at": task.registered_at,
        "last_heartbeat": task.last_heartbeat,
        "metadata": task.metadata,
        "config": task.config,
        "executable_type": exec_type,
        "executable_uri": exec_uri,
        "executable_name": exec_name,
        "executable_checksum": exec_sum,
        "executable_args": exec_args,
        "executable_env": exec_env,
        "result_uris": task.result_uris,
        "last_result_uri": task.last_result_uri,
        "last_result_status": task.last_result_status,
        "last_completed_at": task.last_completed_at,
        "last_result_metadata": task.last_result_metadata,
        "deletion_requested_at": task.deletion_requested_at,
        "deletion_reason": task.deletion_reason,
    })
}

pub(crate) async fn list_tasks(
    state: GatewayState,
    Query(q): Query<ListQuery>,
) -> Json<serde_json::Value> {
    use crate::proto::sms::ListTasksRequest;

    let mut client = state.task_client.clone();
    let resp = client
        .list_tasks(ListTasksRequest {
            status_filter: -1,
            priority_filter: -1,
            limit: 0,
            offset: 0,
        })
        .await
        .unwrap()
        .into_inner();
    let total = resp.total_count as usize;
    let tasks = resp.tasks;
    let mut list = Vec::with_capacity(tasks.len());
    for task in tasks {
        let counts = task_replica_counts(&state, &task.task_id)
            .await
            .unwrap_or((0, 0));
        list.push(task_summary_json(task, counts.0, counts.1));
    }

    if let Some(qs) = q.q.as_ref().map(|s| s.to_lowercase()) {
        list.retain(|item| {
            let id = item["task_id"].as_str().unwrap_or("").to_lowercase();
            let name = item["name"].as_str().unwrap_or("").to_lowercase();
            let endpoint = item["endpoint"].as_str().unwrap_or("").to_lowercase();
            let meta = item["metadata"].as_object();
            let meta_hit = meta
                .map(|m| {
                    m.iter().any(|(k, v)| {
                        k.to_lowercase().contains(&qs)
                            || v.as_str().unwrap_or("").to_lowercase().contains(&qs)
                    })
                })
                .unwrap_or(false);
            id.contains(&qs)
                || name.contains(&qs)
                || endpoint.contains(&qs)
                || meta_hit
        });
    }

    let (field, asc) = if let Some(sort) = &q.sort {
        let mut parts = sort.split(':');
        let field = parts.next().unwrap_or("").to_string();
        let order = parts.next().unwrap_or("asc");
        let asc = order != "desc";
        (field, asc)
    } else if let Some(field) = &q.sort_by {
        let asc = q.order.as_deref().unwrap_or("asc") != "desc";
        (field.clone(), asc)
    } else {
        (String::new(), true)
    };
    if !field.is_empty() {
        match field.as_str() {
            "last_heartbeat" => list.sort_by_key(|i| i["last_heartbeat"].as_i64().unwrap_or(0)),
            "registered_at" => list.sort_by_key(|i| i["registered_at"].as_i64().unwrap_or(0)),
            _ => {}
        }
        if !asc {
            list.reverse();
        }
    }

    if let Some(offset) = q.offset {
        if offset < list.len() {
            list = list.split_off(offset);
        } else {
            list.clear();
        }
    }
    if let Some(limit) = q.limit {
        if limit < list.len() {
            list.truncate(limit);
        }
    }
    Json(json!({ "tasks": list, "total_count": total }))
}

pub(crate) async fn create_task(
    state: GatewayState,
    Json(body): Json<CreateTaskBody>,
) -> Json<serde_json::Value> {
    use crate::proto::sms::{ExecutableType, RegisterTaskRequest, TaskExecutable, TaskPriority};

    let priority = match body.priority.as_ref().map(|s| s.to_ascii_lowercase()) {
        Some(s) if s == "low" => TaskPriority::Low as i32,
        Some(s) if s == "high" => TaskPriority::High as i32,
        Some(s) if s == "urgent" => TaskPriority::Urgent as i32,
        Some(s) if s == "unknown" => TaskPriority::Unknown as i32,
        _ => TaskPriority::Normal as i32,
    };
    let executable = body.executable.as_ref().map(|e| {
        let executable_type = match e.r#type.as_ref().map(|s| s.to_ascii_lowercase()) {
            Some(s) if s == "binary" => ExecutableType::Binary as i32,
            Some(s) if s == "script" => ExecutableType::Script as i32,
            Some(s) if s == "container" => ExecutableType::Container as i32,
            Some(s) if s == "wasm" => ExecutableType::Wasm as i32,
            Some(s) if s == "process" => ExecutableType::Process as i32,
            _ => ExecutableType::Unknown as i32,
        };
        TaskExecutable {
            r#type: executable_type,
            uri: e.uri.clone().unwrap_or_default(),
            name: e.name.clone().unwrap_or_default(),
            checksum_sha256: e.checksum_sha256.clone().unwrap_or_default(),
            args: e.args.clone().unwrap_or_default(),
            env: e.env.clone().unwrap_or_default(),
        }
    });
    let metadata = body.metadata.clone().unwrap_or_default();
    let scheduling_strategy = match body.scheduling_strategy.as_deref() {
        Some("spread") | None => crate::proto::sms::TaskSchedulingStrategy::Spread as i32,
        _ => crate::proto::sms::TaskSchedulingStrategy::Unknown as i32,
    };
    let request = RegisterTaskRequest {
        name: body.name,
        description: body.description.unwrap_or_default(),
        priority,
        endpoint: body.endpoint,
        version: body.version,
        capabilities: body.capabilities.unwrap_or_default(),
        metadata: metadata.clone(),
        config: body.config.unwrap_or_default(),
        executable,
        desired_replicas: body.desired_replicas.unwrap_or(1),
        scheduling_strategy,
    };
    let mut client = state.task_client.clone();
    match client.register_task(tonic::Request::new(request)).await {
        Ok(response) => {
            let inner = response.into_inner();
            Json(
                json!({ "success": inner.success, "task_id": inner.task_id, "message": inner.message }),
            )
        }
        Err(error) => Json(json!({ "success": false, "message": error.to_string() })),
    }
}

pub(crate) async fn get_task_detail(
    state: GatewayState,
    Path(task_id): Path<String>,
) -> Json<serde_json::Value> {
    use crate::proto::sms::GetTaskRequest;

    let mut client = state.task_client.clone();
    match client.get_task(GetTaskRequest { task_id }).await {
        Ok(response) => {
            let inner = response.into_inner();
            if let Some(task) = inner.task {
                Json(json!({
                    "found": true,
                    "task": task_detail_json(task),
                }))
            } else {
                Json(json!({"found": false}))
            }
        }
        Err(_) => Json(json!({"found": false})),
    }
}

pub(crate) async fn delete_task_admin(
    state: GatewayState,
    Path(task_id): Path<String>,
    Json(body): Json<DeleteTaskBody>,
) -> Json<serde_json::Value> {
    use crate::proto::sms::DeleteTaskRequest;

    let mut client = state.task_client.clone();
    match client
        .delete_task(tonic::Request::new(DeleteTaskRequest {
            task_id,
            reason: body.reason.unwrap_or_default(),
            force: body.force.unwrap_or(false),
        }))
        .await
    {
        Ok(response) => {
            let inner = response.into_inner();
            let task = inner.task.map(|task| {
                json!({
                    "task_id": task.task_id,
                    "status": crate::sms::task_status_to_public_str(task.status),
                    "deletion_requested_at": task.deletion_requested_at,
                    "deletion_reason": task.deletion_reason,
                })
            });
            Json(json!({
                "success": inner.success,
                "message": inner.message,
                "task_id": inner.task_id,
                "task": task,
            }))
        }
        Err(error) => Json(json!({ "success": false, "message": error.to_string() })),
    }
}
