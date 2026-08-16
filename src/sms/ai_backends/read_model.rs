//! Read-model aggregation for unified AI backends / 统一 AI backend 的只读模型聚合

use std::collections::{BTreeMap, BTreeSet};

use crate::sms::ai_backends::model::{
    AiBackendHostingModel, AiBackendNodeRuntimeStatusModel, AiBackendNodeStatusRecordModel,
    AiBackendPlacementRecordModel, AiBackendRecordModel, AiModelView, AiModelViewInstance,
};

/// Build `AiModelView` records from control-plane state / 从控制面状态构建 `AiModelView`
pub fn build_ai_model_views(
    backends: &[AiBackendRecordModel],
    placements: &[AiBackendPlacementRecordModel],
    statuses: &[AiBackendNodeStatusRecordModel],
) -> Vec<AiModelView> {
    let placements_by_backend = index_placements_by_backend(placements);
    let statuses_by_backend_node = index_statuses_by_backend_node(statuses);
    let mut views: BTreeMap<(String, String, AiBackendHostingModel), AggregationState> =
        BTreeMap::new();

    for backend in backends {
        let key = (
            backend.provider.clone(),
            backend.model.clone(),
            backend.hosting,
        );
        let state = views
            .entry(key)
            .or_insert_with(|| AggregationState::new(backend));
        state.backend_ids.insert(backend.backend_id.clone());
        state.operations.extend(backend.spec.operations.iter().cloned());
        state.features.extend(backend.spec.features.iter().cloned());
        state.transports.extend(backend.spec.transports.iter().cloned());

        if let Some(backend_placements) = placements_by_backend.get(&backend.backend_id) {
            for placement in backend_placements {
                let status_key = (backend.backend_id.clone(), placement.node_uuid.clone());
                let status = statuses_by_backend_node.get(&status_key);
                state.total_nodes.insert(placement.node_uuid.clone());
                if backend.is_enabled() && placement.is_enabled() {
                    state.enabled_nodes.insert(placement.node_uuid.clone());
                }
                if matches!(
                    status.map(|value| value.status),
                    Some(AiBackendNodeRuntimeStatusModel::Ready)
                ) && status.map(|value| value.available).unwrap_or(false)
                {
                    state.ready_nodes.insert(placement.node_uuid.clone());
                }
                state.instances.push(AiModelViewInstance {
                    backend_id: backend.backend_id.clone(),
                    node_uuid: placement.node_uuid.clone(),
                    placement_state: placement.desired_state,
                    backend_state: backend.desired_state,
                    runtime_status: status.map(|value| value.status),
                    runtime_backend_name: status.and_then(non_empty_owned_runtime_name),
                    endpoint: status.and_then(non_empty_owned_endpoint),
                    available: status.map(|value| value.available).unwrap_or(false),
                });
            }
        }
    }

    let mut result = views
        .into_values()
        .map(AggregationState::into_view)
        .collect::<Vec<_>>();
    result.sort_by(|left, right| {
        (&left.provider, &left.model, left.hosting).cmp(&(&right.provider, &right.model, right.hosting))
    });
    result
}

/// Aggregation bucket used while building the read model / 构建只读模型时使用的聚合桶
struct AggregationState {
    view: AiModelView,
    backend_ids: BTreeSet<String>,
    operations: BTreeSet<String>,
    features: BTreeSet<String>,
    transports: BTreeSet<String>,
    enabled_nodes: BTreeSet<String>,
    ready_nodes: BTreeSet<String>,
    total_nodes: BTreeSet<String>,
    instances: Vec<AiModelViewInstance>,
}

impl AggregationState {
    /// Create a new aggregation bucket / 创建新的聚合桶
    fn new(backend: &AiBackendRecordModel) -> Self {
        Self {
            view: AiModelView::new(
                backend.provider.clone(),
                backend.model.clone(),
                backend.hosting,
            ),
            backend_ids: BTreeSet::new(),
            operations: BTreeSet::new(),
            features: BTreeSet::new(),
            transports: BTreeSet::new(),
            enabled_nodes: BTreeSet::new(),
            ready_nodes: BTreeSet::new(),
            total_nodes: BTreeSet::new(),
            instances: Vec::new(),
        }
    }

    /// Finish one bucket into a stable view / 把单个聚合桶收束为稳定视图
    fn into_view(mut self) -> AiModelView {
        self.instances.sort_by(|left, right| {
            (&left.backend_id, &left.node_uuid).cmp(&(&right.backend_id, &right.node_uuid))
        });
        self.view.instances = self.instances;
        self.view.enabled_nodes = self.enabled_nodes.len();
        self.view.ready_nodes = self.ready_nodes.len();
        self.view.total_nodes = self.total_nodes.len();
        self.view.finalize_sets(
            self.backend_ids,
            self.operations,
            self.features,
            self.transports,
        );
        self.view
    }
}

