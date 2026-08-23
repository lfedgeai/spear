use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};

use crate::ai_backend_types::{CanonicalBackendKind, CanonicalBackendProvider};
use crate::proto::sms::{BackendHosting, BackendOrigin, BackendSpec};
use crate::spearlet::ai::credential_resolver::global as global_credential_resolver;
use crate::spearlet::ai::remote_preflight::{
    default_requested_checks, preflight_remote_backend, RemoteBackendPreflightCheck,
};

use super::AppState;

/// Node-side backend preflight request.
/// 节点侧 backend 预检请求。
#[derive(Debug, Clone, Deserialize)]
pub(super) struct BackendPreflightRequest {
    pub(super) backend: BackendPreflightBackend,
    pub(super) requested_checks: Option<Vec<String>>,
}

/// Canonical backend draft used by node-side preflight.
/// 节点侧预检使用的规范 backend 草稿。
#[derive(Debug, Clone, Deserialize)]
pub(super) struct BackendPreflightBackend {
    pub(super) provider: String,
    pub(super) model: String,
    pub(super) hosting: String,
    pub(super) backend_kind: String,
    pub(super) base_url: Option<String>,
    pub(super) credential_ref: Option<String>,
    pub(super) operations: Option<Vec<String>>,
    pub(super) features: Option<Vec<String>>,
    pub(super) transports: Option<Vec<String>>,
}

/// Node-side backend preflight response.
/// 节点侧 backend 预检响应。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "outcome_kind", rename_all = "snake_case")]
pub(super) enum BackendPreflightOutcomeResponse {
    Success {
        phase: String,
        resolved_endpoint: Option<String>,
        http_status: Option<u16>,
        checks: Vec<RemoteBackendPreflightCheck>,
        auth_valid: Option<bool>,
        model_accessible: Option<bool>,
    },
    Failure {
        phase: String,
        error_code: String,
        resolved_endpoint: Option<String>,
        http_status: Option<u16>,
        provider_code: Option<String>,
        checks: Vec<RemoteBackendPreflightCheck>,
        auth_valid: Option<bool>,
        model_accessible: Option<bool>,
    },
}

/// Provider-specific OpenAI-compatible remote-preflight outcome.
/// OpenAI-compatible remote-preflight 的 provider-specific 结果。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "outcome_kind", rename_all = "snake_case")]
pub(super) enum BackendPreflightOpenAiCompatibleOutcomeResponse {
    ConnectivityVerified {
        resolved_endpoint: Option<String>,
        http_status: Option<u16>,
        checks: Vec<RemoteBackendPreflightCheck>,
    },
    AuthVerified {
        resolved_endpoint: Option<String>,
        http_status: Option<u16>,
        checks: Vec<RemoteBackendPreflightCheck>,
        auth_valid: bool,
    },
    ModelAccessVerified {
        resolved_endpoint: Option<String>,
        http_status: Option<u16>,
        checks: Vec<RemoteBackendPreflightCheck>,
        auth_valid: bool,
        model_accessible: bool,
    },
    ConnectivityFailed {
        error_code: String,
        resolved_endpoint: Option<String>,
        http_status: Option<u16>,
        provider_code: Option<String>,
        checks: Vec<RemoteBackendPreflightCheck>,
    },
    AuthFailed {
        error_code: String,
        resolved_endpoint: Option<String>,
        http_status: Option<u16>,
        provider_code: Option<String>,
        checks: Vec<RemoteBackendPreflightCheck>,
        auth_valid: bool,
    },
    ModelAccessFailed {
        error_code: String,
        resolved_endpoint: Option<String>,
        http_status: Option<u16>,
        provider_code: Option<String>,
        checks: Vec<RemoteBackendPreflightCheck>,
        auth_valid: bool,
        model_accessible: bool,
    },
}

/// Provider-specific Ollama remote-preflight outcome.
/// Ollama remote-preflight 的 provider-specific 结果。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "outcome_kind", rename_all = "snake_case")]
pub(super) enum BackendPreflightOllamaOutcomeResponse {
    ConnectivityVerified {
        resolved_endpoint: Option<String>,
        http_status: Option<u16>,
        checks: Vec<RemoteBackendPreflightCheck>,
    },
    ModelAccessVerified {
        resolved_endpoint: Option<String>,
        http_status: Option<u16>,
        checks: Vec<RemoteBackendPreflightCheck>,
        model_accessible: bool,
    },
    ConnectivityFailed {
        error_code: String,
        resolved_endpoint: Option<String>,
        http_status: Option<u16>,
        provider_code: Option<String>,
        checks: Vec<RemoteBackendPreflightCheck>,
    },
    ModelAccessFailed {
        error_code: String,
        resolved_endpoint: Option<String>,
        http_status: Option<u16>,
        provider_code: Option<String>,
        checks: Vec<RemoteBackendPreflightCheck>,
        model_accessible: bool,
    },
}

