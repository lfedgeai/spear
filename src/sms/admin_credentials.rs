use crate::proto::sms::{
    CredentialEvent, CredentialInfo, CredentialMaterial, WatchCredentialMaterialsResponse,
    WatchCredentialsResponse,
};
use crate::sms::ai_backends::model::AiBackendRecordModel;
use crate::sms::registry_watch::{RegistryWatchHub, WatchStream};
use crate::storage::kv::{serialization, KvStore};
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use futures::StreamExt;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::Status;

const ADMIN_CREDENTIALS_KEY: &str = "sms_admin_credentials_v1";
const SMS_CREDENTIAL_MASTER_KEY_ENV: &str = "SMS_CREDENTIAL_MASTER_KEY";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct CredentialSnapshot {
    revision: u64,
    credentials: Vec<CredentialRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CredentialRecord {
    name: String,
    provider_kind: String,
    encrypted_secret: Vec<u8>,
    version: u64,
    description: String,
    disabled: bool,
    created_at_ms: i64,
    updated_at_ms: i64,
}

pub trait SecretCipher: Send + Sync {
    fn encrypt(&self, plaintext: &str) -> Result<Vec<u8>, Status>;
    fn decrypt(&self, ciphertext: &[u8]) -> Result<String, Status>;
}

pub struct AesGcmSecretCipher {
    cipher: Aes256Gcm,
}

impl AesGcmSecretCipher {
    pub fn from_env() -> Result<Self, Status> {
        let secret = std::env::var(SMS_CREDENTIAL_MASTER_KEY_ENV).map_err(|_| {
            Status::failed_precondition(format!(
                "{} must be configured before managing credentials",
                SMS_CREDENTIAL_MASTER_KEY_ENV
            ))
        })?;
        let digest = Sha256::digest(secret.as_bytes());
        let key = Key::<Aes256Gcm>::from_slice(&digest);
        Ok(Self {
            cipher: Aes256Gcm::new(key),
        })
    }

    pub fn from_env_if_present() -> Result<Option<Self>, Status> {
        match std::env::var(SMS_CREDENTIAL_MASTER_KEY_ENV) {
            Ok(_) => Self::from_env().map(Some),
            Err(std::env::VarError::NotPresent) => Ok(None),
            Err(std::env::VarError::NotUnicode(_)) => Err(Status::failed_precondition(format!(
                "{} must be valid utf-8",
                SMS_CREDENTIAL_MASTER_KEY_ENV
            ))),
        }
    }
}

impl SecretCipher for AesGcmSecretCipher {
    fn encrypt(&self, plaintext: &str) -> Result<Vec<u8>, Status> {
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = self
            .cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| Status::internal(format!("encrypt credential failed: {}", e)))?;
        let mut out = nonce_bytes.to_vec();
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    fn decrypt(&self, ciphertext: &[u8]) -> Result<String, Status> {
        if ciphertext.len() < 12 {
            return Err(Status::internal("credential ciphertext too short"));
        }
        let (nonce_bytes, body) = ciphertext.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);
        let plaintext = self
            .cipher
            .decrypt(nonce, body)
            .map_err(|e| Status::internal(format!("decrypt credential failed: {}", e)))?;
        String::from_utf8(plaintext)
            .map_err(|e| Status::internal(format!("credential plaintext invalid utf8: {}", e)))
    }
}

pub struct AdminCredentialsState {
    kv: Arc<dyn KvStore>,
    snapshot: RwLock<CredentialSnapshot>,
    watch: RegistryWatchHub<CredentialEvent>,
    cipher: Option<Arc<dyn SecretCipher>>,
}

impl std::fmt::Debug for AdminCredentialsState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdminCredentialsState").finish_non_exhaustive()
    }
}

impl AdminCredentialsState {
    pub async fn new(kv: Arc<dyn KvStore>) -> Result<Self, Status> {
        let loaded = kv
            .get(&ADMIN_CREDENTIALS_KEY.to_string())
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        let snapshot = if let Some(bytes) = loaded {
            serialization::deserialize::<CredentialSnapshot>(&bytes)
                .map_err(|e| Status::internal(e.to_string()))?
        } else {
            CredentialSnapshot::default()
        };
        Ok(Self {
            kv,
            snapshot: RwLock::new(snapshot),
            watch: RegistryWatchHub::new(256, 256),
            cipher: AesGcmSecretCipher::from_env_if_present()?
                .map(|cipher| Arc::new(cipher) as Arc<dyn SecretCipher>),
        })
    }

