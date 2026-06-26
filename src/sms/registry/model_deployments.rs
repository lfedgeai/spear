use std::pin::Pin;
use std::sync::Arc;

use tonic::Status;
use tokio_stream::StreamExt;

use crate::proto::sms::{
    DeleteModelDeploymentResponse, ListModelDeploymentsRequest, ListModelDeploymentsResponse,
    ModelDeploymentEvent, ModelDeploymentPhase, ModelDeploymentRecord, ModelDeploymentStatus,
    ReportModelDeploymentStatusRequest, ReportModelDeploymentStatusResponse,
    UpsertModelDeploymentResponse,
    WatchModelDeploymentsRequest, WatchModelDeploymentsResponse,
};
use crate::sms::registry::state::ModelDeploymentRegistryState;

/// Build one filtered/paginated model deployment listing snapshot.
/// 构建一次过滤/分页后的 model deployment 列表快照。
pub async fn list_model_deployments_page(
    state: &ModelDeploymentRegistryState,
    req: ListModelDeploymentsRequest,
) -> ListModelDeploymentsResponse {
    let limit = if req.limit == 0 { 200 } else { req.limit.min(500) };
    let offset = req.offset;
    let filter_node = req.target_node_uuid.trim().to_string();
    let filter_provider = req.provider.trim().to_string();

    let registry_revision = state.watch.current_revision();
    let guard = state.records.read().await;
    let mut list = guard
        .values()
        .cloned()
        .filter(|r| {
            if !filter_node.is_empty() {
                r.spec
                    .as_ref()
                    .map(|s| s.target_node_uuid == filter_node)
                    .unwrap_or(false)
            } else {
                true
            }
        })
        .filter(|r| {
            if !filter_provider.is_empty() {
                r.spec
                    .as_ref()
                    .map(|s| s.provider == filter_provider)
                    .unwrap_or(false)
            } else {
                true
            }
        })
        .collect::<Vec<_>>();
    list.sort_by(|a, b| b.updated_at_ms.cmp(&a.updated_at_ms));

    let total_count = list.len() as u32;
    let start = offset as usize;
    let end = start.saturating_add(limit as usize);
    let page = if start >= list.len() {
        Vec::new()
    } else {
        list[start..list.len().min(end)].to_vec()
    };

    ListModelDeploymentsResponse {
        revision: registry_revision,
        records: page,
        total_count,
    }
}

/// Validate, normalize, persist, and publish one model deployment upsert.
/// 校验、规范化、持久化并发布一条 model deployment upsert。
pub async fn upsert_model_deployment_record(
    state: &ModelDeploymentRegistryState,
    mut record: ModelDeploymentRecord,
) -> Result<UpsertModelDeploymentResponse, Status> {
    let target_node_uuid = {
        let spec = record
            .spec
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("spec is required"))?;
        if spec.target_node_uuid.trim().is_empty() {
            return Err(Status::invalid_argument("spec.target_node_uuid is required"));
        }
        if spec.provider.trim().is_empty() {
            return Err(Status::invalid_argument("spec.provider is required"));
        }
        if spec.model.trim().is_empty() {
            return Err(Status::invalid_argument("spec.model is required"));
        }
        spec.target_node_uuid.clone()
    };

    let now_ms = chrono::Utc::now().timestamp_millis();
    let deployment_id = if record.deployment_id.trim().is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        record.deployment_id.clone()
    };

    let mut guard = state.records.write().await;
    let created_at_ms = guard
        .get(&deployment_id)
        .map(|r| r.created_at_ms)
        .filter(|v| *v > 0)
        .unwrap_or(now_ms);

    let new_registry_revision = state.watch.bump_revision();
    record.deployment_id = deployment_id.clone();
    record.revision = new_registry_revision;
    record.created_at_ms = created_at_ms;
    record.updated_at_ms = now_ms;
    if record.status.is_none() {
        record.status = Some(ModelDeploymentStatus {
            phase: ModelDeploymentPhase::Pending as i32,
            message: String::new(),
            updated_at_ms: now_ms,
        });
    }

    guard.insert(deployment_id.clone(), record);
    drop(guard);

    state
        .id_to_node
        .write()
        .await
        .insert(deployment_id.clone(), target_node_uuid);

    state
        .watch
        .push_event(ModelDeploymentEvent {
            revision: new_registry_revision,
            upserts: vec![deployment_id.clone()],
            deletes: Vec::new(),
        })
        .await;

    Ok(UpsertModelDeploymentResponse {
        revision: new_registry_revision,
        deployment_id,
    })
}

