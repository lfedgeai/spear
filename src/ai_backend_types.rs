//! Shared typed helpers for AI backend provider/kind/hosting values.
//! AI backend 的 provider/kind/hosting 共享强类型辅助。

use crate::proto::sms::{
    AiBackendDesiredState, AiBackendHosting, AiBackendManagementMode, BackendOrigin,
};

/// Canonical backend hosting kind used by internal code paths.
/// 内部代码路径使用的规范 backend hosting 类型。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CanonicalBackendHosting {
    /// Node-local managed runtime / 节点本地托管运行时
    Local,
    /// Remote endpoint adapter / 远端 endpoint 适配器
    Remote,
    /// Unknown or unsupported value / 未知或不支持的取值
    Unknown(String),
}

impl CanonicalBackendHosting {
    /// Parse hosting from a raw string boundary value.
    /// 从边界层原始字符串解析 hosting。
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "local" => Self::Local,
            "remote" => Self::Remote,
            other => Self::Unknown(other.to_string()),
        }
    }

    /// Return the canonical string form for stable serialization.
    /// 返回稳定序列化使用的规范字符串形式。
    pub fn as_str(&self) -> &str {
        match self {
            Self::Local => "local",
            Self::Remote => "remote",
            Self::Unknown(value) => value.as_str(),
        }
    }

    /// Convert to protobuf hosting enum.
    /// 转换为 protobuf hosting 枚举。
    pub fn to_proto_enum(&self) -> AiBackendHosting {
        match self {
            Self::Local => AiBackendHosting::Local,
            Self::Remote => AiBackendHosting::Remote,
            Self::Unknown(_) => AiBackendHosting::Unspecified,
        }
    }

    /// Parse from protobuf hosting enum value.
    /// 从 protobuf hosting 枚举值解析。
    pub fn from_proto_i32(value: i32) -> Self {
        match AiBackendHosting::try_from(value).unwrap_or(AiBackendHosting::Unspecified) {
            AiBackendHosting::Local => Self::Local,
            AiBackendHosting::Remote => Self::Remote,
            AiBackendHosting::Unspecified => Self::Unknown("unspecified".to_string()),
        }
    }
}

/// Canonical backend provider used by internal orchestration code.
/// 内部编排代码使用的规范 backend provider。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CanonicalBackendProvider {
    /// OpenAI or OpenAI-compatible provider family / OpenAI 或 OpenAI-compatible provider 家族
    OpenAi,
    /// Ollama provider family / Ollama provider 家族
    Ollama,
    /// Local llama.cpp managed runtime / 本地 llama.cpp 托管运行时
    LlamaCpp,
    /// Local vLLM family / 本地 vLLM 家族
    Vllm,
    /// Internal testing or stub provider / 内部测试或 stub provider
    Internal,
    /// Unknown or unsupported value / 未知或不支持的取值
    Unknown(String),
}

impl CanonicalBackendProvider {
    /// Parse provider from a raw string boundary value.
    /// 从边界层原始字符串解析 provider。
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "openai" | "openai_compatible" => Self::OpenAi,
            "ollama" => Self::Ollama,
            "llamacpp" | "llama_cpp" | "llama.cpp" => Self::LlamaCpp,
            "vllm" | "vllm-openai" => Self::Vllm,
            "internal" => Self::Internal,
            other => Self::Unknown(other.to_string()),
        }
    }

    /// Return the canonical string form for stable serialization.
    /// 返回稳定序列化使用的规范字符串形式。
    pub fn as_str(&self) -> &str {
        match self {
            Self::OpenAi => "openai",
            Self::Ollama => "ollama",
            Self::LlamaCpp => "llamacpp",
            Self::Vllm => "vllm",
            Self::Internal => "internal",
            Self::Unknown(value) => value.as_str(),
        }
    }
}

