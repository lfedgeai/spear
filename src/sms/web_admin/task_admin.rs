use axum::{
    extract::{Path, Query},
    Json,
};

use crate::sms::gateway::GatewayState;
use crate::sms::query_support::{collect_task_instances_bounded, instance_is_active_and_fresh};
use crate::sms::task_api::{
    build_register_task_request, task_to_admin_detail_response, task_to_admin_summary_response,
    task_to_deleted_snapshot, AdminDeletedTaskSnapshot, AdminTaskDetailResponse,
    AdminTaskSummaryResponse, RegisterTaskSpec, TaskExecutableSpec,
};

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

async fn task_replica_counts(
    state: &GatewayState,
    task_id: &str,
) -> Result<(usize, usize), tonic::Status> {
    let mut client = state.execution_index_client.clone();
    let mut active_instances = 0usize;
    let mut ready_instances = 0usize;
    let now_ms = chrono::Utc::now().timestamp_millis();

    for instance in collect_task_instances_bounded(&mut client, task_id, 100).await? {
        if !instance_is_active_and_fresh(
            &state.config,
            instance.status,
            instance.last_seen_ms,
            now_ms,
        ) {
            continue;
        }
        active_instances += 1;
        if crate::sms::is_instance_routable(instance.status) {
            ready_instances += 1;
        }
    }

    Ok((active_instances, ready_instances))
}

#[derive(serde::Serialize)]
pub(crate) struct AdminTaskListResponse {
    pub(crate) tasks: Vec<AdminTaskSummaryResponse>,
    pub(crate) total_count: usize,
}

#[derive(serde::Serialize)]
pub(crate) struct AdminTaskDetailEnvelope {
    pub(crate) found: bool,
    pub(crate) task: Option<AdminTaskDetailResponse>,
}

#[derive(serde::Serialize)]
pub(crate) struct AdminTaskMutationResponse {
    pub(crate) success: bool,
    pub(crate) message: String,
    pub(crate) task_id: Option<String>,
    pub(crate) task: Option<AdminDeletedTaskSnapshot>,
}

pub(crate) async fn list_tasks(
    state: GatewayState,
    Query(q): Query<ListQuery>,
) -> Json<AdminTaskListResponse> {
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
        list.push(task_to_admin_summary_response(task, counts.0, counts.1));
    }

    if let Some(qs) = q.q.as_ref().map(|s| s.to_lowercase()) {
        list.retain(|item| {
            let meta_hit = item
                .metadata
                .iter()
                .any(|(k, v)| k.to_lowercase().contains(&qs) || v.to_lowercase().contains(&qs));
            item.task_id.to_lowercase().contains(&qs)
                || item.name.to_lowercase().contains(&qs)
                || item.endpoint.to_lowercase().contains(&qs)
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
            "last_heartbeat" => list.sort_by_key(|item| item.last_heartbeat),
            "registered_at" => list.sort_by_key(|item| item.registered_at),
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
    Json(AdminTaskListResponse {
        tasks: list,
        total_count: total,
    })
}

pub(crate) async fn create_task(
    state: GatewayState,
    Json(body): Json<CreateTaskBody>,
) -> Json<serde_json::Value> {
    use crate::proto::sms::TaskPriority;

    let priority = match body.priority.as_ref().map(|s| s.to_ascii_lowercase()) {
        Some(s) if s == "low" => TaskPriority::Low as i32,
        Some(s) if s == "high" => TaskPriority::High as i32,
        Some(s) if s == "urgent" => TaskPriority::Urgent as i32,
        Some(s) if s == "unknown" => TaskPriority::Unknown as i32,
        _ => TaskPriority::Normal as i32,
    };
    let request = build_register_task_request(RegisterTaskSpec {
        name: body.name,
        description: body.description.unwrap_or_default(),
        priority,
        desired_replicas: body.desired_replicas.unwrap_or(1),
        scheduling_strategy: body.scheduling_strategy,
        endpoint: body.endpoint,
        version: body.version,
        capabilities: body.capabilities.unwrap_or_default(),
        metadata: body.metadata.unwrap_or_default(),
        config: body.config.unwrap_or_default(),
        executable: body.executable.map(|executable| TaskExecutableSpec {
            executable_type: executable.r#type.unwrap_or_default(),
            uri: executable.uri.unwrap_or_default(),
            name: executable.name,
            checksum_sha256: executable.checksum_sha256,
            args: executable.args,
            env: executable.env,
        }),
    });
    let mut client = state.task_client.clone();
    match client.register_task(tonic::Request::new(request)).await {
        Ok(response) => {
            let inner = response.into_inner();
            Json(serde_json::json!({
                "success": inner.success,
                "task_id": inner.task_id,
                "message": inner.message
            }))
        }
        Err(error) => Json(serde_json::json!({ "success": false, "message": error.to_string() })),
    }
}

pub(crate) async fn get_task_detail(
    state: GatewayState,
    Path(task_id): Path<String>,
) -> Json<AdminTaskDetailEnvelope> {
    use crate::proto::sms::GetTaskRequest;

    let mut client = state.task_client.clone();
    match client.get_task(GetTaskRequest { task_id }).await {
        Ok(response) => {
            let inner = response.into_inner();
            if let Some(task) = inner.task {
                Json(AdminTaskDetailEnvelope {
                    found: true,
                    task: Some(task_to_admin_detail_response(task)),
                })
            } else {
                Json(AdminTaskDetailEnvelope {
                    found: false,
                    task: None,
                })
            }
        }
        Err(_) => Json(AdminTaskDetailEnvelope {
            found: false,
            task: None,
        }),
    }
}

pub(crate) async fn delete_task_admin(
    state: GatewayState,
    Path(task_id): Path<String>,
    Json(body): Json<DeleteTaskBody>,
) -> Json<AdminTaskMutationResponse> {
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
            Json(AdminTaskMutationResponse {
                success: inner.success,
                message: inner.message,
                task_id: Some(inner.task_id),
                task: inner.task.map(task_to_deleted_snapshot),
            })
        }
        Err(error) => Json(AdminTaskMutationResponse {
            success: false,
            message: error.to_string(),
            task_id: None,
            task: None,
        }),
    }
}
