use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, Weak};
use std::time::Duration;

use parking_lot::RwLock;
use serde::Serialize;
use tokio::sync::oneshot;
use tokio::time::interval;
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;
use tonic::transport::Channel;

use crate::proto::sms::admin_credential_service_client::AdminCredentialServiceClient;
use crate::proto::sms::{
    ListCredentialMaterialsRequest, WatchCredentialMaterialsRequest,
};
use crate::spearlet::ai::dynamic_credential_store::global_dynamic_credentials;
use crate::spearlet::controller::Controller;

#[derive(Debug, Clone, Serialize)]
pub struct CredentialSyncStatusSnapshot {
    pub status: String,
    pub started: bool,
    pub watch_connected: bool,
    pub applied_revision: u64,
    pub local_store_epoch: u64,
    pub credential_count: usize,
    pub last_success_at_ms: Option<i64>,
    pub last_error: Option<String>,
}

#[derive(Debug, Default)]
struct CredentialSyncStatusState {
    started: AtomicBool,
    watch_connected: AtomicBool,
    applied_revision: AtomicU64,
    last_success_at_ms: AtomicI64,
    last_error: RwLock<Option<String>>,
}

impl CredentialSyncStatusState {
    fn mark_started(&self) {
        self.started.store(true, Ordering::Release);
        self.clear_error();
    }

    fn mark_stopped(&self) {
        self.started.store(false, Ordering::Release);
        self.watch_connected.store(false, Ordering::Release);
    }

    fn mark_watch_connected(&self, connected: bool) {
        self.watch_connected.store(connected, Ordering::Release);
    }

    fn record_success(&self, revision: u64) {
        self.applied_revision.store(revision, Ordering::Release);
        self.last_success_at_ms
            .store(chrono::Utc::now().timestamp_millis(), Ordering::Release);
        self.clear_error();
    }

    fn record_error(&self, message: impl Into<String>) {
        *self.last_error.write() = Some(message.into());
        self.watch_connected.store(false, Ordering::Release);
    }

    fn clear_error(&self) {
        *self.last_error.write() = None;
    }

    fn snapshot(&self) -> CredentialSyncStatusSnapshot {
        let started = self.started.load(Ordering::Acquire);
        let watch_connected = self.watch_connected.load(Ordering::Acquire);
        let applied_revision = self.applied_revision.load(Ordering::Acquire);
        let last_success_at_ms = match self.last_success_at_ms.load(Ordering::Acquire) {
            0 => None,
            ts => Some(ts),
        };
        let last_error = self.last_error.read().clone();
        let local_store = global_dynamic_credentials();
        let credential_count = local_store.count();
        let local_store_epoch = local_store.revision();
        let status = if !started {
            "disabled"
        } else if watch_connected {
            "ready"
        } else if last_error.is_some() {
            "degraded"
        } else {
            "syncing"
        };
        CredentialSyncStatusSnapshot {
            status: status.to_string(),
            started,
            watch_connected,
            applied_revision,
            local_store_epoch,
            credential_count,
            last_success_at_ms,
            last_error,
        }
    }
}

static GLOBAL_CREDENTIAL_SYNC_STATUS: OnceLock<CredentialSyncStatusState> = OnceLock::new();

pub fn global_credential_sync_status_snapshot() -> CredentialSyncStatusSnapshot {
    GLOBAL_CREDENTIAL_SYNC_STATUS
        .get_or_init(CredentialSyncStatusState::default)
        .snapshot()
}

pub struct CredentialSyncService {
    sms_channel: Channel,
    poll_interval: Duration,
    cancel: CancellationToken,
    applied_revision: AtomicU64,
    started: AtomicBool,
    self_weak: Weak<CredentialSyncService>,
}

