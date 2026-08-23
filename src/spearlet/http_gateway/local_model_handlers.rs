use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

use crate::ai_backend_types::CanonicalBackendProvider;
use crate::spearlet::local_models::llamacpp::preflight_model_source;

use super::AppState;

/// Node-side local model preflight request.
/// 节点侧本地模型预检请求。
#[derive(Debug, Clone, Deserialize)]
pub(super) struct LocalModelPreflightRequest {
    pub(super) provider: String,
    pub(super) model: String,
    pub(super) model_url: Option<String>,
    pub(super) model_path: Option<String>,
    pub(super) skip_download: Option<bool>,
    pub(super) download_timeout_s: Option<u64>,
}

/// Node-side local model preflight response.
/// 节点侧本地模型预检响应。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "source_kind", rename_all = "snake_case")]
pub(super) enum LocalModelPreflightDetailsResponse {
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

/// Node-side local model preflight response envelope.
/// 节点侧本地模型预检响应包裹结构。
#[derive(Debug, Clone, Serialize)]
pub(super) struct LocalModelPreflightResponse {
    pub(super) success: bool,
    pub(super) details: LocalModelPreflightDetailsResponse,
    pub(super) message: String,
}

/// Typed provider family selector for local-model preflight.
/// local-model preflight 的强类型 provider 家族选择器。
#[derive(Debug, Clone, PartialEq, Eq)]
enum LocalModelPreflightProvider {
    /// llama.cpp local-model preflight family.
    /// llama.cpp 本地模型预检家族。
    LlamaCpp,
    /// Unsupported local-model preflight provider.
    /// 不支持的本地模型预检 provider。
    Unsupported(String),
}

impl LocalModelPreflightProvider {
    /// Parse request provider into typed local preflight family.
    /// 将请求 provider 解析为强类型 local preflight 家族。
    fn parse(value: &str) -> Self {
        match CanonicalBackendProvider::parse(value) {
            CanonicalBackendProvider::LlamaCpp => Self::LlamaCpp,
            other => Self::Unsupported(other.as_str().to_string()),
        }
    }
}

/// Typed llama.cpp preflight params builder.
/// 强类型 llama.cpp 预检参数构建器。
#[derive(Debug, Clone, PartialEq, Eq)]
struct LlamaCppPreflightParams {
    model_url: Option<String>,
    model_path: Option<String>,
    skip_download: bool,
    download_timeout_s: Option<u64>,
}

impl LlamaCppPreflightParams {
    /// Build typed llama.cpp preflight params from request body.
    /// 从请求体构建强类型 llama.cpp 预检参数。
    fn from_request(body: &LocalModelPreflightRequest) -> Self {
        Self {
            model_url: body
                .model_url
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string),
            model_path: body
                .model_path
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string),
            skip_download: body.skip_download.unwrap_or(false),
            download_timeout_s: body.download_timeout_s,
        }
    }

    /// Render typed params into the legacy key-value shape.
    /// 将强类型参数渲染为遗留 key-value 形态。
    fn to_metadata_map(&self) -> HashMap<String, String> {
        let mut params = HashMap::new();
        if let Some(model_url) = &self.model_url {
            params.insert("model_url".to_string(), model_url.clone());
        }
        if let Some(model_path) = &self.model_path {
            params.insert("model_path".to_string(), model_path.clone());
        }
        if self.skip_download {
            params.insert("skip_download".to_string(), "1".to_string());
        }
        if let Some(download_timeout_s) = self.download_timeout_s {
            params.insert(
                "download_timeout_s".to_string(),
                download_timeout_s.to_string(),
            );
        }
        params
    }
}

/// Build a failed node-side preflight response without details.
/// 构建不包含 details 的失败节点侧预检响应。
fn local_model_preflight_error(message: String) -> LocalModelPreflightResponse {
    LocalModelPreflightResponse {
        success: false,
        details: LocalModelPreflightDetailsResponse::Unavailable,
        message,
    }
}

