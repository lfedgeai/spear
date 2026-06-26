use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use parking_lot::RwLock;

use crate::proto::sms::CredentialMaterial;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DynamicCredentialLookup {
    Ready(String),
    Disabled,
    NotSynced,
    Missing,
}

#[derive(Clone, Debug, Default)]
pub struct DynamicCredentialStore {
    by_name: Arc<RwLock<HashMap<String, CredentialMaterial>>>,
    revision: Arc<AtomicU64>,
}

impl DynamicCredentialStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_credentials(&self, credentials: Vec<CredentialMaterial>) {
        let mut next = HashMap::new();
        for credential in credentials {
            let name = credential.name.trim();
            if name.is_empty() {
                continue;
            }
            next.insert(name.to_string(), credential);
        }
        let mut guard = self.by_name.write();
        *guard = next;
        self.revision.fetch_add(1, Ordering::Relaxed);
    }

    pub fn clear(&self) {
        let mut guard = self.by_name.write();
        guard.clear();
        self.revision.fetch_add(1, Ordering::Relaxed);
    }

    pub fn lookup(&self, credential_ref: &str) -> DynamicCredentialLookup {
        let name = credential_ref.trim();
        if name.is_empty() {
            return DynamicCredentialLookup::Missing;
        }
        let guard = self.by_name.read();
        let Some(credential) = guard.get(name) else {
            return DynamicCredentialLookup::Missing;
        };
        if credential.disabled {
            return DynamicCredentialLookup::Disabled;
        }
        let secret = credential.secret.trim();
        if secret.is_empty() {
            DynamicCredentialLookup::NotSynced
        } else {
            DynamicCredentialLookup::Ready(secret.to_string())
        }
    }

    pub fn get_secret(&self, credential_ref: &str) -> Option<String> {
        match self.lookup(credential_ref) {
            DynamicCredentialLookup::Ready(secret) => Some(secret),
            DynamicCredentialLookup::Disabled
            | DynamicCredentialLookup::NotSynced
            | DynamicCredentialLookup::Missing => None,
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::Relaxed)
    }

    pub fn count(&self) -> usize {
        self.by_name.read().len()
    }
}

static GLOBAL_DYNAMIC_CREDENTIALS: OnceLock<DynamicCredentialStore> = OnceLock::new();

pub fn global_dynamic_credentials() -> DynamicCredentialStore {
    GLOBAL_DYNAMIC_CREDENTIALS
        .get_or_init(DynamicCredentialStore::new)
        .clone()
}

#[cfg(test)]
static GLOBAL_DYNAMIC_CREDENTIALS_TEST_LOCK: OnceLock<std::sync::Mutex<()>> = OnceLock::new();

#[cfg(test)]
pub fn global_dynamic_credentials_test_lock() -> &'static std::sync::Mutex<()> {
    GLOBAL_DYNAMIC_CREDENTIALS_TEST_LOCK.get_or_init(|| std::sync::Mutex::new(()))
}