impl CredentialSyncService {
    pub fn new(sms_channel: Channel, poll_interval: Duration) -> Arc<Self> {
        Arc::new_cyclic(|weak| Self {
            sms_channel,
            poll_interval: poll_interval.max(Duration::from_secs(1)),
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
        GLOBAL_CREDENTIAL_SYNC_STATUS
            .get_or_init(CredentialSyncStatusState::default)
            .mark_started();
        if let Some(svc) = self.self_weak.upgrade() {
            svc.spawn();
        }
    }

    pub fn shutdown(&self) {
        self.cancel.cancel();
        global_dynamic_credentials().clear();
        GLOBAL_CREDENTIAL_SYNC_STATUS
            .get_or_init(CredentialSyncStatusState::default)
            .mark_stopped();
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
                    .expect("build credential sync runtime");
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

            let mut client = AdminCredentialServiceClient::new(self.sms_channel.clone());
            let resp = match client
                .list_credential_materials(ListCredentialMaterialsRequest {})
                .await
            {
                Ok(r) => {
                    backoff_ms = 200;
                    GLOBAL_CREDENTIAL_SYNC_STATUS
                        .get_or_init(CredentialSyncStatusState::default)
                        .clear_error();
                    r.into_inner()
                }
                Err(e) => {
                    GLOBAL_CREDENTIAL_SYNC_STATUS
                        .get_or_init(CredentialSyncStatusState::default)
                        .record_error(format!("list failed: {}", e));
                    tracing::warn!(error = %e, "List credential materials from SMS failed");
                    tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                    backoff_ms = (backoff_ms * 2).min(10_000);
                    continue;
                }
            };

            cursor_revision = cursor_revision.max(resp.revision);
            self.apply_snapshot(resp.revision, resp.credentials);

            let watch_resp = client
                .watch_credential_materials(WatchCredentialMaterialsRequest {
                    since_revision: cursor_revision,
                })
                .await;
            let mut stream = match watch_resp {
                Ok(r) => {
                    backoff_ms = 200;
                    GLOBAL_CREDENTIAL_SYNC_STATUS
                        .get_or_init(CredentialSyncStatusState::default)
                        .mark_watch_connected(true);
                    r.into_inner()
                }
                Err(e) => {
                    GLOBAL_CREDENTIAL_SYNC_STATUS
                        .get_or_init(CredentialSyncStatusState::default)
                        .record_error(format!("watch failed: {}", e));
                    tracing::warn!(error = %e, cursor_revision, "WatchCredentialMaterials failed");
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
                        let resp = match client.list_credential_materials(ListCredentialMaterialsRequest {}).await {
                            Ok(r) => r.into_inner(),
                            Err(e) => {
                                GLOBAL_CREDENTIAL_SYNC_STATUS
                                    .get_or_init(CredentialSyncStatusState::default)
                                    .record_error(format!("periodic refresh failed: {}", e));
                                tracing::warn!(error = %e, "List credential materials from SMS failed in periodic refresh");
                                break;
                            }
                        };
                        cursor_revision = cursor_revision.max(resp.revision);
                        self.apply_snapshot(resp.revision, resp.credentials);
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
                                let resp = match client.list_credential_materials(ListCredentialMaterialsRequest {}).await {
                                    Ok(r) => r.into_inner(),
                                    Err(e) => {
                                        GLOBAL_CREDENTIAL_SYNC_STATUS
                                            .get_or_init(CredentialSyncStatusState::default)
                                            .record_error(format!("watch refresh failed: {}", e));
                                        tracing::warn!(error = %e, "List credential materials from SMS failed after watch event");
                                        break;
                                    }
                                };
                                cursor_revision = cursor_revision.max(resp.revision);
                                self.apply_snapshot(resp.revision, resp.credentials);
                            }
                            Err(e) => {
                                GLOBAL_CREDENTIAL_SYNC_STATUS
                                    .get_or_init(CredentialSyncStatusState::default)
                                    .record_error(format!("watch stream error: {}", e));
                                tracing::warn!(error = %e, code = ?e.code(), "WatchCredentialMaterials stream error");
                                break;
                            }
                        }
                    }
                }
            }

            GLOBAL_CREDENTIAL_SYNC_STATUS
                .get_or_init(CredentialSyncStatusState::default)
                .mark_watch_connected(false);
            tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
            backoff_ms = (backoff_ms * 2).min(10_000);
        }
    }

    fn apply_snapshot(&self, revision: u64, credentials: Vec<crate::proto::sms::CredentialMaterial>) {
        if revision == 0 {
            return;
        }
        let prev = self.applied_revision.load(Ordering::Acquire);
        if revision == prev {
            return;
        }
        global_dynamic_credentials().set_credentials(credentials);
        self.applied_revision.store(revision, Ordering::Release);
        GLOBAL_CREDENTIAL_SYNC_STATUS
            .get_or_init(CredentialSyncStatusState::default)
            .record_success(revision);
        tracing::info!(revision, "Applied SMS credential materials revision");
    }
}

impl Controller for CredentialSyncService {
    fn name(&self) -> &'static str {
        "credential_sync"
    }

    fn start(&self) {
        CredentialSyncService::start(self)
    }

    fn shutdown(&self) {
        CredentialSyncService::shutdown(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_sync_status_snapshot_reports_ready_state() {
        let _guard = crate::spearlet::ai::dynamic_credential_store::global_dynamic_credentials_test_lock()
            .lock()
            .expect("lock");
        let store = global_dynamic_credentials();
        store.clear();
        store.set_credentials(vec![crate::proto::sms::CredentialMaterial {
            name: "openai-default".to_string(),
            provider_kind: "inline_encrypted".to_string(),
            secret: "sk-test".to_string(),
            version: 1,
            disabled: false,
            updated_at_ms: 123,
        }]);

        let state = CredentialSyncStatusState::default();
        state.mark_started();
        state.mark_watch_connected(true);
        state.record_success(7);

        let snapshot = state.snapshot();
        assert_eq!(snapshot.status, "ready");
        assert!(snapshot.started);
        assert!(snapshot.watch_connected);
        assert_eq!(snapshot.applied_revision, 7);
        assert_eq!(snapshot.credential_count, 1);
        assert!(snapshot.last_success_at_ms.is_some());

        store.clear();
    }
}
