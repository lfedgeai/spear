use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::time::{interval, timeout};
use tokio_util::sync::CancellationToken;
use tonic::transport::Channel;
use tracing::{debug, warn};

use crate::proto::sms::backend_registry_service_client::BackendRegistryServiceClient;
use crate::proto::sms::{
    BackendInfo, BackendStatus, NodeBackendSnapshot, ReportNodeBackendsRequest,
};
use crate::spearlet::ai::backend_assembly::backend_spec_from_config;
use crate::spearlet::ai::credential_resolver::{CredentialResolution, CredentialResolver};
use crate::spearlet::config::SpearletConfig;
use crate::spearlet::local_models::ManagedBackendRegistry;
use crate::spearlet::ai::dynamic_backend_registry::DynamicBackendSource;
use crate::spearlet::controller::Controller;

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

impl Controller for BackendReporterService {
    fn name(&self) -> &'static str {
        "backend_reporter"
    }

    fn start(&self) {
        BackendReporterService::start(self)
    }

    fn shutdown(&self) {
        BackendReporterService::shutdown(self)
    }
}

fn build_backend_info_list(cfg: &SpearletConfig) -> Vec<BackendInfo> {
    let resolver = CredentialResolver::from_config(cfg);
    let mut out = Vec::new();

    for b in cfg.ai.backends.iter() {
        let mut status = BackendStatus::Available as i32;
        let mut reason = String::new();

        if b.credential_ref
            .as_deref()
            .map(|s| s.trim())
            .is_some_and(|s| !s.is_empty())
        {
            let credential_ref = b.credential_ref.as_deref().unwrap_or_default().trim();
            let credential_state = resolver.resolve_api_key_state(Some(credential_ref));
            match credential_state {
                CredentialResolution::Ready(_) => {}
                CredentialResolution::Disabled
                | CredentialResolution::NotSynced
                | CredentialResolution::Missing => {
                    status = BackendStatus::Unavailable as i32;
                    reason = credential_state.message(credential_ref);
                }
            }
        }

        let spec = backend_spec_from_config(b);
        out.push(BackendInfo {
            spec: Some(spec),
            status,
            status_reason: reason,
        });
    }

    out
}

fn collect_reportable_backends(
    cfg: &SpearletConfig,
    managed_backends: Option<&ManagedBackendRegistry>,
) -> Vec<BackendInfo> {
    let static_backends = build_backend_info_list(cfg);
    let mut by_name: HashMap<String, BackendInfo> = HashMap::new();
    for b in static_backends.into_iter() {
        let Some(spec) = b.spec.as_ref() else {
            continue;
        };
        let name = spec.name.trim();
        if name.is_empty() {
            continue;
        }
        by_name.insert(name.to_string(), b);
    }

    if let Some(m) = managed_backends {
        // Report dynamic runtime backend facts back to SMS, including SMS-synced remote
        // backends, so the Web Admin catalog can reflect what nodes can actually serve.
        // 将动态运行时 backend 状态回报给 SMS（包含从 SMS 同步的 remote backends），
        // 以便 Web Admin 目录能反映节点真实可用性。
        for b in m
            .list_merged_sorted(&[DynamicBackendSource::LocalController, DynamicBackendSource::Sms])
            .into_iter()
        {
            let Some(spec) = b.spec.as_ref() else {
                continue;
            };
            let name = spec.name.trim();
            if name.is_empty() {
                continue;
            }
            by_name.insert(name.to_string(), b);
        }
    }

    by_name.into_values().collect::<Vec<_>>()
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
        let backends = collect_reportable_backends(&config, managed_backends.as_ref());
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::sms::{BackendHosting, BackendOrigin, BackendSpec, CredentialMaterial};
    use crate::spearlet::ai::dynamic_backend_registry::DynamicBackendRegistry;
    use crate::spearlet::config::AiBackendConfig;

    fn mk_backend(name: &str, kind: &str, origin: BackendOrigin) -> BackendInfo {
        BackendInfo {
            spec: Some(BackendSpec {
                name: name.to_string(),
                kind: kind.to_string(),
                operations: vec!["chat_completions".to_string()],
                features: vec![],
                transports: vec!["http".to_string()],
                weight: 100,
                priority: 0,
                base_url: "https://example.com/v1".to_string(),
                provider: String::new(),
                model: String::new(),
                hosting: BackendHosting::Remote as i32,
                credential_ref: String::new(),
                origin: origin as i32,
                deployment_id: String::new(),
            }),
            status: BackendStatus::Available as i32,
            status_reason: String::new(),
        }
    }

    #[test]
    fn collect_reportable_backends_includes_sms_dynamic_backends() {
        let mut cfg = SpearletConfig::default();
        cfg.ai.backends.push(AiBackendConfig {
            name: "static-openai".to_string(),
            kind: "openai_chat_completion".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            hosting: Some("remote".to_string()),
            model: None,
            credential_ref: None,
            provider: None,
            origin: None,
            deployment_id: None,
            weight: 100,
            priority: 0,
            ops: vec!["chat_completions".to_string()],
            features: vec![],
            transports: vec!["http".to_string()],
        });

        let managed = DynamicBackendRegistry::new();
        managed.set_backends(
            DynamicBackendSource::LocalController,
            vec![mk_backend(
                "local-managed",
                "ollama_chat",
                BackendOrigin::LocalController,
            )],
        );
        managed.set_backends(
            DynamicBackendSource::Sms,
            vec![mk_backend("sms-remote", "openai_chat_completion", BackendOrigin::Sms)],
        );

        let reported = collect_reportable_backends(&cfg, Some(&managed));
        let mut names = reported
            .into_iter()
            .filter_map(|b| b.spec.map(|s| s.name))
            .collect::<Vec<_>>();
        names.sort();

        assert_eq!(
            names,
            vec![
                "local-managed".to_string(),
                "sms-remote".to_string(),
                "static-openai".to_string()
            ]
        );
    }

    #[test]
    fn build_backend_info_list_reports_disabled_dynamic_credential_as_unavailable() {
        let _guard = crate::spearlet::ai::dynamic_credential_store::global_dynamic_credentials_test_lock()
            .lock()
            .expect("lock");
        let store = crate::spearlet::ai::dynamic_credential_store::global_dynamic_credentials();
        store.clear();
        store.set_credentials(vec![CredentialMaterial {
            name: "openai-default".to_string(),
            provider_kind: "inline_encrypted".to_string(),
            secret: "sk-test".to_string(),
            version: 1,
            disabled: true,
            updated_at_ms: 0,
        }]);

        let mut cfg = SpearletConfig::default();
        cfg.ai.backends.push(AiBackendConfig {
            name: "openai-chat".to_string(),
            kind: "openai_chat_completion".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            hosting: Some("remote".to_string()),
            model: None,
            credential_ref: Some("openai-default".to_string()),
            provider: None,
            origin: None,
            deployment_id: None,
            weight: 100,
            priority: 0,
            ops: vec!["chat_completions".to_string()],
            features: vec![],
            transports: vec!["http".to_string()],
        });

        let infos = build_backend_info_list(&cfg);
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].status, BackendStatus::Unavailable as i32);
        assert!(infos[0]
            .status_reason
            .contains("credential_ref 'openai-default' is disabled"));
        store.clear();
    }
}
