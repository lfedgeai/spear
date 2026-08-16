use std::sync::Arc;

use crate::proto::sms::{BackendHosting, BackendInfo, BackendOrigin, BackendSpec, BackendStatus};
use crate::spearlet::ai::credential_resolver::CredentialResolver;
use crate::spearlet::config::AiBackendConfig;
use crate::spearlet::execution::ai::backends::ollama_chat::OllamaChatBackendAdapter;
use crate::spearlet::execution::ai::backends::openai_chat_completion::OpenAIChatCompletionBackendAdapter;
use crate::spearlet::execution::ai::backends::openai_realtime_ws::OpenAIRealtimeWsBackendAdapter;
use crate::spearlet::execution::ai::backends::stub::StubBackendAdapter;
use crate::spearlet::execution::ai::backends::{
    BackendAdapter, KIND_OLLAMA_CHAT, KIND_OPENAI_CHAT_COMPLETION, KIND_OPENAI_REALTIME_WS,
    KIND_STUB,
};
use crate::spearlet::execution::ai::ir::Operation;
use crate::spearlet::execution::ai::router::capabilities::Capabilities;
use crate::spearlet::execution::ai::router::registry::{BackendInstance, Hosting};

#[derive(Debug, Clone)]
pub struct BackendSpecParts {
    pub name: String,
    pub kind: String,
    pub operations: Vec<String>,
    pub features: Vec<String>,
    pub transports: Vec<String>,
    pub weight: u32,
    pub priority: i32,
    pub base_url: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub hosting: Hosting,
    pub credential_ref: Option<String>,
    pub origin: BackendOrigin,
    pub deployment_id: Option<String>,
}

pub fn infer_provider(kind: &str) -> String {
    let k = kind.trim();
    if k.starts_with("openai_") {
        "openai".to_string()
    } else if k == KIND_OLLAMA_CHAT {
        "ollama".to_string()
    } else if k == KIND_STUB {
        "internal".to_string()
    } else {
        "unknown".to_string()
    }
}

pub fn parse_origin(v: Option<&str>) -> BackendOrigin {
    match v.map(|s| s.trim().to_ascii_lowercase()) {
        Some(s) if s == "sms" => BackendOrigin::Sms,
        Some(s) if s == "static_config" => BackendOrigin::StaticConfig,
        Some(s) if s == "local_controller" => BackendOrigin::LocalController,
        _ => BackendOrigin::StaticConfig,
    }
}

pub fn parse_hosting(s: &str) -> Hosting {
    let v = s.trim().to_ascii_lowercase();
    match v.as_str() {
        "local" => Hosting::Local,
        "remote" => Hosting::Remote,
        _ => Hosting::Unknown,
    }
}

pub fn resolve_hosting(override_value: Option<&str>) -> Hosting {
    if let Some(v) = override_value.map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let parsed = parse_hosting(v);
        if parsed != Hosting::Unknown {
            return parsed;
        }
    }
    Hosting::Unknown
}

pub fn hosting_to_proto(h: Hosting) -> i32 {
    match h {
        Hosting::Local => BackendHosting::NodeLocal as i32,
        Hosting::Remote => BackendHosting::Remote as i32,
        Hosting::Unknown => BackendHosting::Unspecified as i32,
    }
}

pub fn hosting_from_proto(v: i32) -> Hosting {
    match v {
        x if x == BackendHosting::NodeLocal as i32 => Hosting::Local,
        x if x == BackendHosting::Remote as i32 => Hosting::Remote,
        _ => Hosting::Unknown,
    }
}

fn normalized_optional(v: Option<&str>) -> String {
    v.map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_default()
}

fn optional_string(v: &str) -> Option<String> {
    let trimmed = v.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub fn origin_to_config_value(origin: i32) -> Option<String> {
    match BackendOrigin::try_from(origin).ok() {
        Some(BackendOrigin::Sms) => Some("sms".to_string()),
        Some(BackendOrigin::StaticConfig) => Some("static_config".to_string()),
        Some(BackendOrigin::LocalController) => Some("local_controller".to_string()),
        _ => None,
    }
}

pub fn hosting_to_config_value(hosting: i32) -> Option<String> {
    match hosting {
        x if x == BackendHosting::Remote as i32 => Some("remote".to_string()),
        x if x == BackendHosting::NodeLocal as i32 => Some("local".to_string()),
        x if x == BackendHosting::Unspecified as i32 => Some("unknown".to_string()),
        _ => None,
    }
}

/// Build the canonical `BackendSpec` from typed parts.
/// 基于类型化输入构建统一的 `BackendSpec`。
pub fn backend_spec_from_parts(parts: BackendSpecParts) -> BackendSpec {
    let provider = parts
        .provider
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| infer_provider(&parts.kind));
    BackendSpec {
        name: parts.name,
        kind: parts.kind,
        operations: parts.operations,
        features: parts.features,
        transports: parts.transports,
        weight: parts.weight,
        priority: parts.priority,
        base_url: parts.base_url,
        provider,
        model: normalized_optional(parts.model.as_deref()),
        hosting: hosting_to_proto(parts.hosting),
        credential_ref: normalized_optional(parts.credential_ref.as_deref()),
        origin: parts.origin as i32,
        deployment_id: normalized_optional(parts.deployment_id.as_deref()),
    }
}

