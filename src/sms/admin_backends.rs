use std::sync::Arc;

use tokio::sync::RwLock;
use tonic::Status;

use crate::proto::sms::{BackendHosting, BackendOrigin, BackendSpec, RemoteBackendEvent};
use crate::sms::registry_watch::{RegistryWatchHub, WatchStream};
use crate::storage::kv::{serialization, KvStore};

pub const ADMIN_REMOTE_BACKENDS_KEY: &str = "admin:ai:remote_backends:v1";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub(crate) struct BackendSpecRecord {
    name: String,
    kind: String,
    base_url: String,
    model: String,
    pub(crate) credential_ref: String,
    weight: u32,
    priority: i32,
    operations: Vec<String>,
    features: Vec<String>,
    transports: Vec<String>,
    provider: String,
    hosting: i32,
    origin: i32,
    deployment_id: String,
}

impl From<BackendSpec> for BackendSpecRecord {
    fn from(v: BackendSpec) -> Self {
        Self {
            name: v.name,
            kind: v.kind,
            base_url: v.base_url,
            model: v.model,
            credential_ref: v.credential_ref,
            weight: v.weight,
            priority: v.priority,
            operations: v.operations,
            features: v.features,
            transports: v.transports,
            provider: v.provider,
            hosting: v.hosting,
            origin: v.origin,
            deployment_id: v.deployment_id,
        }
    }
}

impl From<BackendSpecRecord> for BackendSpec {
    fn from(v: BackendSpecRecord) -> Self {
        Self {
            name: v.name,
            kind: v.kind,
            operations: v.operations,
            features: v.features,
            transports: v.transports,
            weight: v.weight,
            priority: v.priority,
            base_url: v.base_url,
            provider: v.provider,
            model: v.model,
            hosting: v.hosting,
            credential_ref: v.credential_ref,
            origin: v.origin,
            deployment_id: v.deployment_id,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub(crate) struct BackendSpecSnapshot {
    revision: u64,
    pub(crate) backends: Vec<BackendSpecRecord>,
}

#[derive(Debug, Clone)]
pub struct BackendSpecSnapshotView {
    pub revision: u64,
    pub backends: Vec<BackendSpec>,
}

#[derive(Debug)]
pub struct AdminBackendsState {
    kv: Arc<dyn KvStore>,
    snapshot: RwLock<BackendSpecSnapshot>,
    watch: RegistryWatchHub<RemoteBackendEvent>,
}

impl AdminBackendsState {
    pub async fn new(kv: Arc<dyn KvStore>) -> Self {
        let snapshot = match kv.get(&ADMIN_REMOTE_BACKENDS_KEY.to_string()).await {
            Ok(Some(bytes)) => serialization::deserialize::<BackendSpecSnapshot>(&bytes)
                .unwrap_or_default(),
            _ => BackendSpecSnapshot::default(),
        };
        Self {
            kv,
            snapshot: RwLock::new(snapshot),
            watch: RegistryWatchHub::new(1024, 1024),
        }
    }

    pub async fn list(&self) -> BackendSpecSnapshotView {
        let snap = self.snapshot.read().await.clone();
        BackendSpecSnapshotView {
            revision: snap.revision,
            backends: snap.backends.into_iter().map(BackendSpec::from).collect(),
        }
    }

    pub async fn watch_remote_backends(&self, since_revision: u64) -> Result<WatchStream<RemoteBackendEvent>, Status> {
        self.watch
            .watch(since_revision, |e| e.revision)
            .await
    }

    pub async fn upsert(&self, mut backend: BackendSpec) -> Result<u64, Status> {
        if backend.name.trim().is_empty() {
            return Err(Status::invalid_argument("missing name"));
        }
        if backend.kind.trim().is_empty() {
            return Err(Status::invalid_argument("missing kind"));
        }
        if backend.base_url.trim().is_empty() {
            return Err(Status::invalid_argument("missing base_url"));
        }
        if backend.operations.is_empty() {
            return Err(Status::invalid_argument("missing operations"));
        }

        backend.hosting = BackendHosting::Remote as i32;
        backend.origin = BackendOrigin::Sms as i32;
        backend.deployment_id = String::new();

        let name = backend.name.clone();
        let backend: BackendSpecRecord = backend.into();
        let mut snap = self.snapshot.write().await;
        let mut replaced = false;
        for b in snap.backends.iter_mut() {
            if b.name == backend.name {
                *b = backend.clone();
                replaced = true;
                break;
            }
        }
        if !replaced {
            snap.backends.push(backend);
        }
        snap.backends.sort_by(|a, b| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
        });
        snap.revision = snap.revision.saturating_add(1);

        let bytes =
            serialization::serialize(&*snap).map_err(|e| Status::internal(e.to_string()))?;
        self.kv
            .put(&ADMIN_REMOTE_BACKENDS_KEY.to_string(), &bytes)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        self.watch
            .push_event(RemoteBackendEvent {
                revision: snap.revision,
                upserts: vec![name],
                deletes: Vec::new(),
            })
            .await;

        Ok(snap.revision)
    }

    pub async fn delete(&self, name: &str) -> Result<(u64, bool), Status> {
        let n = name.trim();
        if n.is_empty() {
            return Err(Status::invalid_argument("missing name"));
        }
        let mut snap = self.snapshot.write().await;
        let before = snap.backends.len();
        snap.backends.retain(|b| b.name != n);
        let deleted = snap.backends.len() != before;
        if deleted {
            snap.revision = snap.revision.saturating_add(1);
            let bytes =
                serialization::serialize(&*snap).map_err(|e| Status::internal(e.to_string()))?;
            self.kv
                .put(&ADMIN_REMOTE_BACKENDS_KEY.to_string(), &bytes)
                .await
                .map_err(|e| Status::internal(e.to_string()))?;

            self.watch
                .push_event(RemoteBackendEvent {
                    revision: snap.revision,
                    upserts: Vec::new(),
                    deletes: vec![n.to_string()],
                })
                .await;
        }
        Ok((snap.revision, deleted))
    }
}
