use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use tokio::time::interval;
use tokio_util::sync::CancellationToken;
use tonic::transport::Channel;
use tracing::{debug, warn};

use crate::proto::sms::mcp_registry_service_client::McpRegistryServiceClient;
use crate::proto::sms::{ListMcpServersRequest, McpServerRecord, WatchMcpServersRequest};
use crate::spearlet::config::SpearletConfig;
use crate::spearlet::controller::Controller;

#[derive(Debug, Clone, Default)]
pub struct McpRegistrySnapshot {
    pub revision: u64,
    pub servers: Vec<McpServerRecord>,
}

#[derive(Debug, Default)]
pub struct McpRegistryCache {
    inner: parking_lot::RwLock<Arc<McpRegistrySnapshot>>,
}

impl McpRegistryCache {
    pub fn snapshot(&self) -> Arc<McpRegistrySnapshot> {
        self.inner.read().clone()
    }

    fn replace(&self, snapshot: McpRegistrySnapshot) {
        *self.inner.write() = Arc::new(snapshot);
    }
}

#[derive(Debug)]
pub struct McpRegistrySyncService {
    config: Arc<SpearletConfig>,
    sms_channel: Option<Channel>,
    cache: Arc<McpRegistryCache>,
    cancel: CancellationToken,
    started: AtomicBool,
}

impl McpRegistrySyncService {
    pub fn new(config: Arc<SpearletConfig>, sms_channel: Option<Channel>) -> Self {
        Self {
            config,
            sms_channel,
            cache: Arc::new(McpRegistryCache::default()),
            cancel: CancellationToken::new(),
            started: AtomicBool::new(false),
        }
    }

    pub fn cache(&self) -> Arc<McpRegistryCache> {
        let snap = self.cache.snapshot();
        let server_ids = snap
            .servers
            .iter()
            .map(|s| s.server_id.clone())
            .collect::<Vec<_>>();
        debug!(
            revision = snap.revision,
            server_count = server_ids.len(),
            server_ids = ?server_ids,
            "MCP registry cache snapshot (cache() called)"
        );
        self.cache.clone()
    }

    pub fn shutdown(&self) {
        self.cancel.cancel();
    }

    pub fn start(&self) {
        debug!("MCP registry sync service started");
        if self.started.swap(true, Ordering::AcqRel) {
            return;
        }

        let config = self.config.clone();
        let sms_channel = self.sms_channel.clone();
        let cache = self.cache.clone();
        let cancel = self.cancel.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                sync_loop(config, sms_channel, cache, cancel).await;
            });
            return;
        }

        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            if let Ok(rt) = rt {
                rt.block_on(async move {
                    sync_loop(config, sms_channel, cache, cancel).await;
                });
            }
        });
    }
}

impl Controller for McpRegistrySyncService {
    fn name(&self) -> &'static str {
        "mcp_registry_sync"
    }

    fn start(&self) {
        McpRegistrySyncService::start(self)
    }

    fn shutdown(&self) {
        McpRegistrySyncService::shutdown(self)
    }
}

async fn refresh_once(
    config: &SpearletConfig,
    client: &mut McpRegistryServiceClient<Channel>,
    cache: &McpRegistryCache,
) -> Result<u64, tonic::Status> {
    let per_attempt = Duration::from_millis(config.sms_connect_timeout_ms)
        .min(Duration::from_secs(5))
        .max(Duration::from_millis(1));
    let resp = tokio::time::timeout(
        per_attempt,
        client.list_mcp_servers(ListMcpServersRequest { since_revision: 0 }),
    )
    .await
    .map_err(|_| tonic::Status::deadline_exceeded("list_mcp_servers timeout"))??
    .into_inner();
    let server_ids = resp
        .servers
        .iter()
        .map(|s| s.server_id.clone())
        .collect::<Vec<_>>();
    let snapshot = McpRegistrySnapshot {
        revision: resp.revision,
        servers: resp.servers,
    };
    cache.replace(snapshot);
    debug!(
        revision = resp.revision,
        server_count = server_ids.len(),
        server_ids = ?server_ids,
        "MCP registry snapshot replaced"
    );
    Ok(resp.revision)
}

static GLOBAL_MCP_REGISTRY_SYNC: OnceLock<Arc<McpRegistrySyncService>> = OnceLock::new();

pub fn global_mcp_registry_sync(config: Arc<SpearletConfig>) -> Arc<McpRegistrySyncService> {
    GLOBAL_MCP_REGISTRY_SYNC
        .get_or_init(|| {
            let svc = Arc::new(McpRegistrySyncService::new(config, None));
            svc.start();
            svc
        })
        .clone()
}

pub fn global_mcp_registry_sync_with_channel(
    config: Arc<SpearletConfig>,
    sms_channel: Option<Channel>,
) -> Arc<McpRegistrySyncService> {
    GLOBAL_MCP_REGISTRY_SYNC
        .get_or_init(|| {
            let svc = Arc::new(McpRegistrySyncService::new(config, sms_channel));
            svc.start();
            svc
        })
        .clone()
}

async fn sync_loop(
    config: Arc<SpearletConfig>,
    sms_channel: Option<Channel>,
    cache: Arc<McpRegistryCache>,
    cancel: CancellationToken,
) {
    let mut backoff_ms = config.sms_connect_retry_ms.max(200);
    let mut watch_cursor_revision: u64;
    let mut poll = interval(Duration::from_secs(60));

    let Some(channel) = sms_channel else {
        return;
    };

    loop {
        if cancel.is_cancelled() {
            return;
        }

        let mut client = McpRegistryServiceClient::new(channel.clone());

        match refresh_once(&config, &mut client, &cache).await {
            Ok(r) => {
                watch_cursor_revision = r;
                debug!(watch_cursor_revision, "MCP registry snapshot refreshed");
                backoff_ms = config.sms_connect_retry_ms.max(200);
            }
            Err(e) => {
                warn!(error = %e, "MCP registry list failed");
                tokio::select! {
                    _ = cancel.cancelled() => return,
                    _ = tokio::time::sleep(Duration::from_millis(backoff_ms)) => {}
                }
                backoff_ms = (backoff_ms * 2).min(10_000);
                continue;
            }
        }

        let per_attempt = Duration::from_millis(config.sms_connect_timeout_ms)
            .min(Duration::from_secs(5))
            .max(Duration::from_millis(1));
        let mut watch_stream = match tokio::time::timeout(
            per_attempt,
            client.watch_mcp_servers(WatchMcpServersRequest {
                since_revision: watch_cursor_revision,
            }),
        )
        .await
        {
            Ok(Ok(r)) => r.into_inner(),
            Ok(Err(e)) => {
                warn!(error = %e, "MCP registry watch start failed");
                continue;
            }
            Err(_) => {
                warn!("MCP registry watch start timeout");
                continue;
            }
        };

        loop {
            tokio::select! {
                _ = cancel.cancelled() => return,
                _ = poll.tick() => {
                    if let Ok(r) = refresh_once(&config, &mut client, &cache).await {
                        watch_cursor_revision = r;
                    }
                }
                msg = watch_stream.message() => {
                    match msg {
                        Ok(Some(resp)) => {
                            if let Some(event) = resp.event {
                                if event.revision > watch_cursor_revision {
                                    if let Ok(r) = refresh_once(&config, &mut client, &cache).await {
                                        watch_cursor_revision = r;
                                    }
                                }
                            }
                        }
                        Ok(None) => break,
                        Err(e) => {
                            warn!(error = %e, "MCP registry watch ended");
                            break;
                        }
                    }
                }
            }
        }
    }
}
