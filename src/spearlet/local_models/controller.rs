use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use reqwest::Client;
use tokio::time::MissedTickBehavior;
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;
use tonic::transport::Channel;
use tonic::{Code, Request};
use tracing::{debug, warn};

use crate::proto::sms::model_deployment_registry_service_client::ModelDeploymentRegistryServiceClient;
use crate::proto::sms::{
    BackendInfo, ListModelDeploymentsRequest, ModelDeploymentPhase, ModelDeploymentStatus,
    ReportModelDeploymentStatusRequest,
};
use crate::spearlet::config::SpearletConfig;
use crate::spearlet::ai::dynamic_backend_registry::DynamicBackendSource;
use crate::spearlet::controller::Controller;

use super::llamacpp::LlamaCppSupervisor;
use super::managed_backends::ManagedBackendRegistry;

#[derive(Clone)]
pub struct LocalModelController {
    config: Arc<SpearletConfig>,
    sms_channel: Option<Channel>,
    managed_backends: ManagedBackendRegistry,
    llamacpp: LlamaCppSupervisor,
    cancel: CancellationToken,
}

impl LocalModelController {
    pub fn new(
        config: Arc<SpearletConfig>,
        sms_channel: Option<Channel>,
        managed_backends: ManagedBackendRegistry,
    ) -> Self {
        let llamacpp = LlamaCppSupervisor::new(config.as_ref());
        Self {
            config,
            sms_channel,
            managed_backends,
            llamacpp,
            cancel: CancellationToken::new(),
        }
    }

    pub fn shutdown(&self) {
        self.cancel.cancel();
        self.managed_backends.clear(DynamicBackendSource::LocalController);
    }

    pub fn start(&self) {
        let Some(channel) = self.sms_channel.clone() else {
            return;
        };
        let this = self.clone();
        tokio::spawn(async move {
            this.run_loop(channel).await;
        });
    }

    async fn run_loop(&self, channel: Channel) {
        let node_uuid = self.config.compute_node_uuid();
        let mut client = ModelDeploymentRegistryServiceClient::new(channel.clone());
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_else(|_| Client::new());

        let mut seen_spec: HashMap<String, String> = HashMap::new();
        let mut watch_cursor_revision: u64 = 0;
        let mut backoff_ms: u64 = 200;

        loop {
            if self.cancel.is_cancelled() {
                self.llamacpp.stop_all().await;
                return;
            }

            let (snapshot_revision, records) = match self
                .list_all_model_deployments_for_node(&mut client, &node_uuid)
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    warn!(error = %e, "ListModelDeployments failed");
                    tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                    backoff_ms = (backoff_ms * 2).min(10_000);
                    continue;
                }
            };
            watch_cursor_revision = watch_cursor_revision.max(snapshot_revision);
            backoff_ms = 200;
            self.reconcile_from_records(&http, &mut client, &node_uuid, &records, &mut seen_spec)
                .await;

            let watch_resp = client
                .watch_model_deployments(Request::new(
                    crate::proto::sms::WatchModelDeploymentsRequest {
                        since_revision: watch_cursor_revision,
                        target_node_uuid: node_uuid.clone(),
                    },
                ))
                .await;

            let mut stream = match watch_resp {
                Ok(r) => {
                    backoff_ms = 200;
                    r.into_inner()
                }
                Err(e) => {
                    warn!(error = %e, watch_cursor_revision, "WatchModelDeployments failed");
                    tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                    backoff_ms = (backoff_ms * 2).min(10_000);
                    continue;
                }
            };

            loop {
                tokio::select! {
                    _ = self.cancel.cancelled() => {
                        self.llamacpp.stop_all().await;
                        return;
                    }
                    item = stream.next() => {
                        let Some(item) = item else {
                            break;
                        };
                        match item {
                            Ok(msg) => {
                                if let Some(ev) = msg.event {
                                    let event_revision = ev.revision;
                                    watch_cursor_revision = watch_cursor_revision.max(event_revision);
                                }
                                let (snapshot_revision, records) = match self.list_all_model_deployments_for_node(&mut client, &node_uuid).await {
                                    Ok(r) => r,
                                    Err(e) => {
                                        warn!(error = %e, "ListModelDeployments failed after watch event");
                                        break;
                                    }
                                };
                                watch_cursor_revision = watch_cursor_revision.max(snapshot_revision);
                                self.reconcile_from_records(&http, &mut client, &node_uuid, &records, &mut seen_spec).await;
                            }
                            Err(e) => {
                                if matches!(e.code(), Code::FailedPrecondition | Code::Aborted) {
                                    debug!(code = ?e.code(), "WatchModelDeployments requires resync");
                                } else {
                                    warn!(error = %e, code = ?e.code(), "WatchModelDeployments stream error");
                                }
                                break;
                            }
                        }
                    }
                }
            }

            tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
            backoff_ms = (backoff_ms * 2).min(10_000);
        }
    }

    async fn list_all_model_deployments_for_node(
        &self,
        client: &mut ModelDeploymentRegistryServiceClient<Channel>,
        node_uuid: &str,
    ) -> Result<(u64, Vec<crate::proto::sms::ModelDeploymentRecord>), tonic::Status> {
        let mut all: Vec<crate::proto::sms::ModelDeploymentRecord> = Vec::new();
        let mut offset: u32 = 0;
        let limit: u32 = 500;
        let mut snapshot_revision: u64 = 0;

        loop {
            let resp = client
                .list_model_deployments(ListModelDeploymentsRequest {
                    limit,
                    offset,
                    target_node_uuid: node_uuid.to_string(),
                    provider: String::new(),
                })
                .await?
                .into_inner();
            snapshot_revision = snapshot_revision.max(resp.revision);
            all.extend(resp.records.into_iter());
            if all.len() >= resp.total_count as usize {
                break;
            }
            offset = offset.saturating_add(limit);
        }

        Ok((snapshot_revision, all))
    }

    async fn reconcile_from_records(
        &self,
        http: &Client,
        client: &mut ModelDeploymentRegistryServiceClient<Channel>,
        node_uuid: &str,
        records: &[crate::proto::sms::ModelDeploymentRecord],
        seen_spec: &mut HashMap<String, String>,
    ) {
        let mut managed_backend_infos: Vec<BackendInfo> = Vec::new();
        let mut live_ids: HashSet<String> = HashSet::new();

        for rec in records.iter() {
            let Some(spec) = rec.spec.as_ref() else {
                continue;
            };
            if spec.target_node_uuid != node_uuid {
                continue;
            }
            live_ids.insert(rec.deployment_id.clone());

            let spec_key = spec_fingerprint(spec);
            let phase = rec
                .status
                .as_ref()
                .map(|s| s.phase)
                .unwrap_or(ModelDeploymentPhase::Pending as i32);
            let is_ready = phase == ModelDeploymentPhase::Ready as i32;

            let prev_key = seen_spec.get(&rec.deployment_id).cloned();
            if prev_key.as_deref() != Some(spec_key.as_str()) {
                if is_ready {
                    if let Some(b) = self
                        .build_backend_info_for_ready(
                            &rec.deployment_id,
                            &spec.provider,
                            &spec.model,
                            &spec.params,
                        )
                        .await
                    {
                        managed_backend_infos.push(b);
                    }
                } else {
                    let res = self
                        .reconcile_one(
                            http,
                            client,
                            node_uuid,
                            rec.deployment_id.clone(),
                            rec.revision,
                            spec.clone(),
                        )
                        .await;
                    if let Ok(Some(b)) = res {
                        managed_backend_infos.push(b);
                    }
                }
                seen_spec.insert(rec.deployment_id.clone(), spec_key);
                continue;
            }

            if is_ready {
                if let Some(b) = self
                    .build_backend_info_for_ready(
                        &rec.deployment_id,
                        &spec.provider,
                        &spec.model,
                        &spec.params,
                    )
                    .await
                {
                    managed_backend_infos.push(b);
                }
            }
        }

        self.managed_backends.set_backends(
            DynamicBackendSource::LocalController,
            managed_backend_infos,
        );
        seen_spec.retain(|id, _| live_ids.contains(id));
        self.llamacpp.stop_removed(&live_ids).await;
    }

    async fn reconcile_one(
        &self,
        http: &Client,
        client: &mut ModelDeploymentRegistryServiceClient<Channel>,
        node_uuid: &str,
        deployment_id: String,
        observed_record_revision: u64,
        spec: crate::proto::sms::ModelDeploymentSpec,
    ) -> Result<Option<BackendInfo>, ()> {
        let provider = spec.provider.to_ascii_lowercase();
        if provider == "vllm" {
            return self
                .reconcile_vllm_placeholder(
                    client,
                    node_uuid,
                    &deployment_id,
                    observed_record_revision,
                )
                .await;
        }
        if provider == "llamacpp" || provider == "llama_cpp" || provider == "llama.cpp" {
            return self
                .reconcile_llamacpp(
                    http,
                    client,
                    node_uuid,
                    &deployment_id,
                    observed_record_revision,
                    &spec,
                )
                .await;
        }
        let _ = self
            .report_status(
                client,
                node_uuid,
                &deployment_id,
                observed_record_revision,
                ModelDeploymentPhase::Failed,
                format!("unsupported provider: {}", spec.provider),
            )
            .await;
        Ok(None)
    }

    async fn reconcile_llamacpp(
        &self,
        http: &Client,
        client: &mut ModelDeploymentRegistryServiceClient<Channel>,
        node_uuid: &str,
        deployment_id: &str,
        observed_record_revision: u64,
        spec: &crate::proto::sms::ModelDeploymentSpec,
    ) -> Result<Option<BackendInfo>, ()> {
        let model = spec.model.trim();
        if model.is_empty() {
            let _ = self
                .report_status(
                    client,
                    node_uuid,
                    deployment_id,
                    observed_record_revision,
                    ModelDeploymentPhase::Failed,
                    "model is required".to_string(),
                )
                .await;
            return Ok(None);
        }

        let spec_key = spec_fingerprint(spec);
        let should_pull = !spec
            .params
            .get("server_mode")
            .map(|s| s.trim().eq_ignore_ascii_case("raw"))
            .unwrap_or(false)
            && !spec
                .params
                .get("skip_download")
                .map(|s| s.trim() == "1")
                .unwrap_or(false);

        let phase = if should_pull {
            ModelDeploymentPhase::Pulling
        } else {
            ModelDeploymentPhase::Starting
        };

        let _ = self
            .report_status(
                client,
                node_uuid,
                deployment_id,
                observed_record_revision,
                phase,
                String::new(),
            )
            .await;

        let start = tokio::time::Instant::now();
        let mut ticker = tokio::time::interval(Duration::from_secs(10));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut fut = std::pin::pin!(self.llamacpp.ensure_server(
            http,
            deployment_id,
            &spec_key,
            model,
            &spec.params
        ));

        let backend = loop {
            tokio::select! {
                res = &mut fut => {
                    match res {
                        Ok(b) => break b,
                        Err(e) => {
                            let _ = self
                                .report_status(
                                    client,
                                    node_uuid,
                                    deployment_id,
                                    observed_record_revision,
                                    ModelDeploymentPhase::Failed,
                                    e,
                                )
                                .await;
                            return Ok(None);
                        }
                    }
                }
                _ = ticker.tick() => {
                    let elapsed_s = start.elapsed().as_secs();
                    let msg = if should_pull {
                        format!("pulling ({}s)", elapsed_s)
                    } else {
                        format!("starting ({}s)", elapsed_s)
                    };
                    let _ = self
                        .report_status(
                            client,
                            node_uuid,
                            deployment_id,
                            observed_record_revision,
                            phase,
                            msg,
                        )
                        .await;
                }
            }
        };

        let _ = self
            .report_status(
                client,
                node_uuid,
                deployment_id,
                observed_record_revision,
                ModelDeploymentPhase::Ready,
                String::new(),
            )
            .await;

        Ok(Some(backend))
    }

    async fn reconcile_vllm_placeholder(
        &self,
        client: &mut ModelDeploymentRegistryServiceClient<Channel>,
        node_uuid: &str,
        deployment_id: &str,
        observed_record_revision: u64,
    ) -> Result<Option<BackendInfo>, ()> {
        let _ = self
            .report_status(
                client,
                node_uuid,
                deployment_id,
                observed_record_revision,
                ModelDeploymentPhase::Pending,
                "placeholder: vLLM local backend create/start is not implemented yet".to_string(),
            )
            .await;
        Ok(None)
    }

    async fn report_status(
        &self,
        client: &mut ModelDeploymentRegistryServiceClient<Channel>,
        node_uuid: &str,
        deployment_id: &str,
        observed_record_revision: u64,
        phase: ModelDeploymentPhase,
        message: String,
    ) -> Result<(), tonic::Status> {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let status = ModelDeploymentStatus {
            phase: phase as i32,
            message,
            updated_at_ms: now_ms,
        };
        let resp = client
            .report_model_deployment_status(ReportModelDeploymentStatusRequest {
                deployment_id: deployment_id.to_string(),
                node_uuid: node_uuid.to_string(),
                observed_revision: observed_record_revision,
                status: Some(status),
            })
            .await?;
        if !resp.into_inner().success {
            return Err(tonic::Status::failed_precondition(
                "stale observed_record_revision",
            ));
        }
        Ok(())
    }

    async fn build_backend_info_for_ready(
        &self,
        deployment_id: &str,
        provider: &str,
        _model: &str,
        _params: &std::collections::HashMap<String, String>,
    ) -> Option<BackendInfo> {
        if provider.eq_ignore_ascii_case("llamacpp")
            || provider.eq_ignore_ascii_case("llama_cpp")
            || provider.eq_ignore_ascii_case("llama.cpp")
        {
            return self.llamacpp.get_backend(deployment_id).await;
        }
        None
    }
}

impl Controller for LocalModelController {
    fn name(&self) -> &'static str {
        "local_model_controller"
    }

    fn start(&self) {
        LocalModelController::start(self)
    }

    fn shutdown(&self) {
        LocalModelController::shutdown(self)
    }
}

fn spec_fingerprint(spec: &crate::proto::sms::ModelDeploymentSpec) -> String {
    let mut parts: Vec<(String, String)> = spec
        .params
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    parts.sort_by(|a, b| a.0.cmp(&b.0));
    let params = parts
        .into_iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");
    format!(
        "provider={};model={};params={}",
        spec.provider, spec.model, params
    )
}
