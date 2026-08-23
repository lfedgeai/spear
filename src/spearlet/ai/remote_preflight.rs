use std::time::Instant;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ai_backend_types::{CanonicalBackendKind, CanonicalBackendProvider};
use crate::proto::sms::BackendSpec;
use crate::spearlet::ai::credential_resolver::{CredentialResolution, CredentialResolver};
use crate::spearlet::execution::ai::backends::http_json::join_url;

/// Supported remote preflight provider families.
/// 当前支持的 remote preflight provider 家族。
#[derive(Debug, Clone, PartialEq, Eq)]
enum RemotePreflightProviderFamily {
    /// OpenAI-compatible HTTP family / OpenAI-compatible HTTP 家族
    OpenAiCompatible,
    /// Ollama-compatible HTTP family / Ollama-compatible HTTP 家族
    Ollama,
    /// Unsupported provider family / 不支持的 provider 家族
    Unknown(String),
}

impl RemotePreflightProviderFamily {
    /// Classify one backend spec into a remote preflight provider family.
    /// 将一个 backend spec 分类到 remote preflight provider 家族。
    fn from_spec(spec: &BackendSpec) -> Self {
        let raw_provider = spec.provider.trim().to_ascii_lowercase();
        let raw_kind = spec.kind.trim().to_ascii_lowercase();
        let provider = CanonicalBackendProvider::parse(&spec.provider);
        let kind = CanonicalBackendKind::parse(&spec.kind);

        if provider == CanonicalBackendProvider::Ollama || kind == CanonicalBackendKind::OllamaChat
        {
            return Self::Ollama;
        }
        if provider == CanonicalBackendProvider::OpenAi
            || matches!(
                kind,
                CanonicalBackendKind::OpenAiChatCompletion | CanonicalBackendKind::OpenAiRealtimeWs
            )
            || raw_kind.starts_with("openai_")
        {
            return Self::OpenAiCompatible;
        }
        if !raw_provider.is_empty() {
            Self::Unknown(raw_provider)
        } else {
            Self::Unknown(raw_kind)
        }
    }

    /// Return the stable serialized family name.
    /// 返回稳定序列化使用的家族名称。
    fn as_str(&self) -> &str {
        match self {
            Self::OpenAiCompatible => "openai_compatible",
            Self::Ollama => "ollama",
            Self::Unknown(value) => value.as_str(),
        }
    }

    /// Return the default requested checks for one provider family.
    /// 返回一个 provider 家族的默认检查项。
    fn default_requested_checks(&self) -> Vec<String> {
        match self {
            Self::Ollama => vec!["connectivity".to_string(), "model_access".to_string()],
            Self::OpenAiCompatible | Self::Unknown(_) => vec![
                "connectivity".to_string(),
                "auth".to_string(),
                "model_access".to_string(),
            ],
        }
    }
}

/// One named preflight check outcome.
/// 单个具名预检检查项结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteBackendPreflightCheck {
    pub name: String,
    pub ok: bool,
}

/// Node-side remote backend preflight report.
/// 节点侧 remote backend 预检报告。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteBackendPreflightReport {
    pub success: bool,
    pub provider: String,
    pub phase: String,
    pub error_code: Option<String>,
    pub message: String,
    pub resolved_endpoint: Option<String>,
    pub http_status: Option<u16>,
    pub provider_code: Option<String>,
    pub latency_ms: Option<u64>,
    pub checks: Vec<RemoteBackendPreflightCheck>,
    pub auth_valid: Option<bool>,
    pub model_accessible: Option<bool>,
}

impl RemoteBackendPreflightReport {
    /// Create a successful report with consistent defaults.
    /// 构造一个具有一致默认值的成功报告。
    fn success(
        provider: &str,
        phase: &str,
        message: String,
        resolved_endpoint: Option<String>,
        http_status: Option<u16>,
        latency_ms: u64,
        checks: Vec<RemoteBackendPreflightCheck>,
        auth_valid: Option<bool>,
        model_accessible: Option<bool>,
    ) -> Self {
        Self {
            success: true,
            provider: provider.to_string(),
            phase: phase.to_string(),
            error_code: None,
            message,
            resolved_endpoint,
            http_status,
            provider_code: None,
            latency_ms: Some(latency_ms),
            checks,
            auth_valid,
            model_accessible,
        }
    }

