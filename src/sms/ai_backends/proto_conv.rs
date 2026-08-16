//! Proto/domain conversions for unified AI backends / 统一 AI backend 的 proto 与领域转换

use std::collections::BTreeMap;

use prost_types::{value::Kind as ProstValueKind, Struct as ProstStruct, Value as ProstValue};

use crate::proto::sms::{
    AiBackendDesiredState, AiBackendHosting, AiBackendManagementMode,
    AiBackendNodeStatus as ProtoAiBackendNodeStatus, AiBackendNodeStatusRecord as ProtoAiBackendNodeStatusRecord,
    AiBackendPlacementRecord as ProtoAiBackendPlacementRecord, AiBackendRecord as ProtoAiBackendRecord,
    BackendSpec,
};
use crate::sms::ai_backends::model::{
    AiBackendDesiredStateModel, AiBackendHostingModel, AiBackendManagementModeModel,
    AiBackendNodeRuntimeStatusModel, AiBackendNodeStatusRecordModel, AiBackendPlacementRecordModel,
    AiBackendRecordModel, AiBackendSpecModel,
};
use crate::sms::services::error::SmsError;

/// Convert a domain backend record into a proto record / 把领域 backend 记录转换为 proto 记录
pub fn proto_backend_from_domain(record: &AiBackendRecordModel) -> ProtoAiBackendRecord {
    ProtoAiBackendRecord {
        backend_id: record.backend_id.clone(),
        display_name: record.display_name.clone(),
        provider: record.provider.clone(),
        model: record.model.clone(),
        hosting: proto_hosting_from_domain(record.hosting) as i32,
        backend_kind: record.backend_kind.clone(),
        desired_state: proto_desired_state_from_domain(record.desired_state) as i32,
        management_mode: proto_management_mode_from_domain(record.management_mode) as i32,
        credential_ref: record.credential_ref.clone(),
        spec: Some(proto_backend_spec_from_domain(&record.spec)),
        labels: record.labels.clone().into_iter().collect(),
        metadata: Some(proto_struct_from_json(&record.metadata)),
        generation: record.generation,
        created_at_ms: record.created_at_ms,
        updated_at_ms: record.updated_at_ms,
    }
}

/// Convert a proto backend record into a domain record / 把 proto backend 记录转换为领域记录
pub fn domain_backend_from_proto(record: &ProtoAiBackendRecord) -> Result<AiBackendRecordModel, SmsError> {
    Ok(AiBackendRecordModel {
        backend_id: record.backend_id.clone(),
        display_name: record.display_name.clone(),
        provider: record.provider.clone(),
        model: record.model.clone(),
        hosting: domain_hosting_from_proto(record.hosting)?,
        backend_kind: record.backend_kind.clone(),
        desired_state: domain_desired_state_from_proto(record.desired_state)?,
        management_mode: domain_management_mode_from_proto(record.management_mode)?,
        credential_ref: record.credential_ref.clone(),
        spec: domain_backend_spec_from_proto(
            record
                .spec
                .as_ref()
                .ok_or_else(|| SmsError::InvalidRequest("missing backend spec".to_string()))?,
        ),
        labels: record.labels.clone().into_iter().collect::<BTreeMap<_, _>>(),
        metadata: json_from_proto_struct(record.metadata.as_ref()),
        generation: record.generation,
        created_at_ms: record.created_at_ms,
        updated_at_ms: record.updated_at_ms,
    })
}

/// Convert a domain placement record into a proto placement / 把领域 placement 记录转换为 proto placement
pub fn proto_placement_from_domain(
    record: &AiBackendPlacementRecordModel,
) -> ProtoAiBackendPlacementRecord {
    ProtoAiBackendPlacementRecord {
        placement_id: record.placement_id.clone(),
        backend_id: record.backend_id.clone(),
        node_uuid: record.node_uuid.clone(),
        desired_state: proto_desired_state_from_domain(record.desired_state) as i32,
        weight_override: record.weight_override,
        priority_override: record.priority_override,
        generation: record.generation,
        created_at_ms: record.created_at_ms,
        updated_at_ms: record.updated_at_ms,
    }
}