/// Apply runtime weight/priority overrides to a backend spec.
/// 将运行时 weight/priority 覆盖项应用到 backend spec。
pub fn apply_runtime_overrides(
    spec: &mut BackendSpec,
    weight_override: Option<i32>,
    priority_override: Option<i32>,
) {
    if let Some(weight_override) = weight_override {
        spec.weight = weight_override.max(0) as u32;
    }
    if let Some(priority_override) = priority_override {
        spec.priority = priority_override;
    }
}

/// Build an available runtime backend info from a canonical spec.
/// 基于规范 spec 构建一个可用状态的运行时 backend info。
pub fn available_backend_info(spec: BackendSpec) -> BackendInfo {
    BackendInfo {
        spec: Some(spec),
        status: BackendStatus::Available as i32,
        status_reason: String::new(),
    }
}

/// Convert config-layer backend definition into the shared `BackendSpec`.
/// 将配置层 backend 定义转换为统一的 `BackendSpec`。
pub fn backend_spec_from_config(b: &AiBackendConfig) -> BackendSpec {
    let hosting = resolve_hosting(b.hosting.as_deref());
    let origin = parse_origin(b.origin.as_deref());
    backend_spec_from_parts(BackendSpecParts {
        name: b.name.clone(),
        kind: b.kind.clone(),
        operations: b.ops.clone(),
        features: b.features.clone(),
        transports: b.transports.clone(),
        weight: b.weight,
        priority: b.priority,
        base_url: b.base_url.clone(),
        provider: b.provider.clone(),
        model: b.model.clone(),
        hosting,
        credential_ref: b.credential_ref.clone(),
        origin,
        deployment_id: b.deployment_id.clone(),
    })
}

/// Convert the shared `BackendSpec` into config-layer backend definition.
/// 将统一的 `BackendSpec` 转回配置层 backend 定义。
pub fn ai_backend_config_from_spec(spec: BackendSpec) -> AiBackendConfig {
    AiBackendConfig {
        name: spec.name,
        kind: spec.kind,
        base_url: spec.base_url,
        hosting: hosting_to_config_value(spec.hosting),
        model: optional_string(&spec.model),
        credential_ref: optional_string(&spec.credential_ref),
        provider: optional_string(&spec.provider),
        origin: origin_to_config_value(spec.origin),
        deployment_id: optional_string(&spec.deployment_id),
        weight: spec.weight,
        priority: spec.priority,
        ops: spec.operations,
        features: spec.features,
        transports: spec.transports,
    }
}

pub fn parse_operation(s: &str) -> Option<Operation> {
    match s {
        "chat_completions" => Some(Operation::ChatCompletions),
        "embeddings" => Some(Operation::Embeddings),
        "image_generation" => Some(Operation::ImageGeneration),
        "speech_to_text" => Some(Operation::SpeechToText),
        "text_to_speech" => Some(Operation::TextToSpeech),
        _ => None,
    }
}

fn supported_operations_for_kind(kind: &str) -> Option<&'static [Operation]> {
    match kind {
        KIND_OPENAI_CHAT_COMPLETION | KIND_OLLAMA_CHAT | KIND_STUB => {
            Some(&[Operation::ChatCompletions])
        }
        KIND_OPENAI_REALTIME_WS => Some(&[Operation::SpeechToText]),
        _ => None,
    }
}

fn kind_supports_operation(kind: &str, operation: &Operation) -> bool {
    supported_operations_for_kind(kind)
        .map(|ops| ops.iter().any(|op| op == operation))
        .unwrap_or(true)
}

pub fn capabilities_from_spec(spec: &BackendSpec) -> Option<Capabilities> {
    let ops = spec
        .operations
        .iter()
        .filter_map(|s| parse_operation(s))
        .collect::<Vec<_>>();
    if ops.is_empty() {
        return None;
    }
    if ops
        .iter()
        .any(|operation| !kind_supports_operation(&spec.kind, operation))
    {
        tracing::warn!(
            backend = %spec.name,
            kind = %spec.kind,
            operations = ?spec.operations,
            "backend declares operations that are unsupported for this kind"
        );
        return None;
    }
    Some(Capabilities {
        ops,
        features: spec.features.clone(),
        transports: spec.transports.clone(),
    })
}