/// Typed provider-family result for node-side remote preflight.
/// 节点侧 remote preflight 的强类型 provider-family 结果。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "provider_family", rename_all = "snake_case")]
pub(super) enum BackendPreflightProviderResultResponse {
    OpenAiCompatible {
        outcome: BackendPreflightOpenAiCompatibleOutcomeResponse,
    },
    Ollama {
        outcome: BackendPreflightOllamaOutcomeResponse,
    },
    Unknown {
        provider: String,
        outcome: BackendPreflightOutcomeResponse,
    },
}

/// Node-side backend preflight response envelope.
/// 节点侧 backend 预检响应包裹结构。
#[derive(Debug, Clone, Serialize)]
pub(super) struct BackendPreflightResponse {
    pub(super) success: bool,
    pub(super) result: BackendPreflightProviderResultResponse,
    pub(super) message: String,
    pub(super) latency_ms: Option<u64>,
}

/// Typed provider family for node-side remote preflight DTO mapping.
/// 节点侧 remote preflight DTO 映射使用的强类型 provider 家族。
#[derive(Debug, Clone, PartialEq, Eq)]
enum BackendPreflightProviderFamily {
    OpenAiCompatible,
    Ollama,
    Unknown(String),
}

impl BackendPreflightProviderFamily {
    /// Parse provider family from backend input boundary values.
    /// 从 backend 输入边界值解析 provider family。
    fn parse(provider: &str, backend_kind: &str) -> Self {
        match CanonicalBackendProvider::parse(provider) {
            CanonicalBackendProvider::OpenAi => Self::OpenAiCompatible,
            CanonicalBackendProvider::Ollama => Self::Ollama,
            CanonicalBackendProvider::Unknown(_) => {
                match CanonicalBackendKind::parse(backend_kind).inferred_provider() {
                    CanonicalBackendProvider::OpenAi => Self::OpenAiCompatible,
                    CanonicalBackendProvider::Ollama => Self::Ollama,
                    _ => Self::Unknown(provider.trim().to_string()),
                }
            }
            _ => Self::Unknown(provider.trim().to_string()),
        }
    }

    /// Wrap one typed outcome into the provider-family result.
    /// 将一个强类型 outcome 包装成 provider-family 结果。
    fn wrap(self, outcome: BackendPreflightOutcomeResponse) -> BackendPreflightProviderResultResponse {
        match self {
            Self::OpenAiCompatible => {
                BackendPreflightProviderResultResponse::OpenAiCompatible {
                    outcome: BackendPreflightOpenAiCompatibleOutcomeResponse::from_generic(
                        outcome,
                    ),
                }
            }
            Self::Ollama => BackendPreflightProviderResultResponse::Ollama {
                outcome: BackendPreflightOllamaOutcomeResponse::from_generic(outcome),
            },
            Self::Unknown(provider) => {
                BackendPreflightProviderResultResponse::Unknown { provider, outcome }
            }
        }
    }
}

impl BackendPreflightOpenAiCompatibleOutcomeResponse {
    /// Convert one generic remote-preflight outcome into the OpenAI-compatible shape.
    /// 将通用 remote-preflight 结果转换为 OpenAI-compatible 形态。
    fn from_generic(outcome: BackendPreflightOutcomeResponse) -> Self {
        match outcome {
            BackendPreflightOutcomeResponse::Success {
                phase,
                resolved_endpoint,
                http_status,
                checks,
                auth_valid,
                model_accessible,
            } => match phase.as_str() {
                "model_access" => Self::ModelAccessVerified {
                    resolved_endpoint,
                    http_status,
                    checks,
                    auth_valid: auth_valid.unwrap_or(true),
                    model_accessible: model_accessible.unwrap_or(true),
                },
                "auth" => Self::AuthVerified {
                    resolved_endpoint,
                    http_status,
                    checks,
                    auth_valid: auth_valid.unwrap_or(true),
                },
                _ => Self::ConnectivityVerified {
                    resolved_endpoint,
                    http_status,
                    checks,
                },
            },
            BackendPreflightOutcomeResponse::Failure {
                phase,
                error_code,
                resolved_endpoint,
                http_status,
                provider_code,
                checks,
                auth_valid,
                model_accessible,
            } => match phase.as_str() {
                "model_access" => Self::ModelAccessFailed {
                    error_code,
                    resolved_endpoint,
                    http_status,
                    provider_code,
                    checks,
                    // Failure at model-access phase implies auth already passed.
                    // model_access 阶段失败意味着 auth 已经通过。
                    auth_valid: auth_valid.unwrap_or(true),
                    model_accessible: model_accessible.unwrap_or(false),
                },
                "auth" => Self::AuthFailed {
                    error_code,
                    resolved_endpoint,
                    http_status,
                    provider_code,
                    checks,
                    auth_valid: auth_valid.unwrap_or(false),
                },
                _ => Self::ConnectivityFailed {
                    error_code,
                    resolved_endpoint,
                    http_status,
                    provider_code,
                    checks,
                },
            },
        }
    }
}

