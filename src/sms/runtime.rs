use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tracing::warn;

use crate::proto::sms::EventOp;
use crate::sms::ai_backends::KvAiBackendRepository;
use crate::sms::admin_credentials::AdminCredentialsState;
use crate::sms::config::SmsConfig;
use crate::sms::instance_execution_index::InstanceExecutionIndex;
use crate::sms::service::SmsServiceImpl;
use crate::sms::services::{node_service::NodeService, resource_service::ResourceService};
use crate::sms::unified_events::UnifiedEventBus;
use crate::storage::kv::{
    create_kv_store_from_config, get_kv_store_factory, KvStore, KvStoreConfig,
};

pub(crate) struct SmsRuntimeStores {
    pub(crate) unified_events: Arc<UnifiedEventBus>,
    pub(crate) instance_execution_index: Arc<InstanceExecutionIndex>,
    pub(crate) admin_credentials: Arc<AdminCredentialsState>,
    pub(crate) ai_backend_repository: Arc<KvAiBackendRepository>,
}

fn select_event_kv_config(config: &SmsConfig) -> KvStoreConfig {
    let supported = get_kv_store_factory().supported_backends();
    if let Some(ev) = &config.event_kv {
        let backend = if supported.contains(&ev.backend) {
            ev.backend.clone()
        } else {
            "memory".to_string()
        };
        KvStoreConfig {
            backend,
            params: ev.params.clone(),
        }
    } else {
        KvStoreConfig {
            backend: "memory".to_string(),
            params: std::collections::HashMap::new(),
        }
    }
}

fn select_admin_kv_config(config: &SmsConfig) -> KvStoreConfig {
    let supported = get_kv_store_factory().supported_backends();
    let backend = if supported.contains(&config.database.db_type) {
        config.database.db_type.clone()
    } else {
        "memory".to_string()
    };
    let mut params = std::collections::HashMap::new();
    if backend != "memory" {
        params.insert("path".to_string(), config.database.path.clone());
    }
    KvStoreConfig { backend, params }
}

async fn create_kv_or_panic(kv_cfg: &KvStoreConfig, panic_message: &str) -> Arc<dyn KvStore> {
    let kv_box = create_kv_store_from_config(kv_cfg)
        .await
        .expect(panic_message);
    Arc::from(kv_box)
}

pub(crate) async fn build_runtime_stores(config: Arc<SmsConfig>) -> SmsRuntimeStores {
    let event_kv = create_kv_or_panic(
        &select_event_kv_config(&config),
        "Failed to create KV store from config",
    )
    .await;
    let unified_events = Arc::new(UnifiedEventBus::new(event_kv.clone()));
    let stale_after_ms = (config.heartbeat_timeout as i64).saturating_mul(2_000);
    let instance_execution_index =
        Arc::new(InstanceExecutionIndex::new(event_kv, 256, 1000, stale_after_ms));

    let admin_kv_cfg = select_admin_kv_config(&config);
    let admin_kv_box = match create_kv_store_from_config(&admin_kv_cfg).await {
        Ok(v) => v,
        Err(_) => create_kv_store_from_config(&KvStoreConfig {
            backend: "memory".to_string(),
            params: std::collections::HashMap::new(),
        })
        .await
        .expect("Failed to create admin KV store"),
    };
    let admin_kv: Arc<dyn KvStore> = Arc::from(admin_kv_box);
    let admin_credentials = Arc::new(
        AdminCredentialsState::new(admin_kv.clone())
            .await
            .expect("Failed to create admin credential state"),
    );
    let ai_backend_repository = Arc::new(KvAiBackendRepository::new(admin_kv.clone()));

    SmsRuntimeStores {
        unified_events,
        instance_execution_index,
        admin_credentials,
        ai_backend_repository,
    }
}

pub(crate) fn start_cleanup_loop(
    node_service: Arc<RwLock<NodeService>>,
    resource_service: Arc<ResourceService>,
    config: Arc<SmsConfig>,
    unified_events: Arc<UnifiedEventBus>,
) {
    tokio::spawn(async move {
        let mut t = tokio::time::interval(Duration::from_secs(config.cleanup_interval.max(1)));
        loop {
            t.tick().await;
            let updated_nodes = {
                let mut svc = node_service.write().await;
                svc.mark_unhealthy_nodes_offline(config.heartbeat_timeout)
                    .await
                    .unwrap_or_default()
            };
            if !updated_nodes.is_empty() {
                tracing::info!(
                    count = updated_nodes.len(),
                    heartbeat_timeout_s = config.heartbeat_timeout,
                    nodes = ?updated_nodes,
                    "Marked unhealthy nodes offline"
                );
                for mark in updated_nodes.iter() {
                    if mark.previous_status.to_ascii_lowercase() == "offline" {
                        continue;
                    }
                    if let Err(e) = unified_events
                        .publish_node_event(&mark.node, EventOp::Update)
                        .await
                    {
                        warn!(error = %e, uuid = %mark.uuid, "Publish unified node offline event failed");
                    }
                }
            }
            let _ = resource_service
                .cleanup_stale_resources(config.heartbeat_timeout)
                .await;
        }
    });
}

pub(crate) fn start_assignment_reconcile_loop(service: SmsServiceImpl) {
    tokio::spawn(async move {
        let mut t = tokio::time::interval(Duration::from_secs(
            service.config.assignment_reconcile_interval.max(1),
        ));
        loop {
            t.tick().await;
            if let Err(e) = service.reconcile_all_task_assignments().await {
                warn!(error = %e, "Periodic task assignment reconcile failed");
            }
        }
    });
}
