use std::collections::{HashMap, HashSet};

use crate::spearlet::config::SpearletConfig;

pub mod dynamic_backend_registry;
pub mod dynamic_credential_store;
pub mod credential_sync;
pub mod credential_resolver;
pub mod backend_assembly;
pub mod backend_assignment_controller;

pub fn collect_ai_global_environment(cfg: &SpearletConfig) -> HashMap<String, String> {
    let mut cred_env: HashMap<String, String> = HashMap::new();
    for c in cfg.ai.credentials.iter() {
        if c.kind.as_str() != "env" {
            continue;
        }
        if c.name.trim().is_empty() {
            continue;
        }
        if c.api_key_env.trim().is_empty() {
            continue;
        }
        cred_env.insert(c.name.clone(), c.api_key_env.clone());
    }

    let mut required: HashSet<String> = HashSet::new();
    for b in cfg.ai.backends.iter() {
        let Some(r) = b.credential_ref.as_deref().map(|s| s.trim()) else {
            continue;
        };
        if r.is_empty() {
            continue;
        }
        let Some(env) = cred_env.get(r) else {
            continue;
        };
        required.insert(env.clone());
    }

    let mut out: HashMap<String, String> = HashMap::new();
    for env_name in required.into_iter() {
        if let Ok(v) = std::env::var(&env_name) {
            if !v.is_empty() {
                out.insert(env_name, v);
            }
        }
    }
    out
}