impl BackendPreflightOllamaOutcomeResponse {
    /// Convert one generic remote-preflight outcome into the Ollama shape.
    /// 将通用 remote-preflight 结果转换为 Ollama 形态。
    fn from_generic(outcome: BackendPreflightOutcomeResponse) -> Self {
        match outcome {
            BackendPreflightOutcomeResponse::Success {
                phase,
                resolved_endpoint,
                http_status,
                checks,
                model_accessible,
                ..
            } => match phase.as_str() {
                "model_access" => Self::ModelAccessVerified {
                    resolved_endpoint,
                    http_status,
                    checks,
                    model_accessible: model_accessible.unwrap_or(true),
                },
                _ => Self::ConnectivityVerified {
                    resolved_endpoint,
                    http_status,
                    checks,
                },
            },
            BackendPreflightOutcomeResponse::Failure {
                phase,
                error_code,
                resolved_endpoint,
                http_status,
                provider_code,
                checks,
                model_accessible,
                ..
            } => match phase.as_str() {
                "model_access" => Self::ModelAccessFailed {
                    error_code,
                    resolved_endpoint,
                    http_status,
                    provider_code,
                    checks,
                    model_accessible: model_accessible.unwrap_or(false),
                },
                _ => Self::ConnectivityFailed {
                    error_code,
                    resolved_endpoint,
                    http_status,
                    provider_code,
                    checks,
                },
            },
        }
    }
}

/// Build a typed remote-preflight failure response.
/// 构建强类型 remote-preflight 失败响应。
fn backend_preflight_failure_response(
    provider: String,
    backend_kind: String,
    phase: String,
    error_code: String,
    message: String,
) -> BackendPreflightResponse {
    BackendPreflightResponse {
        success: false,
        result: BackendPreflightProviderFamily::parse(&provider, &backend_kind).wrap(
            BackendPreflightOutcomeResponse::Failure {
            phase,
            error_code,
            resolved_endpoint: None,
            http_status: None,
            provider_code: None,
            checks: vec![],
            auth_valid: None,
            model_accessible: None,
        }),
        message,
        latency_ms: None,
    }
}

