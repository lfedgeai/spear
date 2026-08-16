use axum::{extract::State, http::StatusCode, Json};
use std::collections::BTreeMap;
use tracing::debug;

use crate::spearlet::ai::credential_sync::global_credential_sync_status_snapshot;
use crate::spearlet::ai::dynamic_backend_registry::global_dynamic_backends;
use crate::spearlet::execution::ai::engine_holder;

use super::AppState;

/// Get statistics endpoint / 获取统计信息端点
/// GET /monitoring/stats
pub(super) async fn get_stats(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    debug!("GET /monitoring/stats");

    let stats = state.function_service.get_stats().await;
    let exec_stats = state.function_service.get_execution_manager().get_statistics();

    Ok(Json(serde_json::json!({
        "total_executions": exec_stats.total_executions,
        "successful_executions": exec_stats.successful_executions,
        "failed_executions": exec_stats.failed_executions,
        "active_executions": exec_stats.running_executions,
        "queue_size": exec_stats.queue_size,
        "pending_executions": exec_stats.pending_executions,
        "task_count": stats.task_count,
        "artifact_count": stats.artifact_count,
        "instance_count": stats.instance_count,
        "average_response_time_ms": stats.average_response_time_ms
    })))
}

/// Get current in-process AI backend registry snapshot / 获取当前进程内 AI backend registry 快照
/// GET /monitoring/ai/backends
pub(super) async fn get_ai_backends(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let holder = engine_holder::global().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let engine = holder.get();
    let snap = engine.debug_snapshot();
    let dynamic_registry = global_dynamic_backends();
    let by_source = dynamic_registry.snapshot_by_source();
    let dynamic_sources = dynamic_sources_json(&by_source);
    let conflicts = detect_dynamic_backend_conflicts(&by_source);
    Ok(Json(serde_json::json!({
        "engine_revision": holder.revision(),
        "ai_backend_assignment_controller": {
            "enabled": true,
        },
        "backend_reporter": {
            "report_interval_ms": state.config.ai.backend_report_interval_ms,
        },
        "dynamic_backend_registry": {
            "revision": dynamic_registry.revision(),
            "sources": dynamic_sources,
            "conflicts": conflicts,
        },
        "router": snap,
    })))
}

/// Render the dynamic backend registry grouped by source.
/// 渲染按 source 分组的动态 backend registry。
fn dynamic_sources_json(
    by_source: &std::collections::HashMap<
        crate::spearlet::ai::dynamic_backend_registry::DynamicBackendSource,
        Vec<crate::proto::sms::BackendInfo>,
    >,
) -> Vec<serde_json::Value> {
    let mut entries = by_source
        .iter()
        .map(|(source, backends)| {
            let mut names = backends
                .iter()
                .filter_map(|backend| backend.spec.as_ref().map(|spec| spec.name.clone()))
                .filter(|name| !name.trim().is_empty())
                .collect::<Vec<_>>();
            names.sort();
            serde_json::json!({
                "source": source.as_str(),
                "count": names.len(),
                "names": names,
            })
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        left["source"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["source"].as_str().unwrap_or_default())
    });
    entries
}

/// Detect duplicate runtime backend names emitted by multiple sources.
/// 检测由多个 source 同时产出的同名运行时 backend。
fn detect_dynamic_backend_conflicts(
    by_source: &std::collections::HashMap<
        crate::spearlet::ai::dynamic_backend_registry::DynamicBackendSource,
        Vec<crate::proto::sms::BackendInfo>,
    >,
) -> Vec<serde_json::Value> {
    let mut names_to_sources: BTreeMap<String, Vec<&'static str>> = BTreeMap::new();
    for (source, backends) in by_source.iter() {
        for backend in backends {
            let Some(spec) = backend.spec.as_ref() else {
                continue;
            };
            let name = spec.name.trim();
            if name.is_empty() {
                continue;
            }
            let entry = names_to_sources.entry(name.to_string()).or_default();
            let label = source.as_str();
            if !entry.contains(&label) {
                entry.push(label);
            }
        }
    }

    names_to_sources
        .into_iter()
        .filter_map(|(name, mut sources)| {
            if sources.len() <= 1 {
                return None;
            }
            sources.sort();
            Some(serde_json::json!({
                "name": name,
                "sources": sources,
                "message": format!("backend {name} is simultaneously provided by multiple sources"),
            }))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::sms::{BackendInfo, BackendSpec, BackendStatus};
    use crate::spearlet::ai::dynamic_backend_registry::DynamicBackendSource;
    use std::collections::HashMap;

    fn backend(name: &str) -> BackendInfo {
        BackendInfo {
            spec: Some(BackendSpec {
                name: name.to_string(),
                kind: "openai-compatible".to_string(),
                operations: vec!["chat".to_string()],
                features: vec![],
                transports: vec!["http".to_string()],
                weight: 1,
                priority: 0,
                base_url: "https://example.com".to_string(),
                provider: "openai".to_string(),
                model: "gpt-4.1".to_string(),
                hosting: crate::proto::sms::BackendHosting::Remote as i32,
                credential_ref: String::new(),
                origin: crate::proto::sms::BackendOrigin::Sms as i32,
                deployment_id: String::new(),
            }),
            status: BackendStatus::Available as i32,
            status_reason: String::new(),
        }
    }

    #[test]
    fn does_not_flag_unique_names_as_conflicts() {
        let mut by_source = HashMap::new();
        by_source.insert(
            DynamicBackendSource::AiControlPlane,
            vec![backend("cp-only")],
        );

        let conflicts = detect_dynamic_backend_conflicts(&by_source);

        assert!(conflicts.is_empty());
    }
}

/// Get current in-process credential sync snapshot / 获取当前进程内 credential 同步快照
/// GET /monitoring/ai/credentials
pub(super) async fn get_ai_credentials() -> Result<Json<serde_json::Value>, StatusCode> {
    let snapshot = global_credential_sync_status_snapshot();
    Ok(Json(serde_json::json!({
        "credential_sync": snapshot,
    })))
}

/// Get health status endpoint / 获取健康状态端点
/// GET /monitoring/health
pub(super) async fn get_health_status(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    debug!("GET /monitoring/health");

    let health = state.health_service.get_health_status().await;
    let stats = state.function_service.get_stats().await;

    Ok(Json(serde_json::json!({
        "status": health.status,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "details": {
            "node_name": state.config.node_name,
            "object_count": health.object_count,
            "total_object_size": health.total_object_size,
            "pinned_object_count": health.pinned_object_count,
            "task_count": health.task_count,
            "execution_count": health.execution_count,
            "running_executions": health.running_executions,
            "artifact_count": stats.artifact_count,
            "instance_count": stats.instance_count
        }
    })))
}
