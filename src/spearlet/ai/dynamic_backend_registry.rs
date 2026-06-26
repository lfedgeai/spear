use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use parking_lot::RwLock;

use crate::proto::sms::BackendInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DynamicBackendSource {
    LocalController,
    Sms,
}

#[derive(Clone, Debug, Default)]
pub struct DynamicBackendRegistry {
    by_source: Arc<RwLock<HashMap<DynamicBackendSource, Vec<BackendInfo>>>>,
    revision: Arc<AtomicU64>,
}

impl DynamicBackendRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_backends(&self, source: DynamicBackendSource, backends: Vec<BackendInfo>) {
        let mut guard = self.by_source.write();
        guard.insert(source, backends);
        self.revision.fetch_add(1, Ordering::Relaxed);
    }

    pub fn clear(&self, source: DynamicBackendSource) {
        let mut guard = self.by_source.write();
        guard.insert(source, Vec::new());
        self.revision.fetch_add(1, Ordering::Relaxed);
    }

    pub fn list_merged(&self, precedence: &[DynamicBackendSource]) -> Vec<BackendInfo> {
        let guard = self.by_source.read();
        let mut by_name: HashMap<String, BackendInfo> = HashMap::new();
        for src in precedence.iter().copied() {
            let Some(list) = guard.get(&src) else {
                continue;
            };
            for b in list.iter().cloned() {
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
        by_name.into_values().collect()
    }

    pub fn list_merged_sorted(&self, precedence: &[DynamicBackendSource]) -> Vec<BackendInfo> {
        let mut out = self.list_merged(precedence);
        out.sort_by(|a, b| {
            let an = a.spec.as_ref().map(|s| s.name.as_str()).unwrap_or("");
            let bn = b.spec.as_ref().map(|s| s.name.as_str()).unwrap_or("");
            an.to_ascii_lowercase().cmp(&bn.to_ascii_lowercase())
        });
        out
    }

    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::Relaxed)
    }
}

static GLOBAL_DYNAMIC_BACKENDS: OnceLock<DynamicBackendRegistry> = OnceLock::new();

pub fn global_dynamic_backends() -> DynamicBackendRegistry {
    GLOBAL_DYNAMIC_BACKENDS
        .get_or_init(DynamicBackendRegistry::new)
        .clone()
}