/// Canonical backend kind used to centralize hosting and provider rules.
/// 用于集中承载 hosting 与 provider 规则的规范 backend kind。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CanonicalBackendKind {
    /// OpenAI chat completion backend / OpenAI chat completion backend
    OpenAiChatCompletion,
    /// OpenAI realtime websocket backend / OpenAI realtime websocket backend
    OpenAiRealtimeWs,
    /// Ollama chat backend / Ollama chat backend
    OllamaChat,
    /// Local llama.cpp backend / 本地 llama.cpp backend
    LlamaCpp,
    /// Local vLLM backend / 本地 vLLM backend
    Vllm,
    /// Local vLLM OpenAI-compatible backend / 本地 vLLM OpenAI-compatible backend
    VllmOpenAi,
    /// Internal stub backend / 内部 stub backend
    Stub,
    /// Unknown or unsupported value / 未知或不支持的取值
    Unknown(String),
}

impl CanonicalBackendKind {
    /// Parse a backend kind from a raw string boundary value.
    /// 从边界层原始字符串解析 backend kind。
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "openai_chat_completion" => Self::OpenAiChatCompletion,
            "openai_realtime_ws" => Self::OpenAiRealtimeWs,
            "ollama_chat" => Self::OllamaChat,
            "llamacpp" | "llama_cpp" | "llama.cpp" => Self::LlamaCpp,
            "vllm" => Self::Vllm,
            "vllm-openai" => Self::VllmOpenAi,
            "stub" => Self::Stub,
            other => Self::Unknown(other.to_string()),
        }
    }

    /// Return the canonical string form for stable serialization.
    /// 返回稳定序列化使用的规范字符串形式。
    pub fn as_str(&self) -> &str {
        match self {
            Self::OpenAiChatCompletion => "openai_chat_completion",
            Self::OpenAiRealtimeWs => "openai_realtime_ws",
            Self::OllamaChat => "ollama_chat",
            Self::LlamaCpp => "llamacpp",
            Self::Vllm => "vllm",
            Self::VllmOpenAi => "vllm-openai",
            Self::Stub => "stub",
            Self::Unknown(value) => value.as_str(),
        }
    }

    /// Infer the default provider for one backend kind.
    /// 为一个 backend kind 推断默认 provider。
    pub fn inferred_provider(&self) -> CanonicalBackendProvider {
        match self {
            Self::OpenAiChatCompletion | Self::OpenAiRealtimeWs => CanonicalBackendProvider::OpenAi,
            Self::OllamaChat => CanonicalBackendProvider::Ollama,
            Self::LlamaCpp => CanonicalBackendProvider::LlamaCpp,
            Self::Vllm | Self::VllmOpenAi => CanonicalBackendProvider::Vllm,
            Self::Stub => CanonicalBackendProvider::Internal,
            Self::Unknown(_) => CanonicalBackendProvider::Unknown("unknown".to_string()),
        }
    }

    /// Return the allowed hosting family for one backend kind.
    /// 返回一个 backend kind 允许的 hosting 家族。
    pub fn allowed_hosting(&self) -> AllowedHosting {
        match self {
            Self::LlamaCpp | Self::Vllm | Self::VllmOpenAi => AllowedHosting::LocalOnly,
            Self::OpenAiChatCompletion | Self::OpenAiRealtimeWs => AllowedHosting::RemoteOnly,
            Self::OllamaChat => AllowedHosting::Any,
            Self::Stub | Self::Unknown(_) => AllowedHosting::Any,
        }
    }

    /// Check whether this kind supports local-model preflight.
    /// 检查该 kind 是否支持 local-model preflight。
    pub fn supports_local_model_preflight(&self) -> bool {
        match self {
            Self::LlamaCpp => true,
            _ => false,
        }
    }

    /// Check whether this kind supports remote provider preflight.
    /// 检查该 kind 是否支持 remote provider preflight。
    pub fn supports_remote_preflight(&self) -> bool {
        match self {
            Self::OpenAiChatCompletion | Self::OpenAiRealtimeWs | Self::OllamaChat => true,
            _ => false,
        }
    }
}

/// Allowed hosting family for one backend kind.
/// 单个 backend kind 允许的 hosting 家族。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AllowedHosting {
    /// Only local hosting is valid / 仅允许 local hosting
    LocalOnly,
    /// Only remote hosting is valid / 仅允许 remote hosting
    RemoteOnly,
    /// Both hosting kinds are acceptable / 两种 hosting 都可接受
    Any,
}