/// POST /internal/ai/local-models/preflight
/// 在 SPEARlet 节点本地验证模型来源是否可用。
pub(super) async fn preflight_local_model(
    State(state): State<AppState>,
    Json(body): Json<LocalModelPreflightRequest>,
) -> (StatusCode, Json<LocalModelPreflightResponse>) {
    let provider_family = LocalModelPreflightProvider::parse(&body.provider);
    if let LocalModelPreflightProvider::Unsupported(provider) = provider_family {
        return (
            StatusCode::BAD_REQUEST,
            Json(local_model_preflight_error(format!(
                "only local llamacpp preflight is supported (provider={})",
                provider
            ))),
        );
    }
    if body.model.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(local_model_preflight_error("model is required".to_string())),
        );
    }

    let typed_params = LlamaCppPreflightParams::from_request(&body);
    let params = typed_params.to_metadata_map();

    // Use a short probe timeout here so create-time feedback stays fast while still
    // reusing the same node-side network path as the real download logic.
    // 这里使用较短的探测超时，以保证创建时反馈足够快，同时仍复用真实下载所处的节点网络路径。
    let timeout_s = body.download_timeout_s.unwrap_or(10).clamp(1, 30);
    let http = match reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_s))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(local_model_preflight_error(error.to_string())),
            )
        }
    };

    match preflight_model_source(
        &http,
        &state.config.local_models_dir,
        body.model.trim(),
        &params,
    )
    .await
    {
        Ok(report) => (
            StatusCode::OK,
            Json(LocalModelPreflightResponse {
                success: true,
                details: match report.source_kind.as_str() {
                    "model_path" => LocalModelPreflightDetailsResponse::ModelPath {
                        effective_model_path: report.effective_model_path,
                    },
                    "model_url" => LocalModelPreflightDetailsResponse::ModelUrl {
                        effective_model_path: report.effective_model_path,
                        final_url: report
                            .final_url
                            .unwrap_or_else(|| "unknown".to_string()),
                        http_status: report.http_status.unwrap_or(200),
                        content_length: report.content_length,
                    },
                    _ => LocalModelPreflightDetailsResponse::Unavailable,
                },
                message: report.message,
            }),
        ),
        Err(message) => (
            StatusCode::BAD_REQUEST,
            Json(local_model_preflight_error(message)),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_model_preflight_provider_parses_aliases() {
        assert_eq!(
            LocalModelPreflightProvider::parse("llama.cpp"),
            LocalModelPreflightProvider::LlamaCpp
        );
        assert_eq!(
            LocalModelPreflightProvider::parse("llamacpp"),
            LocalModelPreflightProvider::LlamaCpp
        );
    }

    #[test]
    fn local_model_preflight_provider_rejects_non_llamacpp() {
        assert_eq!(
            LocalModelPreflightProvider::parse("vllm"),
            LocalModelPreflightProvider::Unsupported("vllm".to_string())
        );
    }

    #[test]
    fn llamacpp_preflight_params_builds_trimmed_metadata_map() {
        let body = LocalModelPreflightRequest {
            provider: "llamacpp".to_string(),
            model: "qwen".to_string(),
            model_url: Some(" http://example.com/model.gguf ".to_string()),
            model_path: Some(" /tmp/qwen.gguf ".to_string()),
            skip_download: Some(true),
            download_timeout_s: Some(15),
        };
        let params = LlamaCppPreflightParams::from_request(&body).to_metadata_map();

        assert_eq!(
            params.get("model_url").map(String::as_str),
            Some("http://example.com/model.gguf")
        );
        assert_eq!(
            params.get("model_path").map(String::as_str),
            Some("/tmp/qwen.gguf")
        );
        assert_eq!(params.get("skip_download").map(String::as_str), Some("1"));
        assert_eq!(
            params.get("download_timeout_s").map(String::as_str),
            Some("15")
        );
    }
}