/// Delete one model deployment and emit the corresponding registry event.
/// 删除一条 model deployment 并发送对应 registry 事件。
pub async fn delete_model_deployment_record(
    state: &ModelDeploymentRegistryState,
    deployment_id: String,
) -> Result<DeleteModelDeploymentResponse, Status> {
    if deployment_id.trim().is_empty() {
        return Err(Status::invalid_argument("deployment_id is required"));
    }
    let existed = {
        let mut guard = state.records.write().await;
        guard.remove(&deployment_id).is_some()
    };
    if !existed {
        return Err(Status::not_found("deployment not found"));
    }

    {
        let mut map = state.id_to_node.write().await;
        map.remove(&deployment_id);
    }

    let new_registry_revision = state.watch.bump_revision();
    state
        .watch
        .push_event(ModelDeploymentEvent {
            revision: new_registry_revision,
            upserts: Vec::new(),
            deletes: vec![deployment_id],
        })
        .await;

    Ok(DeleteModelDeploymentResponse {
        revision: new_registry_revision,
    })
}

/// Apply one model deployment status report with optimistic revision semantics.
/// 以乐观 revision 语义应用一条 model deployment status 上报。
pub async fn report_model_deployment_status_update(
    state: &ModelDeploymentRegistryState,
    req: ReportModelDeploymentStatusRequest,
) -> Result<ReportModelDeploymentStatusResponse, Status> {
    if req.deployment_id.trim().is_empty() {
        return Err(Status::invalid_argument("deployment_id is required"));
    }
    if req.node_uuid.trim().is_empty() {
        return Err(Status::invalid_argument("node_uuid is required"));
    }
    let mut status = req
        .status
        .ok_or_else(|| Status::invalid_argument("status is required"))?;
    if status.updated_at_ms == 0 {
        status.updated_at_ms = chrono::Utc::now().timestamp_millis();
    }

    let mut guard = state.records.write().await;
    let Some(mut rec) = guard.get(&req.deployment_id).cloned() else {
        return Err(Status::not_found("deployment not found"));
    };
    let target_node_uuid = {
        let Some(spec) = rec.spec.as_ref() else {
            return Err(Status::failed_precondition("spec missing"));
        };
        if spec.target_node_uuid != req.node_uuid {
            return Err(Status::permission_denied("node_uuid mismatch"));
        }
        spec.target_node_uuid.clone()
    };
    let observed_record_revision = req.observed_revision;
    let current_record_revision = rec.revision;
    if observed_record_revision > 0 && observed_record_revision < current_record_revision {
        return Ok(ReportModelDeploymentStatusResponse { success: false });
    }

    let new_registry_revision = state.watch.bump_revision();
    rec.updated_at_ms = status.updated_at_ms;
    rec.status = Some(status);
    guard.insert(req.deployment_id.clone(), rec);
    drop(guard);

    state
        .id_to_node
        .write()
        .await
        .insert(req.deployment_id.clone(), target_node_uuid);

    state
        .watch
        .push_event(ModelDeploymentEvent {
            revision: new_registry_revision,
            upserts: vec![req.deployment_id],
            deletes: Vec::new(),
        })
        .await;

    Ok(ReportModelDeploymentStatusResponse { success: true })
}

/// Build the filtered model deployment watch stream for one target node.
/// 为指定 target node 构建过滤后的 model deployment watch 流。
pub async fn watch_model_deployments_filtered(
    state: Arc<ModelDeploymentRegistryState>,
    req: WatchModelDeploymentsRequest,
) -> Result<
    Pin<
        Box<
            dyn tokio_stream::Stream<Item = Result<WatchModelDeploymentsResponse, Status>>
                + Send
                + 'static,
        >,
    >,
    Status,
> {
    if req.target_node_uuid.trim().is_empty() {
        return Err(Status::invalid_argument("target_node_uuid is required"));
    }
    let node_filter = req.target_node_uuid.clone();
    let watch_cursor_revision = req.since_revision;

    let base = state.watch.watch(watch_cursor_revision, |e| e.revision).await?;
    let stream = base
        .then(move |r| {
            let node_filter = node_filter.clone();
            let state = state.clone();
            async move {
                match r {
                    Ok(mut event) => {
                        let map = state.id_to_node.read().await;
                        event
                            .upserts
                            .retain(|id| map.get(id).map(|n| n == &node_filter).unwrap_or(false));
                        event
                            .deletes
                            .retain(|id| map.get(id).map(|n| n == &node_filter).unwrap_or(true));
                        if event.upserts.is_empty() && event.deletes.is_empty() {
                            None
                        } else {
                            Some(Ok(WatchModelDeploymentsResponse { event: Some(event) }))
                        }
                    }
                    Err(e) => Some(Err(e)),
                }
            }
        })
        .filter_map(|x| x);

    Ok(Box::pin(stream))
}
