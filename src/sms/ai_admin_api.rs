//! Shared AI backend admin API mapping helpers for SMS.
//! SMS 的 AI backend 管理端 API 共享映射辅助模块。
//!
//! This module centralizes typed admin response models for AI backend related
//! endpoints so Web Admin handlers can focus on orchestration.
//! 此模块集中管理 AI backend 相关接口的类型化管理端响应模型，让 Web Admin handler 专注于流程编排。

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminAiBackendSpecResponse {
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) operations: Vec<String>,
    pub(crate) features: Vec<String>,
    pub(crate) transports: Vec<String>,
    pub(crate) weight: u32,
    pub(crate) priority: i32,
    pub(crate) base_url: String,
    pub(crate) provider: String,
    pub(crate) model: String,
    pub(crate) credential_ref: Option<String>,
    pub(crate) origin: i32,
    pub(crate) deployment_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminAiBackendRecordResponse {
    pub(crate) backend_id: String,
    pub(crate) display_name: String,
    pub(crate) provider: String,
    pub(crate) model: String,
    pub(crate) hosting: String,
    pub(crate) backend_kind: String,
    pub(crate) desired_state: String,
    pub(crate) management_mode: String,
    pub(crate) credential_ref: Option<String>,
    pub(crate) spec: Option<AdminAiBackendSpecResponse>,
    pub(crate) local: Option<AdminAiBackendLocalProviderResponse>,
    pub(crate) remote: Option<AdminAiBackendRemoteProviderResponse>,
    pub(crate) labels: std::collections::HashMap<String, String>,
    pub(crate) metadata: serde_json::Value,
    pub(crate) generation: u64,
    pub(crate) created_at_ms: i64,
    pub(crate) updated_at_ms: i64,
}

/// Structured local provider config exposed by admin read APIs.
/// 管理端读接口暴露的结构化 local provider 配置。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "provider_family", content = "config", rename_all = "snake_case")]
pub(crate) enum AdminAiBackendLocalProviderResponse {
    LlamaCpp(AdminAiBackendLlamaCppLocalConfigResponse),
    Vllm(AdminAiBackendVllmLocalConfigResponse),
}

/// Structured llama.cpp local config exposed by admin read APIs.
/// 管理端读接口暴露的结构化 llama.cpp local 配置。
#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminAiBackendLlamaCppLocalConfigResponse {
    pub(crate) model_url: Option<String>,
    pub(crate) model_path: Option<String>,
    pub(crate) skip_download: Option<bool>,
    pub(crate) download_timeout_s: Option<u64>,
    pub(crate) server_mode: Option<String>,
    pub(crate) server_cmd: Option<String>,
    pub(crate) server_cmd_args: Option<String>,
    pub(crate) threads: Option<u32>,
    pub(crate) ctx_size: Option<u32>,
    pub(crate) ready_probe: Option<String>,
    pub(crate) start_timeout_s: Option<u64>,
}

/// Structured vLLM local config exposed by admin read APIs.
/// 管理端读接口暴露的结构化 vLLM local 配置。
#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminAiBackendVllmLocalConfigResponse {
    pub(crate) mode: Option<String>,
    pub(crate) managed_externally: Option<bool>,
}

/// Structured remote provider config exposed by admin read APIs.
/// 管理端读接口暴露的结构化 remote provider 配置。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "provider_family", content = "config", rename_all = "snake_case")]
pub(crate) enum AdminAiBackendRemoteProviderResponse {
    OpenAiCompatible(AdminAiBackendOpenAiCompatibleRemoteConfigResponse),
    Ollama(AdminAiBackendOllamaRemoteConfigResponse),
}

/// Structured OpenAI-compatible remote config exposed by admin read APIs.
/// 管理端读接口暴露的结构化 OpenAI-compatible remote 配置。
#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminAiBackendOpenAiCompatibleRemoteConfigResponse {
    pub(crate) base_url: Option<String>,
    pub(crate) credential_ref: Option<String>,
    pub(crate) operations: Vec<String>,
    pub(crate) features: Vec<String>,
    pub(crate) transports: Vec<String>,
}

