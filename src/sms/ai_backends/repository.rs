//! Repository abstractions for unified AI backends / 统一 AI backend 的仓储抽象

use async_trait::async_trait;

use crate::sms::ai_backends::model::{
    AiBackendNodeStatusRecordModel, AiBackendPlacementRecordModel, AiBackendRecordModel,
};
use crate::sms::services::error::SmsError;

/// Repository contract for backend control-plane records / backend 控制面记录的仓储契约
#[async_trait]
pub trait AiBackendRepository: Send + Sync {
    /// Insert or replace a backend record / 插入或替换 backend 记录
    async fn upsert_backend(
        &self,
        record: AiBackendRecordModel,
    ) -> Result<AiBackendRecordModel, SmsError>;

    /// Fetch one backend by identity / 按身份读取单个 backend
    async fn get_backend(&self, backend_id: &str) -> Result<Option<AiBackendRecordModel>, SmsError>;

    /// List all backends / 列出所有 backend
    async fn list_backends(&self) -> Result<Vec<AiBackendRecordModel>, SmsError>;

    /// Delete one backend / 删除单个 backend
    async fn delete_backend(&self, backend_id: &str) -> Result<bool, SmsError>;

    /// Insert or replace a placement record / 插入或替换 placement 记录
    async fn upsert_placement(
        &self,
        record: AiBackendPlacementRecordModel,
    ) -> Result<AiBackendPlacementRecordModel, SmsError>;

    /// Fetch one placement by identity / 按身份读取单个 placement
    async fn get_placement(
        &self,
        placement_id: &str,
    ) -> Result<Option<AiBackendPlacementRecordModel>, SmsError>;

    /// List all placements / 列出所有 placement
    async fn list_placements(&self) -> Result<Vec<AiBackendPlacementRecordModel>, SmsError>;

    /// List placements owned by one backend / 列出某个 backend 关联的 placement
    async fn list_placements_by_backend(
        &self,
        backend_id: &str,
    ) -> Result<Vec<AiBackendPlacementRecordModel>, SmsError>;

    /// List placements assigned to one node / 列出某个节点关联的 placement
    async fn list_placements_by_node(
        &self,
        node_uuid: &str,
    ) -> Result<Vec<AiBackendPlacementRecordModel>, SmsError>;

    /// Delete one placement / 删除单个 placement
    async fn delete_placement(&self, placement_id: &str) -> Result<bool, SmsError>;

    /// Insert or replace a node status record / 插入或替换节点状态记录
    async fn upsert_node_status(
        &self,
        record: AiBackendNodeStatusRecordModel,
    ) -> Result<AiBackendNodeStatusRecordModel, SmsError>;

    /// Fetch one node status record / 读取单条节点状态记录
    async fn get_node_status(
        &self,
        backend_id: &str,
        node_uuid: &str,
    ) -> Result<Option<AiBackendNodeStatusRecordModel>, SmsError>;

    /// List all node statuses / 列出所有节点状态
    async fn list_node_statuses(&self) -> Result<Vec<AiBackendNodeStatusRecordModel>, SmsError>;

    /// List statuses for one backend / 列出某个 backend 的全部状态
    async fn list_node_statuses_by_backend(
        &self,
        backend_id: &str,
    ) -> Result<Vec<AiBackendNodeStatusRecordModel>, SmsError>;

    /// Delete one node status / 删除单条节点状态
    async fn delete_node_status(&self, backend_id: &str, node_uuid: &str) -> Result<bool, SmsError>;
}