    fn require_cipher(&self) -> Result<Arc<dyn SecretCipher>, Status> {
        self.cipher.clone().ok_or_else(|| {
            Status::failed_precondition(format!(
                "{} must be configured before managing credentials",
                SMS_CREDENTIAL_MASTER_KEY_ENV
            ))
        })
    }

    pub async fn list_infos(&self) -> Result<(u64, Vec<CredentialInfo>), Status> {
        let snap = self.snapshot.read().await;
        Ok((
            snap.revision,
            snap.credentials.iter().cloned().map(info_from_record).collect(),
        ))
    }

    pub async fn list_materials(&self) -> Result<(u64, Vec<CredentialMaterial>), Status> {
        let cipher = self.require_cipher()?;
        let snap = self.snapshot.read().await;
        let mut materials = Vec::with_capacity(snap.credentials.len());
        for record in snap.credentials.iter() {
            materials.push(material_from_record(record, cipher.as_ref())?);
        }
        Ok((snap.revision, materials))
    }

    pub async fn watch_infos(&self, since_revision: u64) -> Result<WatchStream<WatchCredentialsResponse>, Status> {
        let stream = self.watch.watch(since_revision, |e| e.revision).await?;
        Ok(Box::pin(stream.map(|item| item.map(|event| WatchCredentialsResponse { event: Some(event) }))))
    }

    pub async fn watch_materials(
        &self,
        since_revision: u64,
    ) -> Result<WatchStream<WatchCredentialMaterialsResponse>, Status> {
        let stream = self.watch.watch(since_revision, |e| e.revision).await?;
        Ok(Box::pin(stream.map(|item| {
            item.map(|event| WatchCredentialMaterialsResponse { event: Some(event) })
        })))
    }