/// Convert a proto placement record into a domain placement / 把 proto placement 记录转换为领域 placement
pub fn domain_placement_from_proto(
    record: &ProtoAiBackendPlacementRecord,
) -> Result<AiBackendPlacementRecordModel, SmsError> {
    Ok(AiBackendPlacementRecordModel {
        placement_id: record.placement_id.clone(),
        backend_id: record.backend_id.clone(),
        node_uuid: record.node_uuid.clone(),
        desired_state: domain_desired_state_from_proto(record.desired_state)?,
        weight_override: record.weight_override,
        priority_override: record.priority_override,
        generation: record.generation,
        created_at_ms: record.created_at_ms,
        updated_at_ms: record.updated_at_ms,
    })
}

/// Convert a domain node status record into a proto status / 把领域节点状态记录转换为 proto 状态
pub fn proto_status_from_domain(
    record: &AiBackendNodeStatusRecordModel,
) -> ProtoAiBackendNodeStatusRecord {
    ProtoAiBackendNodeStatusRecord {
        backend_id: record.backend_id.clone(),
        node_uuid: record.node_uuid.clone(),
        observed_generation: record.observed_generation,
        status: proto_node_status_from_domain(record.status) as i32,
        status_reason: record.status_reason.clone(),
        runtime_backend_name: record.runtime_backend_name.clone(),
        endpoint: record.endpoint.clone(),
        available: record.available,
        operations: record.operations.clone(),
        features: record.features.clone(),
        transports: record.transports.clone(),
        last_heartbeat_at_ms: record.last_heartbeat_at_ms,
    }
}

/// Convert a proto node status record into a domain status / 把 proto 节点状态记录转换为领域状态
pub fn domain_status_from_proto(
    record: &ProtoAiBackendNodeStatusRecord,
) -> Result<AiBackendNodeStatusRecordModel, SmsError> {
    Ok(AiBackendNodeStatusRecordModel {
        backend_id: record.backend_id.clone(),
        node_uuid: record.node_uuid.clone(),
        observed_generation: record.observed_generation,
        status: domain_node_status_from_proto(record.status)?,
        status_reason: record.status_reason.clone(),
        runtime_backend_name: record.runtime_backend_name.clone(),
        endpoint: record.endpoint.clone(),
        available: record.available,
        operations: record.operations.clone(),
        features: record.features.clone(),
        transports: record.transports.clone(),
        last_heartbeat_at_ms: record.last_heartbeat_at_ms,
    })
}

/// Convert a domain backend spec into proto / 把领域 backend spec 转换为 proto
pub fn proto_backend_spec_from_domain(spec: &AiBackendSpecModel) -> BackendSpec {
    BackendSpec {
        name: spec.name.clone(),
        kind: spec.kind.clone(),
        operations: spec.operations.clone(),
        features: spec.features.clone(),
        transports: spec.transports.clone(),
        weight: spec.weight,
        priority: spec.priority,
        base_url: spec.base_url.clone(),
        provider: spec.provider.clone(),
        model: spec.model.clone(),
        hosting: 0,
        credential_ref: spec.credential_ref.clone(),
        origin: spec.origin,
        deployment_id: spec.deployment_id.clone(),
    }
}

/// Convert a proto backend spec into domain / 把 proto backend spec 转换为领域
pub fn domain_backend_spec_from_proto(spec: &BackendSpec) -> AiBackendSpecModel {
    AiBackendSpecModel {
        name: spec.name.clone(),
        kind: spec.kind.clone(),
        operations: spec.operations.clone(),
        features: spec.features.clone(),
        transports: spec.transports.clone(),
        weight: spec.weight,
        priority: spec.priority,
        base_url: spec.base_url.clone(),
        provider: spec.provider.clone(),
        model: spec.model.clone(),
        credential_ref: spec.credential_ref.clone(),
        origin: spec.origin,
        deployment_id: spec.deployment_id.clone(),
    }
}

/// Convert a domain JSON object into proto Struct / 把领域 JSON 对象转换为 proto Struct
fn proto_struct_from_json(value: &serde_json::Value) -> ProstStruct {
    let fields = value
        .as_object()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|(key, value)| (key, proto_value_from_json(&value)))
        .collect();
    ProstStruct { fields }
}

/// Convert proto Struct into domain JSON / 把 proto Struct 转换为领域 JSON
fn json_from_proto_struct(value: Option<&ProstStruct>) -> serde_json::Value {
    let fields = value
        .map(|inner| {
            inner
                .fields
                .iter()
                .map(|(key, value)| (key.clone(), json_value_from_proto(value)))
                .collect()
        })
        .unwrap_or_default();
    serde_json::Value::Object(fields)
}