/// Canonical backend origin value used across config/admin boundaries.
/// 跨 config/admin 边界复用的规范 backend origin 取值。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CanonicalBackendOrigin {
    /// Originated from SMS control plane / 来源于 SMS 控制面
    Sms,
    /// Originated from static config / 来源于静态配置
    StaticConfig,
    /// Originated from local controller / 来源于本地控制器
    LocalController,
    /// Unknown or unsupported value / 未知或不支持的取值
    Unknown(String),
}

impl CanonicalBackendOrigin {
    /// Parse origin from an optional raw string value.
    /// 从可选原始字符串解析 origin。
    pub fn parse_optional(value: Option<&str>) -> Self {
        match value.map(|inner| inner.trim().to_ascii_lowercase()) {
            Some(inner) if inner == "sms" => Self::Sms,
            Some(inner) if inner == "static_config" => Self::StaticConfig,
            Some(inner) if inner == "local_controller" => Self::LocalController,
            Some(inner) => Self::Unknown(inner),
            None => Self::StaticConfig,
        }
    }

    /// Return the canonical string form for stable serialization.
    /// 返回稳定序列化使用的规范字符串形式。
    pub fn as_str(&self) -> &str {
        match self {
            Self::Sms => "sms",
            Self::StaticConfig => "static_config",
            Self::LocalController => "local_controller",
            Self::Unknown(value) => value.as_str(),
        }
    }

    /// Parse from protobuf origin enum value.
    /// 从 protobuf origin 枚举值解析。
    pub fn from_proto_i32(value: i32) -> Self {
        match BackendOrigin::try_from(value).ok() {
            Some(BackendOrigin::Sms) => Self::Sms,
            Some(BackendOrigin::StaticConfig) => Self::StaticConfig,
            Some(BackendOrigin::LocalController) => Self::LocalController,
            _ => Self::Unknown("unspecified".to_string()),
        }
    }

    /// Convert to protobuf origin enum.
    /// 转换为 protobuf origin 枚举。
    pub fn to_proto_enum(&self) -> BackendOrigin {
        match self {
            Self::Sms => BackendOrigin::Sms,
            Self::StaticConfig => BackendOrigin::StaticConfig,
            Self::LocalController => BackendOrigin::LocalController,
            Self::Unknown(_) => BackendOrigin::StaticConfig,
        }
    }
}

/// Canonical desired-state value used by admin input mapping.
/// 管理端输入映射复用的规范 desired-state 取值。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CanonicalBackendDesiredState {
    /// Enabled desired state / 启用态
    Enabled,
    /// Disabled desired state / 禁用态
    Disabled,
    /// Unknown or unsupported value / 未知或不支持的取值
    Unknown(String),
}

impl CanonicalBackendDesiredState {
    /// Parse desired-state from a raw string value.
    /// 从原始字符串解析 desired-state。
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "enabled" => Self::Enabled,
            "disabled" => Self::Disabled,
            other => Self::Unknown(other.to_string()),
        }
    }

    /// Parse from protobuf desired-state enum value.
    /// 从 protobuf desired-state 枚举值解析。
    pub fn from_proto_i32(value: i32) -> Self {
        match AiBackendDesiredState::try_from(value).unwrap_or(AiBackendDesiredState::Unspecified) {
            AiBackendDesiredState::Enabled => Self::Enabled,
            AiBackendDesiredState::Disabled => Self::Disabled,
            AiBackendDesiredState::Unspecified => Self::Unknown("unspecified".to_string()),
        }
    }

    /// Convert to protobuf desired-state enum.
    /// 转换为 protobuf desired-state 枚举。
    pub fn to_proto_enum(&self) -> AiBackendDesiredState {
        match self {
            Self::Enabled => AiBackendDesiredState::Enabled,
            Self::Disabled => AiBackendDesiredState::Disabled,
            Self::Unknown(_) => AiBackendDesiredState::Unspecified,
        }
    }
}

/// Canonical management-mode value used by admin input mapping.
/// 管理端输入映射复用的规范 management-mode 取值。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CanonicalBackendManagementMode {
    /// SMS-managed remote mode / SMS 管理的远端模式
    SmsRemote,
    /// SMS-managed local mode / SMS 管理的本地模式
    SmsLocal,
    /// Unknown or unsupported value / 未知或不支持的取值
    Unknown(String),
}