/// POST /internal/ai/backends/preflight
/// 在 SPEARlet 节点本地验证 remote backend 的可达性与权限。
pub(super) async fn preflight_backend(
    State(_state): State<AppState>,
    Json(body): Json<BackendPreflightRequest>,
) -> (StatusCode, Json<BackendPreflightResponse>) {
    if body.backend.hosting.trim().to_ascii_lowercase() != "remote" {
        return (
            StatusCode::BAD_REQUEST,
            Json(backend_preflight_failure_response(
                body.backend.provider.trim().to_string(),
                body.backend.backend_kind.trim().to_string(),
                "input".to_string(),
                "invalid_input".to_string(),
                "only remote backend preflight is supported on this route".to_string(),
            )),
        );
    }

    let Some(resolver) = global_credential_resolver() else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(backend_preflight_failure_response(
                body.backend.provider.trim().to_string(),
                body.backend.backend_kind.trim().to_string(),
                "input".to_string(),
                "internal_error".to_string(),
                "global credential resolver is not initialized".to_string(),
            )),
        );
    };

    let spec = BackendSpec {
        name: "preflight-backend".to_string(),
        kind: body.backend.backend_kind.trim().to_string(),
        operations: body.backend.operations.unwrap_or_default(),
        features: body.backend.features.unwrap_or_default(),
        transports: body.backend.transports.unwrap_or_default(),
        weight: 100,
        priority: 0,
        base_url: body.backend.base_url.unwrap_or_default().trim().to_string(),
        provider: body.backend.provider.trim().to_string(),
        model: body.backend.model.trim().to_string(),
        credential_ref: body
            .backend
            .credential_ref
            .unwrap_or_default()
            .trim()
            .to_string(),
        hosting: BackendHosting::Remote as i32,
        origin: BackendOrigin::Sms as i32,
        deployment_id: String::new(),
    };

    let requested_checks = body
        .requested_checks
        .unwrap_or_else(|| default_requested_checks(&spec));
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    let report = preflight_remote_backend(&http, &resolver, &spec, &requested_checks).await;
    let status = if report.success {
        StatusCode::OK
    } else {
        StatusCode::BAD_REQUEST
    };

    (
        status,
        Json(BackendPreflightResponse {
            success: report.success,
            result: BackendPreflightProviderFamily::parse(
                &report.provider,
                &body.backend.backend_kind,
            )
            .wrap(if report.success {
                BackendPreflightOutcomeResponse::Success {
                    phase: report.phase,
                    resolved_endpoint: report.resolved_endpoint,
                    http_status: report.http_status,
                    checks: report.checks,
                    auth_valid: report.auth_valid,
                    model_accessible: report.model_accessible,
                }
            } else {
                BackendPreflightOutcomeResponse::Failure {
                    phase: report.phase,
                    error_code: report
                        .error_code
                        .unwrap_or_else(|| "unknown_failure".to_string()),
                    resolved_endpoint: report.resolved_endpoint,
                    http_status: report.http_status,
                    provider_code: report.provider_code,
                    checks: report.checks,
                    auth_valid: report.auth_valid,
                    model_accessible: report.model_accessible,
                }
            }),
            message: report.message,
            latency_ms: report.latency_ms,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_preflight_provider_family_parses_openai_from_kind_alias() {
        assert_eq!(
            BackendPreflightProviderFamily::parse("", "openai_realtime_ws"),
            BackendPreflightProviderFamily::OpenAiCompatible
        );
    }

    #[test]
    fn backend_preflight_provider_family_preserves_unknown_provider() {
        assert_eq!(
            BackendPreflightProviderFamily::parse("custom-provider", "custom-kind"),
            BackendPreflightProviderFamily::Unknown("custom-provider".to_string())
        );
    }

    #[test]
    fn openai_compatible_outcome_refines_model_access_success() {
        let outcome = BackendPreflightOpenAiCompatibleOutcomeResponse::from_generic(
            BackendPreflightOutcomeResponse::Success {
                phase: "model_access".to_string(),
                resolved_endpoint: Some("https://api.openai.com/v1/models".to_string()),
                http_status: Some(200),
                checks: vec![RemoteBackendPreflightCheck {
                    name: "model_access".to_string(),
                    ok: true,
                }],
                auth_valid: Some(true),
                model_accessible: Some(true),
            },
        );

        assert!(matches!(
            outcome,
            BackendPreflightOpenAiCompatibleOutcomeResponse::ModelAccessVerified {
                resolved_endpoint: Some(_),
                http_status: Some(200),
                auth_valid: true,
                model_accessible: true,
                ..
            }
        ));
    }

    #[test]
    fn ollama_outcome_refines_connectivity_success() {
        let outcome = BackendPreflightOllamaOutcomeResponse::from_generic(
            BackendPreflightOutcomeResponse::Success {
                phase: "connectivity".to_string(),
                resolved_endpoint: Some("http://ollama.local/api/tags".to_string()),
                http_status: Some(200),
                checks: vec![RemoteBackendPreflightCheck {
                    name: "connectivity".to_string(),
                    ok: true,
                }],
                auth_valid: None,
                model_accessible: None,
            },
        );

        assert!(matches!(
            outcome,
            BackendPreflightOllamaOutcomeResponse::ConnectivityVerified {
                resolved_endpoint: Some(_),
                http_status: Some(200),
                ..
            }
        ));
    }

    #[test]
    fn openai_compatible_outcome_refines_auth_failure() {
        let outcome = BackendPreflightOpenAiCompatibleOutcomeResponse::from_generic(
            BackendPreflightOutcomeResponse::Failure {
                phase: "auth".to_string(),
                error_code: "auth_invalid".to_string(),
                resolved_endpoint: Some("https://api.openai.com/v1/models".to_string()),
                http_status: Some(401),
                provider_code: Some("invalid_api_key".to_string()),
                checks: vec![RemoteBackendPreflightCheck {
                    name: "auth".to_string(),
                    ok: false,
                }],
                auth_valid: Some(false),
                model_accessible: None,
            },
        );

        assert!(matches!(
            outcome,
            BackendPreflightOpenAiCompatibleOutcomeResponse::AuthFailed {
                http_status: Some(401),
                auth_valid: false,
                ..
            }
        ));
    }

    #[test]
    fn ollama_outcome_refines_model_access_failure() {
        let outcome = BackendPreflightOllamaOutcomeResponse::from_generic(
            BackendPreflightOutcomeResponse::Failure {
                phase: "model_access".to_string(),
                error_code: "model_not_found".to_string(),
                resolved_endpoint: Some("http://ollama.local/api/tags".to_string()),
                http_status: Some(200),
                provider_code: None,
                checks: vec![RemoteBackendPreflightCheck {
                    name: "model_access".to_string(),
                    ok: false,
                }],
                auth_valid: None,
                model_accessible: Some(false),
            },
        );

        assert!(matches!(
            outcome,
            BackendPreflightOllamaOutcomeResponse::ModelAccessFailed {
                http_status: Some(200),
                model_accessible: false,
                ..
            }
        ));
    }
}
