//! Backend control-plane service / backend 控制面服务

use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::Utc;
use uuid::Uuid;

use crate::sms::ai_backends::model::{
    AiBackendDesiredStateModel, AiBackendHostingModel, AiBackendManagementModeModel,
    AiBackendRecordModel, AiBackendSpecModel,
};
use crate::sms::ai_backends::repository::AiBackendRepository;
use crate::sms::ai_backends::validator::validate_backend_record;
use crate::sms::services::error::SmsError;

/// Input used to create a backend / 创建 backend 的输入
#[derive(Debug, Clone, PartialEq)]
pub struct CreateAiBackendInput {
    pub display_name: String,
    pub provider: String,
    pub model: String,
    pub hosting: AiBackendHostingModel,
    pub backend_kind: String,
    pub management_mode: AiBackendManagementModeModel,
    pub credential_ref: Option<String>,
    pub spec: AiBackendSpecModel,
    pub labels: BTreeMap<String, String>,
    pub metadata: serde_json::Value,
    pub desired_state: AiBackendDesiredStateModel,
}

/// Input used to update a backend / 更新 backend 的输入
#[derive(Debug, Clone, PartialEq)]
pub struct UpdateAiBackendInput {
    pub backend_id: String,
    pub display_name: String,
    pub provider: String,
    pub model: String,
    pub hosting: AiBackendHostingModel,
    pub backend_kind: String,
    pub management_mode: AiBackendManagementModeModel,
    pub credential_ref: Option<String>,
    pub spec: AiBackendSpecModel,
    pub labels: BTreeMap<String, String>,
    pub metadata: serde_json::Value,
    pub desired_state: AiBackendDesiredStateModel,
}

/// Backend CRUD service / backend CRUD 服务
#[derive(Clone)]
pub struct AiBackendService {
    repository: Arc<dyn AiBackendRepository>,
}

impl AiBackendService {
    /// Create a service from a repository / 基于仓储创建服务
    pub fn new(repository: Arc<dyn AiBackendRepository>) -> Self {
        Self { repository }
    }

    /// Create a backend with a fresh UUID / 使用新的 UUID 创建 backend
    pub async fn create_backend(
        &self,
        input: CreateAiBackendInput,
    ) -> Result<AiBackendRecordModel, SmsError> {
        let backend_id = Uuid::new_v4().to_string();
        let now_ms = Utc::now().timestamp_millis();
        let record = AiBackendRecordModel {
            backend_id: backend_id.clone(),
            display_name: input.display_name,
            provider: input.provider,
            model: input.model,
            hosting: input.hosting,
            backend_kind: input.backend_kind,
            desired_state: input.desired_state,
            management_mode: input.management_mode,
            credential_ref: input.credential_ref,
            spec: normalize_spec(input.spec, &backend_id),
            labels: input.labels,
            metadata: input.metadata,
            generation: 1,
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
        };
        validate_backend_record(&record)?;
        self.repository.upsert_backend(record).await
    }

    /// Update a backend by replacing its desired specification / 通过替换期望规格更新 backend
    pub async fn update_backend(
        &self,
        input: UpdateAiBackendInput,
    ) -> Result<AiBackendRecordModel, SmsError> {
        let existing = self
            .repository
            .get_backend(&input.backend_id)
            .await?
            .ok_or_else(|| SmsError::NotFound(format!("backend {} not found", input.backend_id)))?;
        let record = AiBackendRecordModel {
            backend_id: existing.backend_id.clone(),
            display_name: input.display_name,
            provider: input.provider,
            model: input.model,
            hosting: input.hosting,
            backend_kind: input.backend_kind,
            desired_state: input.desired_state,
            management_mode: input.management_mode,
            credential_ref: input.credential_ref,
            spec: normalize_spec(input.spec, &existing.backend_id),
            labels: input.labels,
            metadata: input.metadata,
            generation: existing.generation.saturating_add(1),
            created_at_ms: existing.created_at_ms,
            updated_at_ms: Utc::now().timestamp_millis(),
        };
        validate_backend_record(&record)?;
        self.repository.upsert_backend(record).await
    }