/// Structured Ollama remote config exposed by admin read APIs.
/// 管理端读接口暴露的结构化 Ollama remote 配置。
#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminAiBackendOllamaRemoteConfigResponse {
    pub(crate) base_url: Option<String>,
    pub(crate) operations: Vec<String>,
    pub(crate) features: Vec<String>,
    pub(crate) transports: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminAiBackendListResponse {
    pub(crate) success: bool,
    pub(crate) backends: Vec<AdminAiBackendRecordResponse>,
    pub(crate) total_count: i32,
    pub(crate) message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminAiBackendDetailResponse {
    pub(crate) success: bool,
    pub(crate) found: bool,
    pub(crate) backend: Option<AdminAiBackendRecordResponse>,
    pub(crate) message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminAiBackendMutationResponse {
    pub(crate) success: bool,
    pub(crate) backend: Option<AdminAiBackendRecordResponse>,
    pub(crate) deleted: Option<bool>,
    pub(crate) message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminAiBackendPreflightCheckResponse {
    pub(crate) name: String,
    pub(crate) ok: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "source_kind", rename_all = "snake_case")]
pub(crate) enum AdminAiBackendLocalModelPreflightSourceResponse {
    Unavailable,
    ModelPath {
        effective_model_path: String,
    },
    ModelUrl {
        effective_model_path: String,
        final_url: String,
        http_status: u16,
        content_length: Option<u64>,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "outcome_kind", rename_all = "snake_case")]
pub(crate) enum AdminAiBackendRemoteProviderPreflightOutcomeResponse {
    Success {
        phase: String,
        resolved_endpoint: Option<String>,
        http_status: Option<u16>,
        checks: Vec<AdminAiBackendPreflightCheckResponse>,
        auth_valid: Option<bool>,
        model_accessible: Option<bool>,
    },
    Failure {
        phase: String,
        error_code: String,
        resolved_endpoint: Option<String>,
        http_status: Option<u16>,
        provider_code: Option<String>,
        checks: Vec<AdminAiBackendPreflightCheckResponse>,
        auth_valid: Option<bool>,
        model_accessible: Option<bool>,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "outcome_kind", rename_all = "snake_case")]
pub(crate) enum AdminAiBackendOpenAiCompatiblePreflightOutcomeResponse {
    ConnectivityVerified {
        resolved_endpoint: Option<String>,
        http_status: Option<u16>,
        checks: Vec<AdminAiBackendPreflightCheckResponse>,
    },
    AuthVerified {
        resolved_endpoint: Option<String>,
        http_status: Option<u16>,
        checks: Vec<AdminAiBackendPreflightCheckResponse>,
        auth_valid: bool,
    },
    ModelAccessVerified {
        resolved_endpoint: Option<String>,
        http_status: Option<u16>,
        checks: Vec<AdminAiBackendPreflightCheckResponse>,
        auth_valid: bool,
        model_accessible: bool,
    },
    ConnectivityFailed {
        error_code: String,
        resolved_endpoint: Option<String>,
        http_status: Option<u16>,
        provider_code: Option<String>,
        checks: Vec<AdminAiBackendPreflightCheckResponse>,
    },
    AuthFailed {
        error_code: String,
        resolved_endpoint: Option<String>,
        http_status: Option<u16>,
        provider_code: Option<String>,
        checks: Vec<AdminAiBackendPreflightCheckResponse>,
        auth_valid: bool,
    },
    ModelAccessFailed {
        error_code: String,
        resolved_endpoint: Option<String>,
        http_status: Option<u16>,
        provider_code: Option<String>,
        checks: Vec<AdminAiBackendPreflightCheckResponse>,
        auth_valid: bool,
        model_accessible: bool,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "outcome_kind", rename_all = "snake_case")]
pub(crate) enum AdminAiBackendOllamaPreflightOutcomeResponse {
    ConnectivityVerified {
        resolved_endpoint: Option<String>,
        http_status: Option<u16>,
        checks: Vec<AdminAiBackendPreflightCheckResponse>,
    },
    ModelAccessVerified {
        resolved_endpoint: Option<String>,
        http_status: Option<u16>,
        checks: Vec<AdminAiBackendPreflightCheckResponse>,
        model_accessible: bool,
    },
    ConnectivityFailed {
        error_code: String,
        resolved_endpoint: Option<String>,
        http_status: Option<u16>,
        provider_code: Option<String>,
        checks: Vec<AdminAiBackendPreflightCheckResponse>,
    },
    ModelAccessFailed {
        error_code: String,
        resolved_endpoint: Option<String>,
        http_status: Option<u16>,
        provider_code: Option<String>,
        checks: Vec<AdminAiBackendPreflightCheckResponse>,
        model_accessible: bool,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "provider_family", rename_all = "snake_case")]
pub(crate) enum AdminAiBackendRemoteProviderPreflightResultResponse {
    OpenAiCompatible {
        outcome: AdminAiBackendOpenAiCompatiblePreflightOutcomeResponse,
    },
    Ollama {
        outcome: AdminAiBackendOllamaPreflightOutcomeResponse,
    },
    Unknown {
        provider: String,
        outcome: AdminAiBackendRemoteProviderPreflightOutcomeResponse,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum AdminAiBackendPreflightDetailsResponse {
    LocalModel {
        source: AdminAiBackendLocalModelPreflightSourceResponse,
    },
    RemoteProvider {
        result: AdminAiBackendRemoteProviderPreflightResultResponse,
    },
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminAiBackendPreflightNodeResponse {
    pub(crate) node_uuid: String,
    pub(crate) success: bool,
    pub(crate) details: AdminAiBackendPreflightDetailsResponse,
    pub(crate) latency_ms: Option<u64>,
    pub(crate) message: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminAiBackendPreflightResponse {
    pub(crate) success: bool,
    pub(crate) results: Vec<AdminAiBackendPreflightNodeResponse>,
    pub(crate) message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminAiBackendPlacementResponse {
    pub(crate) placement_id: String,
    pub(crate) backend_id: String,
    pub(crate) node_uuid: String,
    pub(crate) desired_state: String,
    pub(crate) weight_override: Option<i32>,
    pub(crate) priority_override: Option<i32>,
    pub(crate) generation: u64,
    pub(crate) created_at_ms: i64,
    pub(crate) updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminAiBackendPlacementListResponse {
    pub(crate) success: bool,
    pub(crate) placements: Vec<AdminAiBackendPlacementResponse>,
    pub(crate) total_count: i32,
    pub(crate) message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminAiBackendPlacementMutationResponse {
    pub(crate) success: bool,
    pub(crate) placement: Option<AdminAiBackendPlacementResponse>,
    pub(crate) deleted: Option<bool>,
    pub(crate) message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminAiBackendNodeStatusResponse {
    pub(crate) backend_id: String,
    pub(crate) node_uuid: String,
    pub(crate) observed_generation: u64,
    pub(crate) status: String,
    pub(crate) status_reason: String,
    pub(crate) runtime_backend_name: Option<String>,
    pub(crate) endpoint: Option<String>,
    pub(crate) available: bool,
    pub(crate) operations: Vec<String>,
    pub(crate) features: Vec<String>,
    pub(crate) transports: Vec<String>,
    pub(crate) last_heartbeat_at_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminAiBackendNodeStatusListResponse {
    pub(crate) success: bool,
    pub(crate) statuses: Vec<AdminAiBackendNodeStatusResponse>,
    pub(crate) total_count: i32,
    pub(crate) message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminAiBackendAssignmentResponse {
    pub(crate) backend: Option<AdminAiBackendRecordResponse>,
    pub(crate) placement: Option<AdminAiBackendPlacementResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminAiBackendAssignmentListResponse {
    pub(crate) success: bool,
    pub(crate) assignments: Vec<AdminAiBackendAssignmentResponse>,
    pub(crate) total_count: i32,
    pub(crate) message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminAiModelViewInstanceResponse {
    pub(crate) backend_id: String,
    pub(crate) node_uuid: String,
    pub(crate) placement_state: String,
    pub(crate) backend_state: String,
    pub(crate) runtime_status: Option<String>,
    pub(crate) runtime_backend_name: Option<String>,
    pub(crate) endpoint: Option<String>,
    pub(crate) available: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminAiModelViewResponse {
    pub(crate) provider: String,
    pub(crate) model: String,
    pub(crate) hosting: String,
    pub(crate) backend_ids: Vec<String>,
    pub(crate) operations: Vec<String>,
    pub(crate) features: Vec<String>,
    pub(crate) transports: Vec<String>,
    pub(crate) enabled_nodes: i32,
    pub(crate) ready_nodes: i32,
    pub(crate) total_nodes: i32,
    pub(crate) instances: Vec<AdminAiModelViewInstanceResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AdminAiModelViewListResponse {
    pub(crate) success: bool,
    pub(crate) views: Vec<AdminAiModelViewResponse>,
    pub(crate) total_count: i32,
    pub(crate) message: Option<String>,
}

fn trim_to_option(value: Option<String>) -> Option<String> {
    value.and_then(|inner| {
        let trimmed = inner.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

/// Read one optional string field from backend metadata JSON.
/// 从 backend metadata JSON 读取一个可选字符串字段。
fn metadata_string_field(metadata: &serde_json::Value, key: &str) -> Option<String> {
    metadata
        .as_object()
        .and_then(|fields| fields.get(key))
        .and_then(|value| match value {
            serde_json::Value::String(text) => trim_to_option(Some(text.clone())),
            _ => None,
        })
}

/// Read one optional boolean field from backend metadata JSON.
/// 从 backend metadata JSON 读取一个可选布尔字段。
fn metadata_bool_field(metadata: &serde_json::Value, key: &str) -> Option<bool> {
    metadata
        .as_object()
        .and_then(|fields| fields.get(key))
        .and_then(|value| match value {
            serde_json::Value::Bool(flag) => Some(*flag),
            serde_json::Value::String(text) => match text.trim().to_ascii_lowercase().as_str() {
                "1" | "true" => Some(true),
                "0" | "false" => Some(false),
                _ => None,
            },
            _ => None,
        })
}

/// Read one optional unsigned integer field from backend metadata JSON.
/// 从 backend metadata JSON 读取一个可选无符号整数字段。
fn metadata_u64_field(metadata: &serde_json::Value, key: &str) -> Option<u64> {
    metadata
        .as_object()
        .and_then(|fields| fields.get(key))
        .and_then(|value| match value {
            serde_json::Value::Number(number) => number.as_u64(),
            serde_json::Value::String(text) => text.trim().parse::<u64>().ok(),
            _ => None,
        })
}

/// Read one optional u32 field from backend metadata JSON.
/// 从 backend metadata JSON 读取一个可选 u32 字段。
fn metadata_u32_field(metadata: &serde_json::Value, key: &str) -> Option<u32> {
    metadata_u64_field(metadata, key).and_then(|value| u32::try_from(value).ok())
}

fn ai_backend_hosting_label(value: i32) -> &'static str {
    match crate::proto::sms::AiBackendHosting::try_from(value)
        .unwrap_or(crate::proto::sms::AiBackendHosting::Unspecified)
    {
        crate::proto::sms::AiBackendHosting::Local => "local",
        crate::proto::sms::AiBackendHosting::Remote => "remote",
        crate::proto::sms::AiBackendHosting::Unspecified => "unspecified",
    }
}

fn ai_backend_desired_state_label(value: i32) -> &'static str {
    match crate::proto::sms::AiBackendDesiredState::try_from(value)
        .unwrap_or(crate::proto::sms::AiBackendDesiredState::Unspecified)
    {
        crate::proto::sms::AiBackendDesiredState::Enabled => "enabled",
        crate::proto::sms::AiBackendDesiredState::Disabled => "disabled",
        crate::proto::sms::AiBackendDesiredState::Unspecified => "unspecified",
    }
}

fn ai_backend_management_mode_label(value: i32) -> &'static str {
    match crate::proto::sms::AiBackendManagementMode::try_from(value)
        .unwrap_or(crate::proto::sms::AiBackendManagementMode::Unspecified)
    {
        crate::proto::sms::AiBackendManagementMode::SmsLocal => "sms_local",
        crate::proto::sms::AiBackendManagementMode::SmsRemote => "sms_remote",
        crate::proto::sms::AiBackendManagementMode::Unspecified => "unspecified",
    }
}

fn ai_backend_node_status_label(value: i32) -> &'static str {
    match crate::proto::sms::AiBackendNodeStatus::try_from(value)
        .unwrap_or(crate::proto::sms::AiBackendNodeStatus::Unspecified)
    {
        crate::proto::sms::AiBackendNodeStatus::Pending => "pending",
        crate::proto::sms::AiBackendNodeStatus::Reconciling => "reconciling",
        crate::proto::sms::AiBackendNodeStatus::Ready => "ready",
        crate::proto::sms::AiBackendNodeStatus::Degraded => "degraded",
        crate::proto::sms::AiBackendNodeStatus::Error => "error",
        crate::proto::sms::AiBackendNodeStatus::Disabled => "disabled",
        crate::proto::sms::AiBackendNodeStatus::Unspecified => "unspecified",
    }
}

fn proto_value_to_json(value: &prost_types::Value) -> serde_json::Value {
    use prost_types::value::Kind;
    match &value.kind {
        Some(Kind::NullValue(_)) | None => serde_json::Value::Null,
        Some(Kind::NumberValue(number)) => serde_json::json!(number),
        Some(Kind::StringValue(text)) => serde_json::json!(text),
        Some(Kind::BoolValue(flag)) => serde_json::json!(flag),
        Some(Kind::StructValue(struct_value)) => serde_json::Value::Object(
            struct_value
                .fields
                .iter()
                .map(|(key, val)| (key.clone(), proto_value_to_json(val)))
                .collect(),
        ),
        Some(Kind::ListValue(list_value)) => {
            serde_json::Value::Array(list_value.values.iter().map(proto_value_to_json).collect())
        }
    }
}

fn proto_struct_to_json(value: Option<&prost_types::Struct>) -> serde_json::Value {
    let fields = value
        .map(|inner| {
            inner
                .fields
                .iter()
                .map(|(key, val)| (key.clone(), proto_value_to_json(val)))
                .collect()
        })
        .unwrap_or_default();
    serde_json::Value::Object(fields)
}

fn spec_to_response(spec: crate::proto::sms::BackendSpec) -> AdminAiBackendSpecResponse {
    AdminAiBackendSpecResponse {
        name: spec.name,
        kind: spec.kind,
        operations: spec.operations,
        features: spec.features,
        transports: spec.transports,
        weight: spec.weight,
        priority: spec.priority,
        base_url: spec.base_url,
        provider: spec.provider,
        model: spec.model,
        credential_ref: trim_to_option(Some(spec.credential_ref)),
        origin: spec.origin,
        deployment_id: spec.deployment_id,
    }
}

/// Derive one structured local read view from the canonical stored backend record.
/// 从规范化存储后的 backend record 派生一个结构化 local 读视图。
fn backend_record_local_response(
    record: &crate::proto::sms::AiBackendRecord,
    metadata: &serde_json::Value,
) -> Option<AdminAiBackendLocalProviderResponse> {
    if ai_backend_hosting_label(record.hosting) != "local" {
        return None;
    }
    match record.provider.trim().to_ascii_lowercase().as_str() {
        "llamacpp" => Some(AdminAiBackendLocalProviderResponse::LlamaCpp(
            AdminAiBackendLlamaCppLocalConfigResponse {
                model_url: metadata_string_field(metadata, "model_url"),
                model_path: metadata_string_field(metadata, "model_path"),
                skip_download: metadata_bool_field(metadata, "skip_download"),
                download_timeout_s: metadata_u64_field(metadata, "download_timeout_s"),
                server_mode: metadata_string_field(metadata, "server_mode"),
                server_cmd: metadata_string_field(metadata, "server_cmd"),
                server_cmd_args: metadata_string_field(metadata, "server_cmd_args"),
                threads: metadata_u32_field(metadata, "threads"),
                ctx_size: metadata_u32_field(metadata, "ctx_size"),
                ready_probe: metadata_string_field(metadata, "ready_probe"),
                start_timeout_s: metadata_u64_field(metadata, "start_timeout_s"),
            },
        )),
        "vllm" => Some(AdminAiBackendLocalProviderResponse::Vllm(
            AdminAiBackendVllmLocalConfigResponse {
                mode: metadata_string_field(metadata, "mode"),
                managed_externally: metadata_bool_field(metadata, "managed_externally"),
            },
        )),
        _ => None,
    }
}

/// Derive one structured remote read view from the canonical stored backend record.
/// 从规范化存储后的 backend record 派生一个结构化 remote 读视图。
fn backend_record_remote_response(
    record: &crate::proto::sms::AiBackendRecord,
) -> Option<AdminAiBackendRemoteProviderResponse> {
    if ai_backend_hosting_label(record.hosting) != "remote" {
        return None;
    }
    let spec = record.spec.as_ref()?;
    let base_url = trim_to_option(Some(spec.base_url.clone()));
    match record.provider.trim().to_ascii_lowercase().as_str() {
        "openai" => Some(AdminAiBackendRemoteProviderResponse::OpenAiCompatible(
            AdminAiBackendOpenAiCompatibleRemoteConfigResponse {
                base_url,
                credential_ref: trim_to_option(record.credential_ref.clone()),
                operations: spec.operations.clone(),
                features: spec.features.clone(),
                transports: spec.transports.clone(),
            },
        )),
        "ollama" => Some(AdminAiBackendRemoteProviderResponse::Ollama(
            AdminAiBackendOllamaRemoteConfigResponse {
                base_url,
                operations: spec.operations.clone(),
                features: spec.features.clone(),
                transports: spec.transports.clone(),
            },
        )),
        _ => None,
    }
}

pub(crate) fn backend_record_to_response(
    record: crate::proto::sms::AiBackendRecord,
) -> AdminAiBackendRecordResponse {
    let metadata = proto_struct_to_json(record.metadata.as_ref());
    let local = backend_record_local_response(&record, &metadata);
    let remote = backend_record_remote_response(&record);
    AdminAiBackendRecordResponse {
        backend_id: record.backend_id,
        display_name: record.display_name,
        provider: record.provider,
        model: record.model,
        hosting: ai_backend_hosting_label(record.hosting).to_string(),
        backend_kind: record.backend_kind,
        desired_state: ai_backend_desired_state_label(record.desired_state).to_string(),
        management_mode: ai_backend_management_mode_label(record.management_mode).to_string(),
        credential_ref: trim_to_option(record.credential_ref),
        spec: record.spec.map(spec_to_response),
        local,
        remote,
        labels: record.labels,
        metadata,
        generation: record.generation,
        created_at_ms: record.created_at_ms,
        updated_at_ms: record.updated_at_ms,
    }
}

pub(crate) fn backend_placement_to_response(
    record: crate::proto::sms::AiBackendPlacementRecord,
) -> AdminAiBackendPlacementResponse {
    AdminAiBackendPlacementResponse {
        placement_id: record.placement_id,
        backend_id: record.backend_id,
        node_uuid: record.node_uuid,
        desired_state: ai_backend_desired_state_label(record.desired_state).to_string(),
        weight_override: record.weight_override,
        priority_override: record.priority_override,
        generation: record.generation,
        created_at_ms: record.created_at_ms,
        updated_at_ms: record.updated_at_ms,
    }
}

pub(crate) fn backend_node_status_to_response(
    record: crate::proto::sms::AiBackendNodeStatusRecord,
) -> AdminAiBackendNodeStatusResponse {
    AdminAiBackendNodeStatusResponse {
        backend_id: record.backend_id,
        node_uuid: record.node_uuid,
        observed_generation: record.observed_generation,
        status: ai_backend_node_status_label(record.status).to_string(),
        status_reason: record.status_reason,
        runtime_backend_name: trim_to_option(Some(record.runtime_backend_name)),
        endpoint: trim_to_option(Some(record.endpoint)),
        available: record.available,
        operations: record.operations,
        features: record.features,
        transports: record.transports,
        last_heartbeat_at_ms: record.last_heartbeat_at_ms,
    }
}

pub(crate) fn backend_assignment_to_response(
    assignment: crate::proto::sms::ResolvedAiBackendAssignment,
) -> AdminAiBackendAssignmentResponse {
    AdminAiBackendAssignmentResponse {
        backend: assignment.backend.map(backend_record_to_response),
        placement: assignment.placement.map(backend_placement_to_response),
    }
}

fn model_view_instance_to_response(
    instance: crate::proto::sms::AiModelViewInstance,
) -> AdminAiModelViewInstanceResponse {
    AdminAiModelViewInstanceResponse {
        backend_id: instance.backend_id,
        node_uuid: instance.node_uuid,
        placement_state: ai_backend_desired_state_label(instance.placement_state).to_string(),
        backend_state: ai_backend_desired_state_label(instance.backend_state).to_string(),
        runtime_status: instance
            .runtime_status
            .map(|status| ai_backend_node_status_label(status).to_string()),
        runtime_backend_name: trim_to_option(instance.runtime_backend_name),
        endpoint: trim_to_option(instance.endpoint),
        available: instance.available,
    }
}

pub(crate) fn model_view_to_response(
    view: crate::proto::sms::AiModelView,
) -> AdminAiModelViewResponse {
    AdminAiModelViewResponse {
        provider: view.provider,
        model: view.model,
        hosting: ai_backend_hosting_label(view.hosting).to_string(),
        backend_ids: view.backend_ids,
        operations: view.operations,
        features: view.features,
        transports: view.transports,
        enabled_nodes: view.enabled_nodes,
        ready_nodes: view.ready_nodes,
        total_nodes: view.total_nodes,
        instances: view
            .instances
            .into_iter()
            .map(model_view_instance_to_response)
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_record_mapping_keeps_labels_and_metadata() {
        let mut metadata = prost_types::Struct::default();
        metadata.fields.insert(
            "region".to_string(),
            prost_types::Value {
                kind: Some(prost_types::value::Kind::StringValue("sg".to_string())),
            },
        );

        let response = backend_record_to_response(crate::proto::sms::AiBackendRecord {
            backend_id: "backend-1".to_string(),
            display_name: "Backend".to_string(),
            provider: "openai".to_string(),
            model: "gpt".to_string(),
            hosting: crate::proto::sms::AiBackendHosting::Remote as i32,
            backend_kind: "http".to_string(),
            desired_state: crate::proto::sms::AiBackendDesiredState::Enabled as i32,
            management_mode: crate::proto::sms::AiBackendManagementMode::SmsRemote as i32,
            credential_ref: Some("cred-a".to_string()),
            spec: None,
            labels: std::collections::HashMap::from([("tier".to_string(), "prod".to_string())]),
            metadata: Some(metadata),
            generation: 1,
            created_at_ms: 10,
            updated_at_ms: 20,
        });

        assert_eq!(response.hosting, "remote");
        assert_eq!(response.desired_state, "enabled");
        assert_eq!(
            response
                .metadata
                .get("region")
                .and_then(|value| value.as_str()),
            Some("sg")
        );
    }

    #[test]
    fn model_view_mapping_keeps_instance_runtime_status() {
        let response = model_view_to_response(crate::proto::sms::AiModelView {
            provider: "openai".to_string(),
            model: "gpt".to_string(),
            hosting: crate::proto::sms::AiBackendHosting::Local as i32,
            backend_ids: vec!["backend-1".to_string()],
            operations: vec!["chat".to_string()],
            features: vec!["stream".to_string()],
            transports: vec!["http".to_string()],
            enabled_nodes: 1,
            ready_nodes: 1,
            total_nodes: 1,
            instances: vec![crate::proto::sms::AiModelViewInstance {
                backend_id: "backend-1".to_string(),
                node_uuid: "node-1".to_string(),
                placement_state: crate::proto::sms::AiBackendDesiredState::Enabled as i32,
                backend_state: crate::proto::sms::AiBackendDesiredState::Enabled as i32,
                runtime_status: Some(crate::proto::sms::AiBackendNodeStatus::Ready as i32),
                runtime_backend_name: Some("runtime-a".to_string()),
                endpoint: Some("http://127.0.0.1".to_string()),
                available: true,
            }],
        });

        assert_eq!(response.hosting, "local");
        assert_eq!(
            response
                .instances
                .first()
                .and_then(|instance| instance.runtime_status.as_deref()),
            Some("ready")
        );
    }
}
