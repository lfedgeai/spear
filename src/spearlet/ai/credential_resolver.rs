use std::collections::HashMap;
use std::sync::OnceLock;

use crate::spearlet::ai::credential_sync::global_credential_sync_status_snapshot;
use crate::spearlet::ai::dynamic_credential_store::{
    global_dynamic_credentials, DynamicCredentialLookup,
};
use crate::spearlet::config::{AiCredentialConfig, SpearletConfig};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CredentialResolution {
    Ready(String),
    Disabled,
    NotSynced,
    Missing,
}

impl CredentialResolution {
    pub fn code(&self) -> &'static str {
        match self {
            CredentialResolution::Ready(_) => "ok",
            CredentialResolution::Disabled => "credential_disabled",
            CredentialResolution::NotSynced => "credential_not_synced",
            CredentialResolution::Missing => "credential_missing",
        }
    }

    pub fn message(&self, credential_ref: &str) -> String {
        match self {
            CredentialResolution::Ready(_) => {
                format!("credential_ref '{}' is available", credential_ref)
            }
            CredentialResolution::Disabled => {
                format!(
                    "credential_ref '{}' is disabled on this node",
                    credential_ref
                )
            }
            CredentialResolution::NotSynced => format!(
                "credential_ref '{}' has not been synced to this node yet",
                credential_ref
            ),
            CredentialResolution::Missing => format!(
                "credential_ref '{}' is not available on this node",
                credential_ref
            ),
        }
    }

    pub fn into_secret(self) -> Option<String> {
        match self {
            CredentialResolution::Ready(secret) => Some(secret),
            CredentialResolution::Disabled
            | CredentialResolution::NotSynced
            | CredentialResolution::Missing => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CredentialResolver {
    by_name: HashMap<String, AiCredentialConfig>,
    // Optional runtime env overlay. Prefer this map first, then fall back to process env.
    // 可选的运行时环境变量覆盖层：优先读取该 map，再回退到进程环境变量。
    env_overrides: HashMap<String, String>,
}

impl CredentialResolver {
    pub fn from_config(cfg: &SpearletConfig) -> Self {
        let by_name = cfg
            .ai
            .credentials
            .iter()
            .cloned()
            .filter(|c| !c.name.trim().is_empty())
            .map(|c| (c.name.clone(), c))
            .collect();
        Self {
            by_name,
            env_overrides: HashMap::new(),
        }
    }

    pub fn from_config_with_env(cfg: &SpearletConfig, env: &HashMap<String, String>) -> Self {
        let mut r = Self::from_config(cfg);
        r.env_overrides = env.clone();
        r
    }

    fn read_env(&self, name: &str) -> Option<String> {
        if let Some(v) = self.env_overrides.get(name) {
            if !v.trim().is_empty() {
                return Some(v.clone());
            }
        }
        std::env::var(name).ok().filter(|v| !v.trim().is_empty())
    }

    pub fn resolve_api_key_state(&self, credential_ref: Option<&str>) -> CredentialResolution {
        let Some(r) = credential_ref.map(|s| s.trim()).filter(|s| !s.is_empty()) else {
            return CredentialResolution::Missing;
        };
        match global_dynamic_credentials().lookup(r) {
            DynamicCredentialLookup::Ready(secret) => return CredentialResolution::Ready(secret),
            DynamicCredentialLookup::Disabled => return CredentialResolution::Disabled,
            DynamicCredentialLookup::NotSynced => return CredentialResolution::NotSynced,
            DynamicCredentialLookup::Missing => {}
        }

        if let Some(cred) = self.by_name.get(r) {
            if cred.kind != "env" {
                return CredentialResolution::Missing;
            }
            let env = cred.api_key_env.trim();
            if env.is_empty() {
                return CredentialResolution::Missing;
            }
            return self
                .read_env(env)
                .map(CredentialResolution::Ready)
                .unwrap_or(CredentialResolution::Missing);
        }

        let sync = global_credential_sync_status_snapshot();
        if sync.started && sync.last_success_at_ms.is_none() {
            return CredentialResolution::NotSynced;
        }
        CredentialResolution::Missing
    }

    pub fn resolve_api_key(&self, credential_ref: Option<&str>) -> Option<String> {
        self.resolve_api_key_state(credential_ref).into_secret()
    }
}

static GLOBAL: OnceLock<CredentialResolver> = OnceLock::new();

pub fn init_global(cfg: &SpearletConfig) -> CredentialResolver {
    GLOBAL
        .get_or_init(|| CredentialResolver::from_config(cfg))
        .clone()
}

pub fn global() -> Option<CredentialResolver> {
    GLOBAL.get().cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::sms::CredentialMaterial;

    #[test]
    fn resolve_api_key_state_reports_disabled_dynamic_credential() {
        let _guard =
            crate::spearlet::ai::dynamic_credential_store::global_dynamic_credentials_test_lock()
                .lock()
                .expect("lock");
        let store = global_dynamic_credentials();
        store.clear();
        store.set_credentials(vec![CredentialMaterial {
            name: "openai-default".to_string(),
            provider_kind: "inline_encrypted".to_string(),
            secret: "sk-test".to_string(),
            version: 1,
            disabled: true,
            updated_at_ms: 0,
        }]);

        let resolver = CredentialResolver::from_config(&SpearletConfig::default());
        assert_eq!(
            resolver.resolve_api_key_state(Some("openai-default")),
            CredentialResolution::Disabled
        );
        store.clear();
    }

    #[test]
    fn resolve_api_key_state_reports_env_missing_for_config_credential() {
        let _guard =
            crate::spearlet::ai::dynamic_credential_store::global_dynamic_credentials_test_lock()
                .lock()
                .expect("lock");
        let store = global_dynamic_credentials();
        store.clear();
        let mut cfg = SpearletConfig::default();
        cfg.ai.credentials.push(AiCredentialConfig {
            name: "openai-env".to_string(),
            kind: "env".to_string(),
            api_key_env: "OPENAI_ENV_KEY".to_string(),
        });

        let resolver = CredentialResolver::from_config_with_env(&cfg, &HashMap::new());
        assert_eq!(
            resolver.resolve_api_key_state(Some("openai-env")),
            CredentialResolution::Missing
        );
    }
}
