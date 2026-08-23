use std::collections::HashSet;
use std::time::Duration;

use anyhow::{anyhow, Result};
use reqwest::Url;
use serde::Deserialize;

use crate::spearlet::config::{AiBackendConfig, SpearletConfig};
use crate::spearlet::execution::ai::backends::KIND_OLLAMA_CHAT;

#[derive(Debug, Deserialize)]
struct OllamaPsResponse {
    models: Option<Vec<OllamaPsModel>>,
}

#[derive(Debug, Deserialize)]
struct OllamaPsModel {
    name: String,
}

pub async fn maybe_import_ollama_serving_models(cfg: &mut SpearletConfig) -> Result<usize> {
    if !cfg.ai.discovery.ollama.enabled {
        return Ok(0);
    }

    let discovery = cfg.ai.discovery.ollama.clone();
    let base_url = Url::parse(&discovery.base_url)
        .map_err(|e| anyhow!("invalid ollama base_url: {}: {}", discovery.base_url, e))?;

    if !discovery_allows_base_url(&base_url, discovery.allow_remote) {
        tracing::warn!(base_url = %discovery.base_url, "ollama discovery base_url is not allowed");
        return Ok(0);
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(discovery.timeout_ms))
        .build()?;

    let mut model_names = fetch_model_names(&client, &base_url, discovery.scope.as_str()).await?;

    model_names.sort();
    model_names.dedup();

    model_names = apply_allow_deny(model_names, &discovery.allow_models, &discovery.deny_models);
    if model_names.len() > discovery.max_models {
        model_names.truncate(discovery.max_models);
    }

    if model_names.is_empty() {
        return Ok(0);
    }

    let existing_names: HashSet<String> = cfg.ai.backends.iter().map(|b| b.name.clone()).collect();
    let mut imported = 0usize;

    for model in model_names {
        let derived = format!(
            "{}{}",
            discovery.name_prefix,
            sanitize_backend_suffix(&model)
        );
        if existing_names.contains(&derived) {
            if discovery.name_conflict.as_str() == "overwrite" {
                cfg.ai.backends.retain(|b| b.name != derived);
            } else {
                continue;
            }
        }

        cfg.ai.backends.push(AiBackendConfig {
            name: derived,
            kind: KIND_OLLAMA_CHAT.to_string(),
            base_url: discovery.base_url.clone(),
            hosting: Some("local".to_string()),
            model: Some(model),
            credential_ref: None,
            provider: Some("ollama".to_string()),
            origin: Some("local_controller".to_string()),
            deployment_id: None,
            weight: discovery.default_weight,
            priority: discovery.default_priority,
            ops: discovery.default_ops.clone(),
            features: discovery.default_features.clone(),
            transports: discovery.default_transports.clone(),
        });

        imported += 1;
    }

    Ok(imported)
}

async fn fetch_model_names(
    client: &reqwest::Client,
    base_url: &Url,
    scope: &str,
) -> Result<Vec<String>> {
    let endpoint = match scope {
        "serving" => "api/ps",
        "installed" => "api/tags",
        other => return Err(anyhow!("unsupported ollama scope: {other}")),
    };

    let url = base_url.join(endpoint)?;
    let resp = client.get(url).send().await?;
    let status = resp.status();
    let body = resp.bytes().await?;
    if !status.is_success() {
        let body_str = String::from_utf8_lossy(&body);
        return Err(anyhow!(
            "ollama /{} failed: status={} body={}",
            endpoint,
            status,
            body_str
        ));
    }

    let parsed: OllamaPsResponse = serde_json::from_slice(&body)?;
    let mut model_names: Vec<String> = parsed
        .models
        .unwrap_or_default()
        .into_iter()
        .map(|m| m.name)
        .filter(|s| !s.trim().is_empty())
        .collect();
    model_names.sort();
    model_names.dedup();
    Ok(model_names)
}

fn apply_allow_deny(mut models: Vec<String>, allow: &[String], deny: &[String]) -> Vec<String> {
    if !allow.is_empty() {
        let allow_set: HashSet<&str> = allow.iter().map(|s| s.as_str()).collect();
        models.retain(|m| allow_set.contains(m.as_str()));
    }
    if !deny.is_empty() {
        let deny_set: HashSet<&str> = deny.iter().map(|s| s.as_str()).collect();
        models.retain(|m| !deny_set.contains(m.as_str()));
    }
    models
}

fn sanitize_backend_suffix(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        let ok = ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '/');
        if ok {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    while out.contains("//") {
        out = out.replace("//", "/");
    }
    out.trim_matches('/').to_string()
}

fn is_loopback_host(host: &str) -> bool {
    let h = host.trim().to_ascii_lowercase();
    h == "localhost" || h == "127.0.0.1" || h == "::1"
}

fn discovery_allows_base_url(url: &Url, allow_remote: bool) -> bool {
    if allow_remote {
        return true;
    }
    match url.host_str() {
        Some(h) => is_loopback_host(h),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::get, Json, Router};
    use serde_json::json;
    use tokio::net::TcpListener;

    async fn start_ollama_mock(ps_body: serde_json::Value, tags_body: serde_json::Value) -> String {
        let app = Router::new()
            .route(
                "/api/ps",
                get(move || {
                    let b = ps_body.clone();
                    async move { Json(b) }
                }),
            )
            .route(
                "/api/tags",
                get(move || {
                    let b = tags_body.clone();
                    async move { Json(b) }
                }),
            );

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        format!("http://{}", addr)
    }

    #[tokio::test]
    async fn test_import_ollama_models_appends_backends() {
        let base_url = start_ollama_mock(
            json!({
                "models": [
                    {"name": "llama3:latest"},
                    {"name": "qwen2.5:7b"}
                ]
            }),
            json!({"models": []}),
        )
        .await;

        let mut cfg = SpearletConfig::default();
        cfg.ai.discovery.ollama.enabled = true;
        cfg.ai.discovery.ollama.base_url = base_url;

        let n = maybe_import_ollama_serving_models(&mut cfg).await.unwrap();
        assert_eq!(n, 2);
        assert!(cfg.ai.backends.iter().any(|b| b.kind == KIND_OLLAMA_CHAT));
        assert!(cfg
            .ai
            .backends
            .iter()
            .any(|b| b.model.as_deref() == Some("llama3:latest")));
    }

    #[tokio::test]
    async fn test_import_ollama_installed_models_uses_tags() {
        let base_url = start_ollama_mock(
            json!({"models": []}),
            json!({
                "models": [
                    {"name": "llama3:latest"},
                    {"name": "qwen2.5:7b"}
                ]
            }),
        )
        .await;

        let mut cfg = SpearletConfig::default();
        cfg.ai.discovery.ollama.enabled = true;
        cfg.ai.discovery.ollama.scope = "installed".to_string();
        cfg.ai.discovery.ollama.base_url = base_url;

        let n = maybe_import_ollama_serving_models(&mut cfg).await.unwrap();
        assert_eq!(n, 2);
    }
}
