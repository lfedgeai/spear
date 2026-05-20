use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::time::{interval, timeout};
use tokio_util::sync::CancellationToken;
use tonic::transport::Channel;
use tracing::{debug, warn};

use crate::proto::sms::backend_registry_service_client::BackendRegistryServiceClient;
use crate::proto::sms::{
    BackendHosting, BackendInfo, BackendStatus, NodeBackendSnapshot, ReportNodeBackendsRequest,
};
use crate::spearlet::config::{LlmBackendConfig, SpearletConfig};
use crate::spearlet::execution::ai::backends::{
    KIND_OLLAMA_CHAT, KIND_OPENAI_CHAT_COMPLETION, KIND_OPENAI_REALTIME_WS, KIND_STUB,
};
use crate::spearlet::local_models::ManagedBackendRegistry;

#[derive(Debug)]
pub struct BackendReporterService {
    config: Arc<SpearletConfig>,
    sms_channel: Option<Channel>,
    managed_backends: Option<ManagedBackendRegistry>,
    cancel: CancellationToken,
}

impl BackendReporterService {
    pub fn new(
        config: Arc<SpearletConfig>,
        sms_channel: Option<Channel>,
        managed_backends: Option<ManagedBackendRegistry>,
    ) -> Self {
        Self {
            config,
            sms_channel,
            managed_backends,
            cancel: CancellationToken::new(),
        }
    }

    pub fn shutdown(&self) {
        self.cancel.cancel();
    }

    pub fn start(&self) {
        let config = self.config.clone();
        let sms_channel = self.sms_channel.clone();
        let managed_backends = self.managed_backends.clone();
        let cancel = self.cancel.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                report_loop(config, sms_channel, managed_backends, cancel).await;
            });
            return;
        }

        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            if let Ok(rt) = rt {
                rt.block_on(async move {
                    report_loop(config, sms_channel, managed_backends, cancel).await;
                });
            }
        });
    }
}

fn infer_provider(kind: &str) -> String {
    match kind {
        KIND_OPENAI_CHAT_COMPLETION | KIND_OPENAI_REALTIME_WS => "openai".to_string(),
        KIND_OLLAMA_CHAT => "ollama".to_string(),
        KIND_STUB => "internal".to_string(),
        _ => "unknown".to_string(),
    }
}

fn parse_hosting(s: &str) -> Option<i32> {
    let v = s.trim().to_ascii_lowercase();
    match v.as_str() {
        "local" => Some(BackendHosting::NodeLocal as i32),
        "remote" => Some(BackendHosting::Remote as i32),
        "unknown" | "unspecified" => Some(BackendHosting::Unspecified as i32),
        _ => None,
    }
}

fn resolve_hosting(backend: &LlmBackendConfig) -> i32 {
    backend
        .hosting
        .as_deref()
        .and_then(parse_hosting)
        .unwrap_or(BackendHosting::Unspecified as i32)
}

fn credential_env_map(cfg: &SpearletConfig) -> HashMap<String, String> {
    cfg.llm
        .credentials
        .iter()
        .filter(|c| c.kind.as_str() == "env")
        .filter(|c| !c.name.trim().is_empty())
        .filter(|c| !c.api_key_env.trim().is_empty())
        .map(|c| (c.name.clone(), c.api_key_env.clone()))
        .collect()
}

fn resolve_backend_env(
    backend: &LlmBackendConfig,
    creds: &HashMap<String, String>,
) -> Result<String, String> {
    let r = backend
        .credential_ref
        .as_ref()
        .ok_or_else(|| "missing credential_ref".to_string())?;
    if r.trim().is_empty() {
        return Err("credential_ref is empty".to_string());
    }
    creds
        .get(r)
        .cloned()
        .ok_or_else(|| format!("credential_ref not found: {r}"))
}

fn build_backend_info_list(cfg: &SpearletConfig) -> Vec<BackendInfo> {
    let creds = credential_env_map(cfg);
    let mut out = Vec::new();

    for b in cfg.llm.backends.iter() {
        let mut status = BackendStatus::Available as i32;
        let mut reason = String::new();

        if b.credential_ref
            .as_deref()
            .map(|s| s.trim())
            .is_some_and(|s| !s.is_empty())
        {
            let env = resolve_backend_env(b, &creds);
            match env {
                Ok(env_name) => match std::env::var(&env_name) {
                    Ok(v) if !v.trim().is_empty() => {}
                    _ => {
                        status = BackendStatus::Unavailable as i32;
                        reason = format!("missing env {env_name}");
                    }
                },
                Err(e) => {
                    status = BackendStatus::Unavailable as i32;
                    reason = e;
                }
            }
        }

        out.push(BackendInfo {
            name: b.name.clone(),
            kind: b.kind.clone(),
            operations: b.ops.clone(),
            features: b.features.clone(),
            transports: b.transports.clone(),
            weight: b.weight as u32,
            priority: b.priority,
            base_url: b.base_url.clone(),
            status,
            status_reason: reason,
            provider: infer_provider(&b.kind),
            model: b.model.clone().unwrap_or_default(),
            hosting: resolve_hosting(b),
            credential_ref: b.credential_ref.clone().unwrap_or_default(),
        });
    }

    out
}

async fn report_loop(
    config: Arc<SpearletConfig>,
    sms_channel: Option<Channel>,
    managed_backends: Option<ManagedBackendRegistry>,
    cancel: CancellationToken,
) {
    let mut backoff_ms = config.sms_connect_retry_ms.max(200);
    let mut ticker = interval(Duration::from_secs(30));
    let node_uuid = config.compute_node_uuid();
    let mut revision: u64 = 0;

    let Some(channel) = sms_channel else {
        return;
    };

    loop {
        if cancel.is_cancelled() {
            return;
        }
        ticker.tick().await;

        let mut client = BackendRegistryServiceClient::new(channel.clone());

        revision = revision.saturating_add(1);
        let mut backends = build_backend_info_list(&config);
        if let Some(m) = managed_backends.as_ref() {
            backends.extend(m.list());
        }
        let snapshot = NodeBackendSnapshot {
            node_uuid: node_uuid.clone(),
            revision,
            reported_at_ms: 0,
            backends,
        };
        let req = ReportNodeBackendsRequest {
            snapshot: Some(snapshot),
        };
        let per_attempt = Duration::from_millis(config.sms_connect_timeout_ms)
            .min(Duration::from_secs(5))
            .max(Duration::from_millis(1));
        match timeout(per_attempt, client.report_node_backends(req)).await {
            Ok(Ok(resp)) => {
                let inner = resp.into_inner();
                debug!(
                    node_uuid = %node_uuid,
                    accepted_revision = inner.accepted_revision,
                    "backend snapshot reported"
                );
                backoff_ms = config.sms_connect_retry_ms.max(200);
            }
            Ok(Err(e)) => {
                warn!(error = %e, "backend snapshot report failed");
                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                backoff_ms = (backoff_ms * 2).min(10_000);
            }
            Err(_) => {
                warn!("backend snapshot report timeout");
                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                backoff_ms = (backoff_ms * 2).min(10_000);
            }
        }
    }
}