/// Convert JSON into proto Value / 把 JSON 转换为 proto Value
fn proto_value_from_json(value: &serde_json::Value) -> ProstValue {
    let kind = match value {
        serde_json::Value::Null => ProstValueKind::NullValue(0),
        serde_json::Value::Bool(inner) => ProstValueKind::BoolValue(*inner),
        serde_json::Value::Number(inner) => ProstValueKind::NumberValue(inner.as_f64().unwrap_or_default()),
        serde_json::Value::String(inner) => ProstValueKind::StringValue(inner.clone()),
        serde_json::Value::Array(inner) => ProstValueKind::ListValue(prost_types::ListValue {
            values: inner.iter().map(proto_value_from_json).collect(),
        }),
        serde_json::Value::Object(_) => ProstValueKind::StructValue(proto_struct_from_json(value)),
    };
    ProstValue { kind: Some(kind) }
}

/// Convert proto Value into JSON / 把 proto Value 转换为 JSON
fn json_value_from_proto(value: &ProstValue) -> serde_json::Value {
    match value.kind.as_ref() {
        Some(ProstValueKind::NullValue(_)) | None => serde_json::Value::Null,
        Some(ProstValueKind::BoolValue(inner)) => serde_json::Value::Bool(*inner),
        Some(ProstValueKind::NumberValue(inner)) => number_json_from_f64(*inner),
        Some(ProstValueKind::StringValue(inner)) => serde_json::Value::String(inner.clone()),
        Some(ProstValueKind::StructValue(inner)) => json_from_proto_struct(Some(inner)),
        Some(ProstValueKind::ListValue(inner)) => {
            serde_json::Value::Array(inner.values.iter().map(json_value_from_proto).collect())
        }
    }
}

/// Convert an f64 from protobuf Struct into the narrowest JSON number / 把 protobuf Struct 的 f64 转为最窄的 JSON 数字
fn number_json_from_f64(value: f64) -> serde_json::Value {
    if value.is_finite() && value.fract() == 0.0 {
        let text = format!("{value:.0}");
        if let Ok(integer) = text.parse::<i64>() {
            return serde_json::json!(integer);
        }
        if let Ok(integer) = text.parse::<u64>() {
            return serde_json::json!(integer);
        }
    }
    serde_json::json!(value)
}

/// Convert a domain hosting enum into proto / 把领域 hosting 枚举转换为 proto
fn proto_hosting_from_domain(value: AiBackendHostingModel) -> AiBackendHosting {
    match value {
        AiBackendHostingModel::Remote => AiBackendHosting::Remote,
        AiBackendHostingModel::Local => AiBackendHosting::Local,
    }
}

/// Convert a proto hosting enum into domain / 把 proto hosting 枚举转换为领域
fn domain_hosting_from_proto(value: i32) -> Result<AiBackendHostingModel, SmsError> {
    match AiBackendHosting::try_from(value).unwrap_or(AiBackendHosting::Unspecified) {
        AiBackendHosting::Remote => Ok(AiBackendHostingModel::Remote),
        AiBackendHosting::Local => Ok(AiBackendHostingModel::Local),
        AiBackendHosting::Unspecified => Err(SmsError::InvalidRequest(
            "invalid ai backend hosting".to_string(),
        )),
    }
}

/// Convert a domain desired-state enum into proto / 把领域期望状态枚举转换为 proto
fn proto_desired_state_from_domain(value: AiBackendDesiredStateModel) -> AiBackendDesiredState {
    match value {
        AiBackendDesiredStateModel::Enabled => AiBackendDesiredState::Enabled,
        AiBackendDesiredStateModel::Disabled => AiBackendDesiredState::Disabled,
    }
}

/// Convert a proto desired-state enum into domain / 把 proto 期望状态枚举转换为领域
pub fn domain_desired_state_from_proto(value: i32) -> Result<AiBackendDesiredStateModel, SmsError> {
    match AiBackendDesiredState::try_from(value).unwrap_or(AiBackendDesiredState::Unspecified) {
        AiBackendDesiredState::Enabled => Ok(AiBackendDesiredStateModel::Enabled),
        AiBackendDesiredState::Disabled => Ok(AiBackendDesiredStateModel::Disabled),
        AiBackendDesiredState::Unspecified => Err(SmsError::InvalidRequest(
            "invalid ai backend desired state".to_string(),
        )),
    }
}

/// Convert a domain management-mode enum into proto / 把领域管理模式枚举转换为 proto
fn proto_management_mode_from_domain(
    value: AiBackendManagementModeModel,
) -> AiBackendManagementMode {
    match value {
        AiBackendManagementModeModel::SmsRemote => AiBackendManagementMode::SmsRemote,
        AiBackendManagementModeModel::SmsLocal => AiBackendManagementMode::SmsLocal,
    }
}