impl CanonicalBackendManagementMode {
    /// Parse management-mode from a raw string value.
    /// 从原始字符串解析 management-mode。
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "sms_remote" | "remote" => Self::SmsRemote,
            "sms_local" | "local" => Self::SmsLocal,
            other => Self::Unknown(other.to_string()),
        }
    }

    /// Parse from protobuf management-mode enum value.
    /// 从 protobuf management-mode 枚举值解析。
    pub fn from_proto_i32(value: i32) -> Self {
        match AiBackendManagementMode::try_from(value)
            .unwrap_or(AiBackendManagementMode::Unspecified)
        {
            AiBackendManagementMode::SmsRemote => Self::SmsRemote,
            AiBackendManagementMode::SmsLocal => Self::SmsLocal,
            AiBackendManagementMode::Unspecified => Self::Unknown("unspecified".to_string()),
        }
    }

    /// Convert to protobuf management-mode enum.
    /// 转换为 protobuf management-mode 枚举。
    pub fn to_proto_enum(&self) -> AiBackendManagementMode {
        match self {
            Self::SmsRemote => AiBackendManagementMode::SmsRemote,
            Self::SmsLocal => AiBackendManagementMode::SmsLocal,
            Self::Unknown(_) => AiBackendManagementMode::Unspecified,
        }
    }

    /// Derive default management-mode from hosting.
    /// 基于 hosting 推断默认 management-mode。
    pub fn default_for_hosting(hosting: &CanonicalBackendHosting) -> Self {
        match hosting {
            CanonicalBackendHosting::Remote => Self::SmsRemote,
            CanonicalBackendHosting::Local => Self::SmsLocal,
            CanonicalBackendHosting::Unknown(_) => Self::Unknown("unspecified".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AllowedHosting, CanonicalBackendDesiredState, CanonicalBackendHosting,
        CanonicalBackendKind, CanonicalBackendManagementMode, CanonicalBackendOrigin,
        CanonicalBackendProvider,
    };

    #[test]
    fn canonical_backend_kind_infers_provider_and_hosting_family() {
        let kind = CanonicalBackendKind::parse("openai_realtime_ws");
        assert_eq!(kind.inferred_provider(), CanonicalBackendProvider::OpenAi);
        assert_eq!(kind.allowed_hosting(), AllowedHosting::RemoteOnly);
    }

    #[test]
    fn canonical_ollama_kind_allows_both_local_and_remote_hosting() {
        let kind = CanonicalBackendKind::parse("ollama_chat");
        assert_eq!(kind.inferred_provider(), CanonicalBackendProvider::Ollama);
        assert_eq!(kind.allowed_hosting(), AllowedHosting::Any);
    }

    #[test]
    fn canonical_backend_kind_normalizes_llamacpp_aliases() {
        let kind = CanonicalBackendKind::parse("llama.cpp");
        assert_eq!(kind.as_str(), "llamacpp");
        assert!(kind.supports_local_model_preflight());
    }

    #[test]
    fn canonical_backend_hosting_parses_unknown_values() {
        assert_eq!(CanonicalBackendHosting::parse("local").as_str(), "local");
        assert_eq!(CanonicalBackendHosting::parse("edge").as_str(), "edge");
    }

    #[test]
    fn canonical_backend_origin_parses_and_serializes() {
        let origin = CanonicalBackendOrigin::parse_optional(Some("local_controller"));
        assert_eq!(origin.as_str(), "local_controller");
    }

    #[test]
    fn canonical_desired_state_parses_and_handles_unknown() {
        assert_eq!(
            CanonicalBackendDesiredState::parse("enabled"),
            CanonicalBackendDesiredState::Enabled
        );
        assert!(matches!(
            CanonicalBackendDesiredState::parse("paused"),
            CanonicalBackendDesiredState::Unknown(_)
        ));
    }

    #[test]
    fn canonical_management_mode_defaults_from_hosting() {
        assert_eq!(
            CanonicalBackendManagementMode::default_for_hosting(&CanonicalBackendHosting::Remote),
            CanonicalBackendManagementMode::SmsRemote
        );
        assert_eq!(
            CanonicalBackendManagementMode::default_for_hosting(&CanonicalBackendHosting::Local),
            CanonicalBackendManagementMode::SmsLocal
        );
    }
}