    /// Create a failed report with consistent defaults.
    /// 构造一个具有一致默认值的失败报告。
    fn failure(
        provider: &str,
        phase: &str,
        error_code: &str,
        message: String,
        resolved_endpoint: Option<String>,
        http_status: Option<u16>,
        provider_code: Option<String>,
        latency_ms: u64,
        checks: Vec<RemoteBackendPreflightCheck>,
        auth_valid: Option<bool>,
        model_accessible: Option<bool>,
    ) -> Self {
        Self {
            success: false,
            provider: provider.to_string(),
            phase: phase.to_string(),
            error_code: Some(error_code.to_string()),
            message,
            resolved_endpoint,
            http_status,
            provider_code,
            latency_ms: Some(latency_ms),
            checks,
            auth_valid,
            model_accessible,
        }
    }
}

/// Probe a remote backend from the real node-side network path.
/// 从真实节点侧网络路径探测 remote backend。
pub async fn preflight_remote_backend(
    http: &Client,
    resolver: &CredentialResolver,
    spec: &BackendSpec,
    requested_checks: &[String],
) -> RemoteBackendPreflightReport {
    let started = Instant::now();
    let provider_family = RemotePreflightProviderFamily::from_spec(spec);
    let requested = normalized_requested_checks(requested_checks);
    let base_url = spec.base_url.trim().to_string();

    if base_url.is_empty() {
        return RemoteBackendPreflightReport::failure(
            provider_family.as_str(),
            "input",
            "invalid_input",
            "remote backends require spec.base_url".to_string(),
            None,
            None,
            None,
            started.elapsed().as_millis() as u64,
            vec![],
            None,
            None,
        );
    }

    match provider_family {
        RemotePreflightProviderFamily::OpenAiCompatible => {
            preflight_openai_compatible(http, resolver, spec, &requested, started).await
        }
        RemotePreflightProviderFamily::Ollama => {
            preflight_ollama(http, spec, &requested, started).await
        }
        RemotePreflightProviderFamily::Unknown(provider) => RemoteBackendPreflightReport::failure(
            &provider,
            "input",
            "unsupported_provider",
            format!(
                "remote preflight does not support provider '{}' with kind '{}'",
                spec.provider, spec.kind
            ),
            Some(base_url),
            None,
            None,
            started.elapsed().as_millis() as u64,
            vec![],
            None,
            None,
        ),
    }
}

/// Build the default check list for one backend spec.
/// 为一个 backend spec 构建默认检查项列表。
pub fn default_requested_checks(spec: &BackendSpec) -> Vec<String> {
    RemotePreflightProviderFamily::from_spec(spec).default_requested_checks()
}

fn normalized_requested_checks(requested_checks: &[String]) -> Vec<String> {
    let mut out = requested_checks
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    out.sort();
    out.dedup();
    if !out.is_empty() {
        return out;
    }
    vec![
        "connectivity".to_string(),
        "auth".to_string(),
        "model_access".to_string(),
    ]
}

fn check_enabled(requested: &[String], check: &str) -> bool {
    requested.iter().any(|value| value == check)
}

fn checks_with_default_values(requested: &[String]) -> Vec<RemoteBackendPreflightCheck> {
    requested
        .iter()
        .map(|name| RemoteBackendPreflightCheck {
            name: name.clone(),
            ok: false,
        })
        .collect()
}

fn mark_check(checks: &mut [RemoteBackendPreflightCheck], name: &str, ok: bool) {
    if let Some(check) = checks.iter_mut().find(|check| check.name == name) {
        check.ok = ok;
    }
}

fn credential_error_code(state: &CredentialResolution) -> &'static str {
    match state {
        CredentialResolution::Ready(_) => "ok",
        CredentialResolution::Disabled => "credential_disabled",
        CredentialResolution::NotSynced => "credential_not_synced",
        CredentialResolution::Missing => "auth_missing",
    }
}

fn extract_provider_code(payload: &Value) -> Option<String> {
    payload
        .get("error")
        .and_then(|error| error.get("code"))
        .and_then(|value| {
            value
                .as_str()
                .map(ToOwned::to_owned)
                .or_else(|| value.as_i64().map(|number| number.to_string()))
        })
}