/// Convert a proto management-mode enum into domain / 把 proto 管理模式枚举转换为领域
fn domain_management_mode_from_proto(
    value: i32,
) -> Result<AiBackendManagementModeModel, SmsError> {
    match AiBackendManagementMode::try_from(value)
        .unwrap_or(AiBackendManagementMode::Unspecified)
    {
        AiBackendManagementMode::SmsRemote => Ok(AiBackendManagementModeModel::SmsRemote),
        AiBackendManagementMode::SmsLocal => Ok(AiBackendManagementModeModel::SmsLocal),
        AiBackendManagementMode::Unspecified => Err(SmsError::InvalidRequest(
            "invalid ai backend management mode".to_string(),
        )),
    }
}

/// Convert a domain node status enum into proto / 把领域节点状态枚举转换为 proto
fn proto_node_status_from_domain(
    value: AiBackendNodeRuntimeStatusModel,
) -> ProtoAiBackendNodeStatus {
    match value {
        AiBackendNodeRuntimeStatusModel::Pending => ProtoAiBackendNodeStatus::Pending,
        AiBackendNodeRuntimeStatusModel::Reconciling => ProtoAiBackendNodeStatus::Reconciling,
        AiBackendNodeRuntimeStatusModel::Ready => ProtoAiBackendNodeStatus::Ready,
        AiBackendNodeRuntimeStatusModel::Degraded => ProtoAiBackendNodeStatus::Degraded,
        AiBackendNodeRuntimeStatusModel::Error => ProtoAiBackendNodeStatus::Error,
        AiBackendNodeRuntimeStatusModel::Disabled => ProtoAiBackendNodeStatus::Disabled,
    }
}

/// Convert a proto node status enum into domain / 把 proto 节点状态枚举转换为领域
fn domain_node_status_from_proto(
    value: i32,
) -> Result<AiBackendNodeRuntimeStatusModel, SmsError> {
    match ProtoAiBackendNodeStatus::try_from(value)
        .unwrap_or(ProtoAiBackendNodeStatus::Unspecified)
    {
        ProtoAiBackendNodeStatus::Pending => Ok(AiBackendNodeRuntimeStatusModel::Pending),
        ProtoAiBackendNodeStatus::Reconciling => Ok(AiBackendNodeRuntimeStatusModel::Reconciling),
        ProtoAiBackendNodeStatus::Ready => Ok(AiBackendNodeRuntimeStatusModel::Ready),
        ProtoAiBackendNodeStatus::Degraded => Ok(AiBackendNodeRuntimeStatusModel::Degraded),
        ProtoAiBackendNodeStatus::Error => Ok(AiBackendNodeRuntimeStatusModel::Error),
        ProtoAiBackendNodeStatus::Disabled => Ok(AiBackendNodeRuntimeStatusModel::Disabled),
        ProtoAiBackendNodeStatus::Unspecified => Err(SmsError::InvalidRequest(
            "invalid ai backend node status".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sms::ai_backends::model::{
        AiBackendDesiredStateModel, AiBackendHostingModel, AiBackendManagementModeModel,
    };

    #[test]
    fn backend_proto_roundtrip_preserves_metadata() {
        let domain = AiBackendRecordModel {
            backend_id: "backend-1".to_string(),
            display_name: "Backend 1".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4.1".to_string(),
            hosting: AiBackendHostingModel::Remote,
            backend_kind: "openai-compatible".to_string(),
            desired_state: AiBackendDesiredStateModel::Enabled,
            management_mode: AiBackendManagementModeModel::SmsRemote,
            credential_ref: Some("cred-1".to_string()),
            spec: AiBackendSpecModel {
                name: "backend-1".to_string(),
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
            labels: BTreeMap::from([(String::from("env"), String::from("prod"))]),
            metadata: serde_json::json!({"tier":"critical","limits":{"qps":10}}),
            generation: 1,
            created_at_ms: 1,
            updated_at_ms: 1,
        };

        let proto = proto_backend_from_domain(&domain);
        let roundtrip = domain_backend_from_proto(&proto).unwrap();

        assert_eq!(roundtrip.metadata, domain.metadata);
        assert_eq!(roundtrip.labels, domain.labels);
        assert_eq!(roundtrip.backend_id, domain.backend_id);
    }
}