    /// Fetch one backend / 读取单个 backend
    pub async fn get_backend(
        &self,
        backend_id: &str,
    ) -> Result<Option<AiBackendRecordModel>, SmsError> {
        self.repository.get_backend(backend_id).await
    }

    /// List all backends / 列出所有 backend
    pub async fn list_backends(&self) -> Result<Vec<AiBackendRecordModel>, SmsError> {
        let mut backends = self.repository.list_backends().await?;
        backends.sort_by(|left, right| left.display_name.cmp(&right.display_name));
        Ok(backends)
    }

    /// Delete a backend and its derived state / 删除 backend 及其派生状态
    pub async fn delete_backend(&self, backend_id: &str) -> Result<bool, SmsError> {
        let placements = self
            .repository
            .list_placements_by_backend(backend_id)
            .await?;
        for placement in placements {
            self.repository
                .delete_placement(&placement.placement_id)
                .await?;
        }

        let statuses = self
            .repository
            .list_node_statuses_by_backend(backend_id)
            .await?;
        for status in statuses {
            self.repository
                .delete_node_status(&status.backend_id, &status.node_uuid)
                .await?;
        }

        self.repository.delete_backend(backend_id).await
    }

    /// Set the desired state on one backend / 设置单个 backend 的期望状态
    pub async fn set_desired_state(
        &self,
        backend_id: &str,
        desired_state: AiBackendDesiredStateModel,
    ) -> Result<AiBackendRecordModel, SmsError> {
        let mut record = self
            .repository
            .get_backend(backend_id)
            .await?
            .ok_or_else(|| SmsError::NotFound(format!("backend {backend_id} not found")))?;
        record.desired_state = desired_state;
        record.generation = record.generation.saturating_add(1);
        record.updated_at_ms = Utc::now().timestamp_millis();
        validate_backend_record(&record)?;
        self.repository.upsert_backend(record).await
    }
}

/// Normalize backend spec fields owned by the control plane / 归一化由控制面托管的 spec 字段
fn normalize_spec(mut spec: AiBackendSpecModel, backend_id: &str) -> AiBackendSpecModel {
    spec.name = AiBackendSpecModel::generated_name(backend_id);
    spec.provider = spec.provider.trim().to_string();
    spec.model = spec.model.trim().to_string();
    spec.kind = spec.kind.trim().to_string();
    spec
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sms::ai_backends::repository_kv::KvAiBackendRepository;
    use crate::storage::MemoryKvStore;

    fn sample_create_input() -> CreateAiBackendInput {
        CreateAiBackendInput {
            display_name: "OpenAI Prod".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4.1".to_string(),
            hosting: AiBackendHostingModel::Remote,
            backend_kind: "openai-compatible".to_string(),
            management_mode: AiBackendManagementModeModel::SmsRemote,
            credential_ref: Some("cred-1".to_string()),
            spec: AiBackendSpecModel {
                name: "ignored".to_string(),
                kind: "openai-compatible".to_string(),
                operations: vec!["chat".to_string()],
                features: vec!["stream".to_string()],
                transports: vec!["http".to_string()],
                weight: 1,
                priority: 0,
                base_url: "https://example.com".to_string(),
                provider: "openai".to_string(),
                model: "gpt-4.1".to_string(),
                credential_ref: "cred-1".to_string(),
                origin: 0,
                deployment_id: String::new(),
            },
            labels: BTreeMap::new(),
            metadata: serde_json::json!({}),
            desired_state: AiBackendDesiredStateModel::Enabled,
        }
    }

    #[tokio::test]
    async fn create_backend_generates_uuid_identity_and_runtime_name() {
        let repository = Arc::new(KvAiBackendRepository::new(Arc::new(MemoryKvStore::new())));
        let service = AiBackendService::new(repository);

        let created = service.create_backend(sample_create_input()).await.unwrap();

        assert_eq!(created.generation, 1);
        assert_eq!(
            created.spec.name,
            AiBackendSpecModel::generated_name(&created.backend_id)
        );
        assert!(!created.backend_id.trim().is_empty());
    }
}
