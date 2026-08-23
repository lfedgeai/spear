use std::sync::Arc;

use crate::ai_backend_types::{
    CanonicalBackendHosting, CanonicalBackendKind, CanonicalBackendOrigin, CanonicalBackendProvider,
};
use crate::proto::sms::{BackendHosting, BackendInfo, BackendOrigin, BackendSpec, BackendStatus};
use crate::spearlet::ai::credential_resolver::CredentialResolver;
use crate::spearlet::config::{
    AiBackendConfig, AiBackendProviderView, AiBackendTypedView, LocalBackendConfigView,
    LocalBackendProviderView, RemoteBackendConfigView, RemoteBackendProviderView,
};
use crate::spearlet::execution::ai::backends::ollama_chat::OllamaChatBackendAdapter;
use crate::spearlet::execution::ai::backends::openai_chat_completion::OpenAIChatCompletionBackendAdapter;
use crate::spearlet::execution::ai::backends::openai_realtime_ws::OpenAIRealtimeWsBackendAdapter;
use crate::spearlet::execution::ai::backends::stub::StubBackendAdapter;
use crate::spearlet::execution::ai::backends::BackendAdapter;
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
    CanonicalBackendKind::parse(kind)
        .inferred_provider()
        .as_str()
        .to_string()
}

pub fn parse_origin(v: Option<&str>) -> BackendOrigin {
    CanonicalBackendOrigin::parse_optional(v).to_proto_enum()
}

