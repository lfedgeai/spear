//! Placement service for unified AI backends / 统一 AI backend 的 placement 服务

use std::sync::Arc;

use chrono::Utc;
use uuid::Uuid;

use crate::sms::ai_backends::model::{
    AiBackendDesiredStateModel, AiBackendPlacementRecordModel, ResolvedBackendAssignment,
};
use crate::sms::ai_backends::repository::AiBackendRepository;
use crate::sms::ai_backends::validator::validate_placement_record;
use crate::sms::services::error::SmsError;

/// Input used to create or update one placement / 创建或更新单个 placement 的输入
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpsertPlacementInput {
    pub placement_id: Option<String>,
    pub backend_id: String,
    pub node_uuid: String,
    pub desired_state: AiBackendDesiredStateModel,
    pub weight_override: Option<i32>,
    pub priority_override: Option<i32>,
}

/// Placement application service / placement 应用服务
#[derive(Clone)]
pub struct AiBackendPlacementService {
    repository: Arc<dyn AiBackendRepository>,
}

impl AiBackendPlacementService {
    /// Create a service from a repository / 基于仓储创建服务
    pub fn new(repository: Arc<dyn AiBackendRepository>) -> Self {
        Self { repository }
    }

    /// Create or update one placement / 创建或更新单个 placement
    pub async fn upsert_placement(
        &self,
        input: UpsertPlacementInput,
    ) -> Result<AiBackendPlacementRecordModel, SmsError> {
        if self
            .repository
            .get_backend(&input.backend_id)
            .await?
            .is_none()
        {
            return Err(SmsError::NotFound(format!(
                "backend {} not found",
                input.backend_id
            )));
        }

        let now_ms = Utc::now().timestamp_millis();
        let placement_id = input
            .placement_id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let existing = self.repository.get_placement(&placement_id).await?;
        let record = AiBackendPlacementRecordModel {
            placement_id,
            backend_id: input.backend_id,
            node_uuid: input.node_uuid,
            desired_state: input.desired_state,
            weight_override: input.weight_override,
            priority_override: input.priority_override,
            generation: existing
                .as_ref()
                .map(|value| value.generation.saturating_add(1))
                .unwrap_or(1),
            created_at_ms: existing
                .as_ref()
                .map(|value| value.created_at_ms)
                .unwrap_or(now_ms),
            updated_at_ms: now_ms,
        };
        validate_placement_record(&record)?;
        self.repository.upsert_placement(record).await
    }

    /// Delete one placement / 删除单个 placement
    pub async fn delete_placement(&self, placement_id: &str) -> Result<bool, SmsError> {
        self.repository.delete_placement(placement_id).await
    }

    /// Resolve the active assignments for one node / 解析某个节点的活跃 assignment
    pub async fn list_node_assignments(
        &self,
        node_uuid: &str,
    ) -> Result<Vec<ResolvedBackendAssignment>, SmsError> {
        let placements = self.repository.list_placements_by_node(node_uuid).await?;
        let mut assignments = Vec::new();
        for placement in placements {
            if !placement.is_enabled() {
                continue;
            }
            if let Some(backend) = self.repository.get_backend(&placement.backend_id).await? {
                if backend.is_enabled() {
                    assignments.push(ResolvedBackendAssignment { backend, placement });
                }
            }
        }
        assignments.sort_by(|left, right| left.backend.backend_id.cmp(&right.backend.backend_id));
        Ok(assignments)
    }
}
