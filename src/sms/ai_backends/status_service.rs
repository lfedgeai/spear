//! Node status service for unified AI backends / 统一 AI backend 的节点状态服务

use std::sync::Arc;

use chrono::Utc;

use crate::sms::ai_backends::model::AiBackendNodeStatusRecordModel;
use crate::sms::ai_backends::repository::AiBackendRepository;
use crate::sms::services::error::SmsError;

/// Status reporting application service / 状态上报应用服务
#[derive(Clone)]
pub struct AiBackendStatusService {
    repository: Arc<dyn AiBackendRepository>,
}

impl AiBackendStatusService {
    /// Create a service from a repository / 基于仓储创建服务
    pub fn new(repository: Arc<dyn AiBackendRepository>) -> Self {
        Self { repository }
    }

    /// Report latest statuses from one node / 上报某个节点的最新状态
    pub async fn report_statuses(
        &self,
        node_uuid: &str,
        records: Vec<AiBackendNodeStatusRecordModel>,
    ) -> Result<(), SmsError> {
        let now_ms = Utc::now().timestamp_millis();
        for mut record in records {
            if record.node_uuid != node_uuid {
                return Err(SmsError::InvalidRequest(format!(
                    "status node_uuid {} does not match reporter {node_uuid}",
                    record.node_uuid
                )));
            }
            if self
                .repository
                .get_backend(&record.backend_id)
                .await?
                .is_none()
            {
                return Err(SmsError::NotFound(format!(
                    "backend {} not found",
                    record.backend_id
                )));
            }
            if record.last_heartbeat_at_ms == 0 {
                record.last_heartbeat_at_ms = now_ms;
            }
            self.repository.upsert_node_status(record).await?;
        }
        Ok(())
    }

    /// List statuses for one backend / 列出某个 backend 的全部状态
    pub async fn list_backend_statuses(
        &self,
        backend_id: &str,
    ) -> Result<Vec<AiBackendNodeStatusRecordModel>, SmsError> {
        let mut statuses = self
            .repository
            .list_node_statuses_by_backend(backend_id)
            .await?;
        statuses.sort_by(|left, right| left.node_uuid.cmp(&right.node_uuid));
        Ok(statuses)
    }

    /// Remove stale statuses older than the provided timestamp / 删除早于给定时间戳的陈旧状态
    pub async fn cleanup_stale_statuses(&self, older_than_ms: i64) -> Result<u64, SmsError> {
        let statuses = self.repository.list_node_statuses().await?;
        let mut removed = 0_u64;
        for status in statuses {
            if status.last_heartbeat_at_ms < older_than_ms {
                if self
                    .repository
                    .delete_node_status(&status.backend_id, &status.node_uuid)
                    .await?
                {
                    removed = removed.saturating_add(1);
                }
            }
        }
        Ok(removed)
    }
}