/// Build an index for placements by backend / 构建按 backend 组织的 placement 索引
fn index_placements_by_backend(
    placements: &[AiBackendPlacementRecordModel],
) -> BTreeMap<String, Vec<AiBackendPlacementRecordModel>> {
    let mut index = BTreeMap::new();
    for placement in placements {
        index
            .entry(placement.backend_id.clone())
            .or_insert_with(Vec::new)
            .push(placement.clone());
    }
    index
}

/// Build an index for statuses by backend and node / 构建按 backend 与节点组织的状态索引
fn index_statuses_by_backend_node(
    statuses: &[AiBackendNodeStatusRecordModel],
) -> BTreeMap<(String, String), AiBackendNodeStatusRecordModel> {
    let mut index = BTreeMap::new();
    for status in statuses {
        index.insert(
            (status.backend_id.clone(), status.node_uuid.clone()),
            status.clone(),
        );
    }
    index
}

/// Lift a runtime backend name into an optional owned string / 把运行时 backend 名称提升为可选字符串
fn non_empty_owned_runtime_name(status: &AiBackendNodeStatusRecordModel) -> Option<String> {
    let value = status.runtime_backend_name.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

/// Lift an endpoint into an optional owned string / 把端点提升为可选字符串
fn non_empty_owned_endpoint(status: &AiBackendNodeStatusRecordModel) -> Option<String> {
    let value = status.endpoint.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sms::ai_backends::model::{
        AiBackendDesiredStateModel, AiBackendHostingModel, AiBackendManagementModeModel,
        AiBackendSpecModel,
    };
    use std::collections::BTreeMap;

    fn sample_backend(backend_id: &str) -> AiBackendRecordModel {
        AiBackendRecordModel {
            backend_id: backend_id.to_string(),
            display_name: format!("Backend {backend_id}"),
            provider: "openai".to_string(),
            model: "gpt-4.1".to_string(),
            hosting: AiBackendHostingModel::Remote,
            backend_kind: "openai-compatible".to_string(),
            desired_state: AiBackendDesiredStateModel::Enabled,
            management_mode: AiBackendManagementModeModel::SmsRemote,
            credential_ref: Some("cred-1".to_string()),
            spec: AiBackendSpecModel {
                name: format!("backend-{backend_id}"),
                kind: "openai-compatible".to_string(),
                operations: vec!["chat".to_string()],
                features: vec!["stream".to_string()],
                transports: vec!["http".to_string()],
                weight: 1,
                priority: 0,
                base_url: "https://example.com".to_string(),
                provider: "openai".to_string(),
                model: "gpt-4.1".to_string(),
                credential_ref: "cred-1".to_string(),
                origin: 0,
                deployment_id: String::new(),
            },
            labels: BTreeMap::new(),
            metadata: serde_json::json!({}),
            generation: 1,
            created_at_ms: 1,
            updated_at_ms: 1,
        }
    }

    #[test]
    fn aggregates_counts_by_unique_nodes() {
        let backends = vec![sample_backend("b1"), sample_backend("b2")];
        let placements = vec![
            AiBackendPlacementRecordModel {
                placement_id: "p1".to_string(),
                backend_id: "b1".to_string(),
                node_uuid: "node-1".to_string(),
                desired_state: AiBackendDesiredStateModel::Enabled,
                weight_override: None,
                priority_override: None,
                generation: 1,
                created_at_ms: 1,
                updated_at_ms: 1,
            },
            AiBackendPlacementRecordModel {
                placement_id: "p2".to_string(),
                backend_id: "b2".to_string(),
                node_uuid: "node-1".to_string(),
                desired_state: AiBackendDesiredStateModel::Enabled,
                weight_override: None,
                priority_override: None,
                generation: 1,
                created_at_ms: 1,
                updated_at_ms: 1,
            },
        ];
        let statuses = vec![
            AiBackendNodeStatusRecordModel {
                backend_id: "b1".to_string(),
                node_uuid: "node-1".to_string(),
                observed_generation: 1,
                status: AiBackendNodeRuntimeStatusModel::Ready,
                status_reason: String::new(),
                runtime_backend_name: "backend-b1".to_string(),
                endpoint: "https://example.com/a".to_string(),
                available: true,
                operations: vec!["chat".to_string()],
                features: vec!["stream".to_string()],
                transports: vec!["http".to_string()],
                last_heartbeat_at_ms: 1,
            },
            AiBackendNodeStatusRecordModel {
                backend_id: "b2".to_string(),
                node_uuid: "node-1".to_string(),
                observed_generation: 1,
                status: AiBackendNodeRuntimeStatusModel::Ready,
                status_reason: String::new(),
                runtime_backend_name: "backend-b2".to_string(),
                endpoint: "https://example.com/b".to_string(),
                available: true,
                operations: vec!["chat".to_string()],
                features: vec!["stream".to_string()],
                transports: vec!["http".to_string()],
                last_heartbeat_at_ms: 1,
            },
        ];

        let result = build_ai_model_views(&backends, &placements, &statuses);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].enabled_nodes, 1);
        assert_eq!(result[0].ready_nodes, 1);
        assert_eq!(result[0].total_nodes, 1);
        assert_eq!(result[0].backend_ids.len(), 2);
        assert_eq!(result[0].instances.len(), 2);
    }
}
