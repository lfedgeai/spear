use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::Weak;
use std::time::Duration;

use tokio::sync::oneshot;
use tokio_stream::StreamExt;
use tokio::time::interval;
use tokio_util::sync::CancellationToken;
use tonic::transport::Channel;

use crate::proto::sms::admin_ai_config_service_client::AdminAiConfigServiceClient;
use crate::proto::sms::{
    BackendInfo, BackendOrigin, BackendStatus, ListRemoteBackendsRequest, WatchRemoteBackendsRequest,
};
use crate::spearlet::ai::dynamic_backend_registry::{
    global_dynamic_backends, DynamicBackendSource,
};
use crate::spearlet::ai::RemoteBackendMergePolicy;
use crate::spearlet::controller::Controller;
use crate::spearlet::SpearletConfig;

pub struct RemoteBackendSyncService {
    /// Local Spearlet config snapshot at startup. This is treated as the "base layer".
    base_config: Arc<SpearletConfig>,
    sms_channel: Channel,
    poll_interval: Duration,
    merge_policy: RemoteBackendMergePolicy,
    cancel: CancellationToken,
    applied_revision: AtomicU64,
    started: AtomicBool,
    self_weak: Weak<RemoteBackendSyncService>,
}

impl RemoteBackendSyncService {
    pub fn new(
        base_config: Arc<SpearletConfig>,
        sms_channel: Channel,
        poll_interval: Duration,
        merge_policy: RemoteBackendMergePolicy,
    ) -> Arc<Self> {
        Arc::new_cyclic(|weak| Self {
            base_config,
            sms_channel,
            poll_interval: poll_interval.max(Duration::from_secs(1)),
            merge_policy,
            cancel: CancellationToken::new(),
            applied_revision: AtomicU64::new(0),
            started: AtomicBool::new(false),
            self_weak: weak.clone(),
        })
    }

    pub fn start(&self) {
        if self.started.swap(true, Ordering::AcqRel) {
            return;
        }
        if let Some(svc) = self.self_weak.upgrade() {
            svc.spawn();
        }
    }

    pub fn shutdown(&self) {
        self.cancel.cancel();
        global_dynamic_backends().clear(DynamicBackendSource::Sms);
    }

    pub fn applied_revision(&self) -> u64 {
        self.applied_revision.load(Ordering::Acquire)
    }

    fn spawn(self: &Arc<Self>) {
        let svc = self.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move { svc.run_loop().await });
        } else {
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("build remote backend sync runtime");
                let (tx, rx) = oneshot::channel::<()>();
                let _ = tx;
                rt.block_on(async move {
                    let _ = rx;
                    svc.run_loop().await;
                });
            });
        }
    }

    async fn run_loop(self: Arc<Self>) {
        let mut backoff_ms: u64 = 200;
        let mut cursor_revision: u64 = 0;
        loop {
            if self.cancel.is_cancelled() {
                return;
            }

            let mut client = AdminAiConfigServiceClient::new(self.sms_channel.clone());
            let resp = match client.list_remote_backends(ListRemoteBackendsRequest {}).await {
                Ok(r) => {
                    backoff_ms = 200;
                    r.into_inner()
                }
                Err(e) => {
                    tracing::warn!(error = %e, "List remote backends from SMS failed");
                    tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                    backoff_ms = (backoff_ms * 2).min(10_000);
                    continue;
                }
            };

            cursor_revision = cursor_revision.max(resp.revision);
            if let Some(applied) = self.apply_snapshot(resp.revision, resp.backends) {
                cursor_revision = cursor_revision.max(applied);
            }

            let watch_resp = client
                .watch_remote_backends(WatchRemoteBackendsRequest {
                    since_revision: cursor_revision,
                })
                .await;
            let mut stream = match watch_resp {
                Ok(r) => {
                    backoff_ms = 200;
                    r.into_inner()
                }
                Err(e) => {
                    tracing::warn!(error = %e, cursor_revision, "WatchRemoteBackends failed");
                    tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                    backoff_ms = (backoff_ms * 2).min(10_000);
                    continue;
                }
            };

            let mut periodic = interval(self.poll_interval);
            loop {
                tokio::select! {
                    _ = self.cancel.cancelled() => {
                        return;
                    }
                    _ = periodic.tick() => {
                        let resp = match client.list_remote_backends(ListRemoteBackendsRequest {}).await {
                            Ok(r) => r.into_inner(),
                            Err(e) => {
                                tracing::warn!(error = %e, "List remote backends from SMS failed in periodic refresh");
                                break;
                            }
                        };
                        cursor_revision = cursor_revision.max(resp.revision);
                        self.apply_snapshot(resp.revision, resp.backends);
                    }
                    item = stream.next() => {
                        let Some(item) = item else {
                            break;
                        };
                        match item {
                            Ok(msg) => {
                                if let Some(ev) = msg.event {
                                    cursor_revision = cursor_revision.max(ev.revision);
                                }
                                let resp = match client.list_remote_backends(ListRemoteBackendsRequest {}).await {
                                    Ok(r) => r.into_inner(),
                                    Err(e) => {
                                        tracing::warn!(error = %e, "List remote backends from SMS failed after watch event");
                                        break;
                                    }
                                };
                                cursor_revision = cursor_revision.max(resp.revision);
                                self.apply_snapshot(resp.revision, resp.backends);
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, code = ?e.code(), "WatchRemoteBackends stream error");
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

    fn apply_snapshot(&self, revision: u64, backends: Vec<crate::proto::sms::BackendSpec>) -> Option<u64> {
        if revision == 0 {
            return None;
        }
        let prev = self.applied_revision.load(Ordering::Acquire);
        if revision == prev {
            return None;
        }

        let allow_override = self.merge_policy == RemoteBackendMergePolicy::SmsWinsByName;
        let base_names = if allow_override {
            std::collections::HashSet::<String>::new()
        } else {
            self.base_config
                .ai
                .backends
                .iter()
                .filter_map(|b| {
                    let n = b.name.trim();
                    if n.is_empty() {
                        None
                    } else {
                        Some(n.to_string())
                    }
                })
                .collect()
        };

        let mut out: Vec<BackendInfo> = Vec::new();
        for mut spec in backends.into_iter() {
            if spec.name.trim().is_empty() {
                continue;
            }
            if !allow_override && base_names.contains(spec.name.as_str()) {
                continue;
            }
            spec.origin = BackendOrigin::Sms as i32;
            out.push(BackendInfo {
                spec: Some(spec),
                status: BackendStatus::Available as i32,
                status_reason: String::new(),
            });
        }
        global_dynamic_backends().set_backends(DynamicBackendSource::Sms, out);
        self.applied_revision.store(revision, Ordering::Release);
        tracing::info!(
            revision = revision,
            merge_policy = ?self.merge_policy,
            "Applied SMS remote backends revision"
        );
        Some(revision)
    }
}

impl Controller for RemoteBackendSyncService {
    fn name(&self) -> &'static str {
        "remote_backend_sync"
    }

    fn start(&self) {
        RemoteBackendSyncService::start(self)
    }

    fn shutdown(&self) {
        RemoteBackendSyncService::shutdown(self)
    }
}