pub fn parse_hosting(s: &str) -> Hosting {
    match CanonicalBackendHosting::parse(s) {
        CanonicalBackendHosting::Local => Hosting::Local,
        CanonicalBackendHosting::Remote => Hosting::Remote,
        CanonicalBackendHosting::Unknown(_) => Hosting::Unknown,
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

/// Runtime backend family used to centralize capability and adapter selection.
/// 用于集中 capability 与 adapter 选择的运行时 backend 家族。
#[derive(Debug, Clone, PartialEq, Eq)]
enum RuntimeBackendFamily {
    /// OpenAI chat-completions adapter family / OpenAI chat-completions 适配器家族
    OpenAiChatCompletion,
    /// OpenAI realtime websocket adapter family / OpenAI realtime websocket 适配器家族
    OpenAiRealtimeWs,
    /// Ollama chat adapter family / Ollama chat 适配器家族
    OllamaChat,
    /// Internal stub adapter family / 内部 stub 适配器家族
    Stub,
    /// Unsupported runtime backend family / 不支持的运行时 backend 家族
    Unknown(String),
}

impl RuntimeBackendFamily {
    /// Classify one runtime backend kind into a canonical adapter family.
    /// 将运行时 backend kind 分类为规范 adapter 家族。
    fn from_kind(kind: &str) -> Self {
        match CanonicalBackendKind::parse(kind) {
            CanonicalBackendKind::OpenAiChatCompletion => Self::OpenAiChatCompletion,
            CanonicalBackendKind::OpenAiRealtimeWs => Self::OpenAiRealtimeWs,
            CanonicalBackendKind::OllamaChat => Self::OllamaChat,
            CanonicalBackendKind::Stub => Self::Stub,
            other => Self::Unknown(other.as_str().to_string()),
        }
    }

    /// Return the supported operation set for one runtime family when it is fixed.
    /// 返回一个运行时家族在固定情况下支持的 operation 集合。
    fn supported_operations(&self) -> Option<&'static [Operation]> {
        match self {
            Self::OpenAiChatCompletion | Self::OllamaChat | Self::Stub => {
                Some(&[Operation::ChatCompletions])
            }
            Self::OpenAiRealtimeWs => Some(&[Operation::SpeechToText]),
            Self::Unknown(_) => None,
        }
    }
}

pub fn origin_to_config_value(origin: i32) -> Option<String> {
    Some(
        CanonicalBackendOrigin::from_proto_i32(origin)
            .as_str()
            .to_string(),
    )
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
        .map(CanonicalBackendProvider::parse)
        .filter(|provider| !matches!(provider, CanonicalBackendProvider::Unknown(_)))
        .map(|provider| provider.as_str().to_string())
        .unwrap_or_else(|| infer_provider(&parts.kind));
    BackendSpec {
        name: parts.name,
        kind: CanonicalBackendKind::parse(&parts.kind)
            .as_str()
            .to_string(),
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
    if let Ok(view) = b.typed_view() {
        return backend_spec_from_typed_config(&view);
    }
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

/// Convert typed config view into the shared `BackendSpec`.
/// 将强类型配置视图转换为统一的 `BackendSpec`。
pub fn backend_spec_from_typed_config(view: &AiBackendTypedView) -> BackendSpec {
    match view.provider_view() {
        AiBackendProviderView::Local(local) => backend_spec_from_local_provider_typed_config(local),
        AiBackendProviderView::Remote(remote) => {
            backend_spec_from_remote_provider_typed_config(remote)
        }
    }
}

fn backend_spec_from_local_provider_typed_config(
    view: LocalBackendProviderView<'_>,
) -> BackendSpec {
    match view {
        LocalBackendProviderView::Ollama(local) => {
            backend_spec_from_local_ollama_typed_config(local)
        }
        LocalBackendProviderView::LlamaCpp(local) => {
            backend_spec_from_local_llamacpp_typed_config(local)
        }
        LocalBackendProviderView::Vllm(local) => backend_spec_from_local_vllm_typed_config(local),
        LocalBackendProviderView::Internal(local) => {
            backend_spec_from_local_internal_typed_config(local)
        }
        LocalBackendProviderView::Unknown(local) => {
            backend_spec_from_local_unknown_typed_config(local)
        }
    }
}

fn backend_spec_from_local_typed_config(view: &LocalBackendConfigView) -> BackendSpec {
    backend_spec_from_parts(BackendSpecParts {
        name: view.common.name.clone(),
        kind: view.common.kind.as_str().to_string(),
        operations: view.common.ops.clone(),
        features: view.common.features.clone(),
        transports: view.common.transports.clone(),
        weight: view.common.weight,
        priority: view.common.priority,
        base_url: view.base_url.clone(),
        provider: Some(view.common.provider.as_str().to_string()),
        model: view.model.clone(),
        hosting: parse_hosting("local"),
        credential_ref: view.credential_ref.clone(),
        origin: view.common.origin.to_proto_enum(),
        deployment_id: view.common.deployment_id.clone(),
    })
}

fn backend_spec_from_local_ollama_typed_config(view: &LocalBackendConfigView) -> BackendSpec {
    backend_spec_from_local_typed_config(view)
}

fn backend_spec_from_local_llamacpp_typed_config(view: &LocalBackendConfigView) -> BackendSpec {
    backend_spec_from_local_typed_config(view)
}

fn backend_spec_from_local_vllm_typed_config(view: &LocalBackendConfigView) -> BackendSpec {
    backend_spec_from_local_typed_config(view)
}

fn backend_spec_from_local_internal_typed_config(view: &LocalBackendConfigView) -> BackendSpec {
    backend_spec_from_local_typed_config(view)
}

fn backend_spec_from_local_unknown_typed_config(view: &LocalBackendConfigView) -> BackendSpec {
    backend_spec_from_local_typed_config(view)
}

fn backend_spec_from_remote_provider_typed_config(
    view: RemoteBackendProviderView<'_>,
) -> BackendSpec {
    match view {
        RemoteBackendProviderView::OpenAi(remote) => {
            backend_spec_from_remote_openai_typed_config(remote)
        }
        RemoteBackendProviderView::Ollama(remote) => {
            backend_spec_from_remote_ollama_typed_config(remote)
        }
        RemoteBackendProviderView::Internal(remote) => {
            backend_spec_from_remote_internal_typed_config(remote)
        }
        RemoteBackendProviderView::Unknown(remote) => {
            backend_spec_from_remote_unknown_typed_config(remote)
        }
    }
}

fn backend_spec_from_remote_typed_config(view: &RemoteBackendConfigView) -> BackendSpec {
    backend_spec_from_parts(BackendSpecParts {
        name: view.common.name.clone(),
        kind: view.common.kind.as_str().to_string(),
        operations: view.common.ops.clone(),
        features: view.common.features.clone(),
        transports: view.common.transports.clone(),
        weight: view.common.weight,
        priority: view.common.priority,
        base_url: view.base_url.clone(),
        provider: Some(view.common.provider.as_str().to_string()),
        model: view.model.clone(),
        hosting: parse_hosting("remote"),
        credential_ref: view.credential_ref.clone(),
        origin: view.common.origin.to_proto_enum(),
        deployment_id: view.common.deployment_id.clone(),
    })
}

fn backend_spec_from_remote_openai_typed_config(view: &RemoteBackendConfigView) -> BackendSpec {
    backend_spec_from_remote_typed_config(view)
}

fn backend_spec_from_remote_ollama_typed_config(view: &RemoteBackendConfigView) -> BackendSpec {
    backend_spec_from_remote_typed_config(view)
}

fn backend_spec_from_remote_internal_typed_config(view: &RemoteBackendConfigView) -> BackendSpec {
    backend_spec_from_remote_typed_config(view)
}

fn backend_spec_from_remote_unknown_typed_config(view: &RemoteBackendConfigView) -> BackendSpec {
    backend_spec_from_remote_typed_config(view)
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
    RuntimeBackendFamily::from_kind(kind).supported_operations()
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
    match RuntimeBackendFamily::from_kind(&spec.kind) {
        RuntimeBackendFamily::OpenAiChatCompletion => {
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
        RuntimeBackendFamily::OpenAiRealtimeWs => {
            Some(Arc::new(OpenAIRealtimeWsBackendAdapter::new(
                spec.name.clone(),
                spec.base_url.clone(),
                cred_ref.map(ToOwned::to_owned),
                credential_resolver.cloned(),
            )))
        }
        RuntimeBackendFamily::OllamaChat => Some(Arc::new(OllamaChatBackendAdapter::new(
            spec.name.clone(),
            spec.base_url.clone(),
            if spec.model.trim().is_empty() {
                None
            } else {
                Some(spec.model.clone())
            },
        ))),
        RuntimeBackendFamily::Stub => {
            if !allow_stub {
                return None;
            }
            Some(Arc::new(StubBackendAdapter::new(&spec.name)))
        }
        RuntimeBackendFamily::Unknown(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::sms::BackendOrigin;
    use crate::spearlet::config::{
        AiBackendCommonView, AiBackendTypedView, LocalBackendConfigView, RemoteBackendConfigView,
    };

    #[test]
    fn runtime_backend_family_classifies_supported_runtime_kinds() {
        assert_eq!(
            RuntimeBackendFamily::from_kind("OPENAI_CHAT_COMPLETION"),
            RuntimeBackendFamily::OpenAiChatCompletion
        );
        assert_eq!(
            RuntimeBackendFamily::from_kind("openai_realtime_ws"),
            RuntimeBackendFamily::OpenAiRealtimeWs
        );
        assert_eq!(
            RuntimeBackendFamily::from_kind("ollama_chat"),
            RuntimeBackendFamily::OllamaChat
        );
        assert_eq!(
            RuntimeBackendFamily::from_kind("stub"),
            RuntimeBackendFamily::Stub
        );
    }

    #[test]
    fn capabilities_from_spec_uses_runtime_family_selector() {
        let spec = backend_spec_from_parts(BackendSpecParts {
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
        });

        let capabilities = capabilities_from_spec(&spec).expect("capabilities should exist");
        assert_eq!(capabilities.ops, vec![Operation::SpeechToText]);
    }

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
    fn backend_spec_from_parts_normalizes_provider_and_kind_aliases() {
        let spec = backend_spec_from_parts(BackendSpecParts {
            name: "local-llama".to_string(),
            kind: "llama.cpp".to_string(),
            operations: vec!["chat_completions".to_string()],
            features: vec![],
            transports: vec!["http".to_string()],
            weight: 100,
            priority: 0,
            base_url: String::new(),
            provider: Some("llama_cpp".to_string()),
            model: Some("qwen".to_string()),
            hosting: Hosting::Local,
            credential_ref: None,
            origin: BackendOrigin::StaticConfig,
            deployment_id: None,
        });

        assert_eq!(spec.kind, "llamacpp");
        assert_eq!(spec.provider, "llamacpp");
    }

    #[test]
    fn parse_origin_and_config_value_use_canonical_origin_helper() {
        assert_eq!(
            parse_origin(Some("local_controller")),
            BackendOrigin::LocalController
        );
        assert_eq!(
            origin_to_config_value(BackendOrigin::Sms as i32),
            Some("sms".to_string())
        );
    }

    #[test]
    fn backend_spec_from_typed_config_supports_local_variant() {
        let spec =
            backend_spec_from_typed_config(&AiBackendTypedView::Local(LocalBackendConfigView {
                common: AiBackendCommonView {
                    name: "ollama-local".to_string(),
                    kind: CanonicalBackendKind::parse("ollama_chat"),
                    provider: CanonicalBackendProvider::parse("ollama"),
                    origin: CanonicalBackendOrigin::parse_optional(Some("local_controller")),
                    deployment_id: None,
                    weight: 100,
                    priority: 0,
                    ops: vec!["chat_completions".to_string()],
                    features: vec![],
                    transports: vec!["http".to_string()],
                },
                model: Some("llama3.1:8b".to_string()),
                credential_ref: None,
                base_url: "http://127.0.0.1:11434".to_string(),
            }));

        assert_eq!(spec.kind, "ollama_chat");
        assert_eq!(spec.provider, "ollama");
        assert_eq!(spec.hosting, BackendHosting::NodeLocal as i32);
    }

    #[test]
    fn backend_spec_from_typed_config_supports_remote_variant() {
        let spec =
            backend_spec_from_typed_config(&AiBackendTypedView::Remote(RemoteBackendConfigView {
                common: AiBackendCommonView {
                    name: "openai-remote".to_string(),
                    kind: CanonicalBackendKind::parse("openai_chat_completion"),
                    provider: CanonicalBackendProvider::parse("openai"),
                    origin: CanonicalBackendOrigin::parse_optional(Some("sms")),
                    deployment_id: Some("deploy-1".to_string()),
                    weight: 100,
                    priority: 0,
                    ops: vec!["chat_completions".to_string()],
                    features: vec![],
                    transports: vec!["http".to_string()],
                },
                model: Some("gpt-4o-mini".to_string()),
                credential_ref: Some("openai-key".to_string()),
                base_url: "https://api.openai.com/v1".to_string(),
            }));

        assert_eq!(spec.kind, "openai_chat_completion");
        assert_eq!(spec.provider, "openai");
        assert_eq!(spec.hosting, BackendHosting::Remote as i32);
        assert_eq!(spec.credential_ref, "openai-key");
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

    #[test]
    fn build_adapter_from_spec_respects_stub_family_gate() {
        let spec = backend_spec_from_parts(BackendSpecParts {
            name: "stub-backend".to_string(),
            kind: "stub".to_string(),
            operations: vec!["chat_completions".to_string()],
            features: vec![],
            transports: vec!["memory".to_string()],
            weight: 100,
            priority: 0,
            base_url: String::new(),
            provider: Some("internal".to_string()),
            model: None,
            hosting: Hosting::Local,
            credential_ref: None,
            origin: BackendOrigin::StaticConfig,
            deployment_id: None,
        });

        assert!(build_adapter_from_spec(&spec, None, false).is_none());
        assert!(build_adapter_from_spec(&spec, None, true).is_some());
    }
}