fn map_http_failure(
    _provider: &str,
    phase: &str,
    status: u16,
    payload: Option<&Value>,
) -> (&'static str, Option<String>, String) {
    let provider_code = payload.and_then(extract_provider_code);
    let provider_message = payload
        .and_then(|body| body.get("error"))
        .and_then(|error| error.get("message"))
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned);
    let message = provider_message.unwrap_or_else(|| format!("upstream status: {}", status));
    let error_code = match status {
        401 => "auth_invalid",
        403 => "permission_denied",
        404 => "http_404",
        429 => "provider_rate_limited",
        500..=599 => "provider_unavailable",
        _ if phase == "auth" => "auth_invalid",
        _ => "provider_unavailable",
    };
    (error_code, provider_code, message)
}

fn map_reqwest_error_code(error: &reqwest::Error) -> &'static str {
    let message = error.to_string().to_ascii_lowercase();
    if error.is_timeout() {
        "connect_timeout"
    } else if message.contains("dns") || message.contains("lookup address") {
        "dns_resolve_failed"
    } else if message.contains("certificate") || message.contains("tls") || message.contains("ssl")
    {
        "tls_handshake_failed"
    } else if error.is_connect() {
        "node_unreachable"
    } else {
        "provider_unavailable"
    }
}

async fn preflight_openai_compatible(
    http: &Client,
    resolver: &CredentialResolver,
    spec: &BackendSpec,
    requested: &[String],
    started: Instant,
) -> RemoteBackendPreflightReport {
    let endpoint = if spec.base_url.contains("/v1") {
        join_url(&spec.base_url, "models")
    } else {
        join_url(&spec.base_url, "v1/models")
    };
    let mut checks = checks_with_default_values(requested);
    let credential_state = resolver.resolve_api_key_state(Some(&spec.credential_ref));
    let api_key = match credential_state.clone() {
        CredentialResolution::Ready(secret) => Some(secret),
        state if check_enabled(requested, "auth") => {
            return RemoteBackendPreflightReport::failure(
                "openai_compatible",
                "auth",
                credential_error_code(&state),
                state.message(spec.credential_ref.as_str()),
                Some(endpoint),
                None,
                None,
                started.elapsed().as_millis() as u64,
                checks,
                Some(false),
                None,
            );
        }
        _ => None,
    };

    let mut request = http
        .get(&endpoint)
        .timeout(std::time::Duration::from_secs(10));
    if let Some(api_key) = api_key.as_deref() {
        request = request.bearer_auth(api_key);
    }

    let response = match request.send().await {
        Ok(response) => response,
        Err(error) => {
            return RemoteBackendPreflightReport::failure(
                "openai_compatible",
                "connectivity",
                map_reqwest_error_code(&error),
                error.to_string(),
                Some(endpoint),
                None,
                None,
                started.elapsed().as_millis() as u64,
                checks,
                None,
                None,
            );
        }
    };

    mark_check(&mut checks, "connectivity", true);
    let status = response.status().as_u16();
    let payload = response.json::<Value>().await.ok();
    if !(200..300).contains(&status) {
        let phase = if matches!(status, 401 | 403) {
            "auth"
        } else {
            "connectivity"
        };
        let (error_code, provider_code, message) =
            map_http_failure("openai_compatible", phase, status, payload.as_ref());
        return RemoteBackendPreflightReport::failure(
            "openai_compatible",
            phase,
            error_code,
            message,
            Some(endpoint),
            Some(status),
            provider_code,
            started.elapsed().as_millis() as u64,
            checks,
            Some(false),
            None,
        );
    }

    if check_enabled(requested, "auth") {
        mark_check(&mut checks, "auth", true);
    }

    let model = spec.model.trim();
    if check_enabled(requested, "model_access") && !model.is_empty() {
        let found = payload
            .as_ref()
            .and_then(|body| body.get("data"))
            .and_then(|value| value.as_array())
            .map(|items| {
                items.iter().any(|item| {
                    item.get("id")
                        .and_then(|value| value.as_str())
                        .map(|value| value == model)
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);
        if !found {
            return RemoteBackendPreflightReport::failure(
                "openai_compatible",
                "model_access",
                "model_not_found",
                format!("model '{}' is not visible from this node", model),
                Some(endpoint),
                Some(status),
                None,
                started.elapsed().as_millis() as u64,
                checks,
                Some(true),
                Some(false),
            );
        }
        mark_check(&mut checks, "model_access", true);
    }

    RemoteBackendPreflightReport::success(
        "openai_compatible",
        if check_enabled(requested, "model_access") && !model.is_empty() {
            "model_access"
        } else if check_enabled(requested, "auth") {
            "auth"
        } else {
            "connectivity"
        },
        "authentication and model access verified".to_string(),
        Some(endpoint),
        Some(status),
        started.elapsed().as_millis() as u64,
        checks,
        Some(true),
        if check_enabled(requested, "model_access") && !model.is_empty() {
            Some(true)
        } else {
            None
        },
    )
}

async fn preflight_ollama(
    http: &Client,
    spec: &BackendSpec,
    requested: &[String],
    started: Instant,
) -> RemoteBackendPreflightReport {
    let endpoint = if spec.base_url.contains("/api/") || spec.base_url.ends_with("/api") {
        join_url(&spec.base_url, "tags")
    } else {
        join_url(&spec.base_url, "api/tags")
    };
    let mut checks = checks_with_default_values(requested);
    let response = match http
        .get(&endpoint)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => {
            return RemoteBackendPreflightReport::failure(
                "ollama",
                "connectivity",
                map_reqwest_error_code(&error),
                error.to_string(),
                Some(endpoint),
                None,
                None,
                started.elapsed().as_millis() as u64,
                checks,
                None,
                None,
            );
        }
    };

    mark_check(&mut checks, "connectivity", true);
    let status = response.status().as_u16();
    let payload = response.json::<Value>().await.ok();
    if !(200..300).contains(&status) {
        let (error_code, provider_code, message) =
            map_http_failure("ollama", "connectivity", status, payload.as_ref());
        return RemoteBackendPreflightReport::failure(
            "ollama",
            "connectivity",
            error_code,
            message,
            Some(endpoint),
            Some(status),
            provider_code,
            started.elapsed().as_millis() as u64,
            checks,
            None,
            None,
        );
    }

    let model = spec.model.trim();
    if check_enabled(requested, "model_access") && !model.is_empty() {
        let found = payload
            .as_ref()
            .and_then(|body| body.get("models"))
            .and_then(|value| value.as_array())
            .map(|items| {
                items.iter().any(|item| {
                    item.get("name")
                        .and_then(|value| value.as_str())
                        .map(|value| value == model || value.starts_with(&format!("{model}:")))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);
        if !found {
            return RemoteBackendPreflightReport::failure(
                "ollama",
                "model_access",
                "model_not_found",
                format!("model '{}' is not available from this node", model),
                Some(endpoint),
                Some(status),
                None,
                started.elapsed().as_millis() as u64,
                checks,
                None,
                Some(false),
            );
        }
        mark_check(&mut checks, "model_access", true);
    }

    RemoteBackendPreflightReport::success(
        "ollama",
        if check_enabled(requested, "model_access") && !model.is_empty() {
            "model_access"
        } else {
            "connectivity"
        },
        "endpoint reachability and model access verified".to_string(),
        Some(endpoint),
        Some(status),
        started.elapsed().as_millis() as u64,
        checks,
        None,
        if check_enabled(requested, "model_access") && !model.is_empty() {
            Some(true)
        } else {
            None
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::get, Json, Router};
    use serde_json::json;
    use tokio::net::TcpListener;

    async fn start_json_server(
        path: &'static str,
        payload: Value,
    ) -> (tokio::task::JoinHandle<()>, String) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let app = Router::new().route(
            path,
            get(move || {
                let payload = payload.clone();
                async move { Json(payload) }
            }),
        );
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve");
        });
        (handle, format!("http://{}", addr))
    }

    fn test_backend_spec(kind: &str, provider: &str) -> BackendSpec {
        BackendSpec {
            name: "backend".to_string(),
            kind: kind.to_string(),
            operations: vec!["chat_completions".to_string()],
            features: vec![],
            transports: vec!["http".to_string()],
            weight: 100,
            priority: 0,
            base_url: "https://example.invalid".to_string(),
            provider: provider.to_string(),
            model: "demo".to_string(),
            hosting: crate::proto::sms::BackendHosting::Remote as i32,
            credential_ref: String::new(),
            origin: crate::proto::sms::BackendOrigin::Sms as i32,
            deployment_id: String::new(),
        }
    }

    #[test]
    fn remote_preflight_provider_family_normalizes_openai_aliases() {
        let spec = test_backend_spec("openai_realtime_ws", "openai_compatible");
        assert_eq!(
            RemotePreflightProviderFamily::from_spec(&spec),
            RemotePreflightProviderFamily::OpenAiCompatible
        );
    }

    #[test]
    fn remote_preflight_provider_family_infers_ollama_from_kind() {
        let spec = test_backend_spec("ollama_chat", "");
        assert_eq!(
            RemotePreflightProviderFamily::from_spec(&spec),
            RemotePreflightProviderFamily::Ollama
        );
    }

    #[test]
    fn default_requested_checks_follow_provider_family() {
        let openai_spec = test_backend_spec("openai_chat_completion", "");
        let ollama_spec = test_backend_spec("ollama_chat", "");

        assert_eq!(
            default_requested_checks(&openai_spec),
            vec![
                "connectivity".to_string(),
                "auth".to_string(),
                "model_access".to_string(),
            ]
        );
        assert_eq!(
            default_requested_checks(&ollama_spec),
            vec!["connectivity".to_string(), "model_access".to_string()]
        );
    }

    #[tokio::test]
    async fn openai_preflight_verifies_model_access() {
        let (handle, base_url) = start_json_server(
            "/v1/models",
            json!({
                "data": [{"id": "gpt-4o-mini"}]
            }),
        )
        .await;
        let resolver =
            CredentialResolver::from_config(&crate::spearlet::config::SpearletConfig::default());
        let spec = BackendSpec {
            name: "backend-1".to_string(),
            kind: "openai_chat_completion".to_string(),
            operations: vec!["chat_completions".to_string()],
            features: vec![],
            transports: vec!["http".to_string()],
            weight: 100,
            priority: 0,
            base_url,
            provider: "openai".to_string(),
            model: "gpt-4o-mini".to_string(),
            hosting: crate::proto::sms::BackendHosting::Remote as i32,
            credential_ref: String::new(),
            origin: crate::proto::sms::BackendOrigin::Sms as i32,
            deployment_id: String::new(),
        };

        let report = preflight_remote_backend(
            &Client::new(),
            &resolver,
            &spec,
            &["connectivity".to_string(), "model_access".to_string()],
        )
        .await;

        assert!(report.success);
        assert_eq!(report.phase, "model_access");
        assert_eq!(report.model_accessible, Some(true));
        handle.abort();
    }

    #[tokio::test]
    async fn ollama_preflight_reports_missing_model() {
        let (handle, base_url) = start_json_server(
            "/api/tags",
            json!({
                "models": [{"name": "llama3:8b"}]
            }),
        )
        .await;
        let resolver =
            CredentialResolver::from_config(&crate::spearlet::config::SpearletConfig::default());
        let spec = BackendSpec {
            name: "backend-2".to_string(),
            kind: "ollama_chat".to_string(),
            operations: vec!["chat_completions".to_string()],
            features: vec![],
            transports: vec!["http".to_string()],
            weight: 100,
            priority: 0,
            base_url,
            provider: "ollama".to_string(),
            model: "qwen2.5".to_string(),
            hosting: crate::proto::sms::BackendHosting::Remote as i32,
            credential_ref: String::new(),
            origin: crate::proto::sms::BackendOrigin::Sms as i32,
            deployment_id: String::new(),
        };

        let report = preflight_remote_backend(
            &Client::new(),
            &resolver,
            &spec,
            &["connectivity".to_string(), "model_access".to_string()],
        )
        .await;

        assert!(!report.success);
        assert_eq!(report.error_code.as_deref(), Some("model_not_found"));
        assert_eq!(report.model_accessible, Some(false));
        handle.abort();
    }
}