    pub async fn upsert(
        &self,
        name: &str,
        secret: &str,
        description: &str,
        disabled: bool,
    ) -> Result<u64, Status> {
        let n = name.trim();
        if n.is_empty() {
            return Err(Status::invalid_argument("missing name"));
        }
        let s = secret.trim();
        let now_ms = chrono::Utc::now().timestamp_millis();
        let mut snap = self.snapshot.write().await;
        let mut upserted_name = n.to_string();
        let mut replaced = false;
        for credential in snap.credentials.iter_mut() {
            if credential.name == n {
                // Empty secret keeps the current encrypted secret for metadata-only edits.
                // 空 secret 表示仅更新元数据，保留当前密文 secret。
                if !s.is_empty() {
                    let cipher = self.require_cipher()?;
                    credential.encrypted_secret = cipher.encrypt(s)?;
                }
                credential.provider_kind = "inline_encrypted".to_string();
                credential.version = credential.version.saturating_add(1);
                credential.description = description.trim().to_string();
                credential.disabled = disabled;
                credential.updated_at_ms = now_ms;
                replaced = true;
                break;
            }
        }
        if !replaced {
            if s.is_empty() {
                return Err(Status::invalid_argument("missing secret"));
            }
            let cipher = self.require_cipher()?;
            snap.credentials.push(CredentialRecord {
                name: n.to_string(),
                provider_kind: "inline_encrypted".to_string(),
                encrypted_secret: cipher.encrypt(s)?,
                version: 1,
                description: description.trim().to_string(),
                disabled,
                created_at_ms: now_ms,
                updated_at_ms: now_ms,
            });
        }
        snap.credentials.sort_by(|a, b| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
        });
        snap.revision = snap.revision.saturating_add(1);
        persist_snapshot(self.kv.as_ref(), &snap).await?;
        self.watch
            .push_event(CredentialEvent {
                revision: snap.revision,
                upserts: vec![std::mem::take(&mut upserted_name)],
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
        ensure_not_referenced(self.kv.as_ref(), n).await?;
        let mut snap = self.snapshot.write().await;
        let before = snap.credentials.len();
        snap.credentials.retain(|c| c.name != n);
        let deleted = snap.credentials.len() != before;
        if deleted {
            snap.revision = snap.revision.saturating_add(1);
            persist_snapshot(self.kv.as_ref(), &snap).await?;
            self.watch
                .push_event(CredentialEvent {
                    revision: snap.revision,
                    upserts: Vec::new(),
                    deletes: vec![n.to_string()],
                })
                .await;
        }
        Ok((snap.revision, deleted))
    }
}

async fn persist_snapshot(kv: &dyn KvStore, snapshot: &CredentialSnapshot) -> Result<(), Status> {
    let bytes = serialization::serialize(snapshot).map_err(|e| Status::internal(e.to_string()))?;
    kv.put(&ADMIN_CREDENTIALS_KEY.to_string(), &bytes)
        .await
        .map_err(|e| Status::internal(e.to_string()))
}

async fn ensure_not_referenced(kv: &dyn KvStore, credential_name: &str) -> Result<(), Status> {
    let backend_pairs = kv
        .scan_prefix("ai:backend:")
        .await
        .map_err(|e| Status::internal(e.to_string()))?;

    let referenced = backend_pairs
        .iter()
        .filter_map(|pair| serialization::deserialize::<AiBackendRecordModel>(&pair.value).ok())
        .any(|backend| {
            backend
                .credential_ref
                .as_deref()
                .map(|name| name.trim() == credential_name)
                .unwrap_or(false)
        });

    if referenced {
        return Err(Status::failed_precondition(format!(
            "credential is still referenced by backend(s): {}",
            credential_name
        )));
    }
    Ok(())
}

fn info_from_record(record: CredentialRecord) -> CredentialInfo {
    CredentialInfo {
        name: record.name,
        provider_kind: record.provider_kind,
        version: record.version,
        description: record.description,
        disabled: record.disabled,
        created_at_ms: record.created_at_ms,
        updated_at_ms: record.updated_at_ms,
    }
}

fn material_from_record(
    record: &CredentialRecord,
    cipher: &dyn SecretCipher,
) -> Result<CredentialMaterial, Status> {
    Ok(CredentialMaterial {
        name: record.name.clone(),
        provider_kind: record.provider_kind.clone(),
        secret: cipher.decrypt(&record.encrypted_secret)?,
        version: record.version,
        disabled: record.disabled,
        updated_at_ms: record.updated_at_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::kv::MemoryKvStore;

    #[test]
    fn cipher_roundtrip() {
        std::env::set_var(SMS_CREDENTIAL_MASTER_KEY_ENV, "test-master-key");
        let cipher = AesGcmSecretCipher::from_env().expect("cipher");
        let encrypted = cipher.encrypt("sk-test").expect("encrypt");
        let decrypted = cipher.decrypt(&encrypted).expect("decrypt");
        assert_eq!(decrypted, "sk-test");
    }

    #[tokio::test]
    async fn upsert_and_list_credentials() {
        std::env::set_var(SMS_CREDENTIAL_MASTER_KEY_ENV, "test-master-key");
        let kv = Arc::new(MemoryKvStore::new());
        let state = AdminCredentialsState::new(kv).await.expect("state");
        let revision = state
            .upsert("openai_default", "sk-test", "primary", false)
            .await
            .expect("upsert");
        assert_eq!(revision, 1);

        let (listed_revision, infos) = state.list_infos().await.expect("list infos");
        assert_eq!(listed_revision, 1);
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].name, "openai_default");

        let (_, materials) = state.list_materials().await.expect("list materials");
        assert_eq!(materials.len(), 1);
        assert_eq!(materials[0].secret, "sk-test");
    }

    #[tokio::test]
    async fn upsert_existing_credential_keeps_secret_when_secret_is_empty() {
        std::env::set_var(SMS_CREDENTIAL_MASTER_KEY_ENV, "test-master-key");
        let kv = Arc::new(MemoryKvStore::new());
        let state = AdminCredentialsState::new(kv).await.expect("state");
        state
            .upsert("openai_default", "sk-test", "primary", false)
            .await
            .expect("seed");

        let revision = state
            .upsert("openai_default", "", "rotated metadata", true)
            .await
            .expect("update without secret");
        assert_eq!(revision, 2);

        let (_, infos) = state.list_infos().await.expect("list infos");
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].description, "rotated metadata");
        assert!(infos[0].disabled);
        assert_eq!(infos[0].version, 2);

        let (_, materials) = state.list_materials().await.expect("list materials");
        assert_eq!(materials.len(), 1);
        assert_eq!(materials[0].secret, "sk-test");
    }
}