/// Build a canonical runtime backend instance from `BackendSpec`.
/// 基于 `BackendSpec` 构建统一的运行时 backend instance。
pub fn build_instance_from_spec(
    mut spec: BackendSpec,
    credential_resolver: Option<&CredentialResolver>,
    allow_stub: bool,
) -> Option<BackendInstance> {
    if spec.name.trim().is_empty() {
        return None;
    }
    if spec.provider.trim().is_empty() {
        spec.provider = infer_provider(&spec.kind);
    }
    let hosting = hosting_from_proto(spec.hosting);
    let capabilities = capabilities_from_spec(&spec)?;
    let adapter = build_adapter_from_spec(&spec, credential_resolver, allow_stub)?;
    Some(BackendInstance {
        spec,
        hosting,
        capabilities,
        adapter,
    })
}

/// Build a canonical runtime backend instance from config-layer backend definition.
/// 基于配置层 backend 定义构建统一的运行时 backend instance。
pub fn build_instance_from_config(
    backend: &AiBackendConfig,
    credential_resolver: Option<&CredentialResolver>,
    allow_stub: bool,
) -> Option<BackendInstance> {
    let spec = backend_spec_from_config(backend);
    build_instance_from_spec(spec, credential_resolver, allow_stub)
}

/// Build a runtime adapter from the shared `BackendSpec`.
/// 基于统一 `BackendSpec` 构建运行时 adapter。
pub fn build_adapter_from_spec(
    spec: &BackendSpec,
    credential_resolver: Option<&CredentialResolver>,
    allow_stub: bool,
) -> Option<Arc<dyn BackendAdapter>> {
    let cred_ref = if spec.credential_ref.trim().is_empty() {
        None
    } else {
        Some(spec.credential_ref.as_str())
    };
    match spec.kind.as_str() {
        KIND_OPENAI_CHAT_COMPLETION => {
            let mut a = OpenAIChatCompletionBackendAdapter::new(
                spec.name.clone(),
                spec.base_url.clone(),
                cred_ref.map(ToOwned::to_owned),
                credential_resolver.cloned(),
            );
            if !spec.model.trim().is_empty() {
                a = a.with_fixed_model(spec.model.clone());
            }
            Some(Arc::new(a))
        }
        KIND_OPENAI_REALTIME_WS => {
            Some(Arc::new(OpenAIRealtimeWsBackendAdapter::new(
                spec.name.clone(),
                spec.base_url.clone(),
                cred_ref.map(ToOwned::to_owned),
                credential_resolver.cloned(),
            )))
        }
        KIND_OLLAMA_CHAT => Some(Arc::new(OllamaChatBackendAdapter::new(
            spec.name.clone(),
            spec.base_url.clone(),
            if spec.model.trim().is_empty() {
                None
            } else {
                Some(spec.model.clone())
            },
        ))),
        KIND_STUB => {
            if !allow_stub {
                return None;
            }
            Some(Arc::new(StubBackendAdapter::new(&spec.name)))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::sms::BackendOrigin;

    #[test]
    fn build_instance_from_spec_infers_provider_and_hosting() {
        let inst = build_instance_from_spec(
            backend_spec_from_parts(BackendSpecParts {
                name: "rt".to_string(),
                kind: "openai_realtime_ws".to_string(),
                operations: vec!["speech_to_text".to_string()],
                features: vec![],
                transports: vec!["websocket".to_string()],
                weight: 100,
                priority: 0,
                base_url: "https://api.openai.com/v1".to_string(),
                provider: None,
                model: None,
                hosting: Hosting::Remote,
                credential_ref: None,
                origin: BackendOrigin::Sms,
                deployment_id: None,
            }),
            None,
            true,
        )
        .expect("backend instance should be built");

        assert_eq!(inst.spec.provider, "openai");
        assert_eq!(inst.hosting, Hosting::Remote);
        assert_eq!(inst.spec.kind, "openai_realtime_ws");
    }

    #[test]
    fn parse_operation_rejects_unimplemented_realtime_voice() {
        assert_eq!(parse_operation("realtime_voice"), None);
    }

    #[test]
    fn build_instance_from_spec_rejects_kind_operation_mismatch() {
        let inst = build_instance_from_spec(
            backend_spec_from_parts(BackendSpecParts {
                name: "rt-invalid".to_string(),
                kind: "openai_realtime_ws".to_string(),
                operations: vec!["chat_completions".to_string()],
                features: vec![],
                transports: vec!["websocket".to_string()],
                weight: 100,
                priority: 0,
                base_url: "https://api.openai.com/v1".to_string(),
                provider: None,
                model: None,
                hosting: Hosting::Remote,
                credential_ref: None,
                origin: BackendOrigin::Sms,
                deployment_id: None,
            }),
            None,
            true,
        );

        assert!(inst.is_none());
    }

}
