//! KV-backed repository implementation / 基于 KV 的仓储实现

use async_trait::async_trait;
use serde::de::DeserializeOwned;
use std::sync::Arc;

use crate::sms::ai_backends::model::{
    AiBackendNodeStatusRecordModel, AiBackendPlacementRecordModel, AiBackendRecordModel,
};
use crate::sms::ai_backends::repository::AiBackendRepository;
use crate::sms::services::error::SmsError;
use crate::storage::kv::{serialization, KvStore};

/// Key namespace helpers / 键命名空间辅助
mod keys {
    /// Backend primary record prefix / backend 主记录前缀
    pub const BACKEND_PREFIX: &str = "ai:backend:";
    /// Placement primary record prefix / placement 主记录前缀
    pub const PLACEMENT_PREFIX: &str = "ai:placement:";
    /// Node status primary record prefix / 节点状态主记录前缀
    pub const STATUS_PREFIX: &str = "ai:status:";
    /// Placement-by-backend index prefix / 按 backend 建的 placement 索引前缀
    pub const PLACEMENT_BY_BACKEND_PREFIX: &str = "ai:index:placements_by_backend:";
    /// Placement-by-node index prefix / 按节点建的 placement 索引前缀
    pub const PLACEMENT_BY_NODE_PREFIX: &str = "ai:index:placements_by_node:";
    /// Status-by-backend index prefix / 按 backend 建的状态索引前缀
    pub const STATUS_BY_BACKEND_PREFIX: &str = "ai:index:statuses_by_backend:";

    /// Build a backend record key / 构建 backend 记录键
    pub fn backend(backend_id: &str) -> String {
        format!("{BACKEND_PREFIX}{backend_id}")
    }

    /// Build a placement record key / 构建 placement 记录键
    pub fn placement(placement_id: &str) -> String {
        format!("{PLACEMENT_PREFIX}{placement_id}")
    }

    /// Build a node status record key / 构建节点状态记录键
    pub fn status(backend_id: &str, node_uuid: &str) -> String {
        format!("{STATUS_PREFIX}{backend_id}:{node_uuid}")
    }

    /// Build a placement index by backend / 构建按 backend 的 placement 索引键
    pub fn placement_by_backend(backend_id: &str, placement_id: &str) -> String {
        format!("{PLACEMENT_BY_BACKEND_PREFIX}{backend_id}:{placement_id}")
    }

    /// Build a placement index by node / 构建按节点的 placement 索引键
    pub fn placement_by_node(node_uuid: &str, placement_id: &str) -> String {
        format!("{PLACEMENT_BY_NODE_PREFIX}{node_uuid}:{placement_id}")
    }

    /// Build a status index by backend / 构建按 backend 的状态索引键
    pub fn status_by_backend(backend_id: &str, node_uuid: &str) -> String {
        format!("{STATUS_BY_BACKEND_PREFIX}{backend_id}:{node_uuid}")
    }

    /// Prefix for all placement indexes owned by one backend / 某个 backend 下全部 placement 索引的前缀
    pub fn placement_by_backend_prefix(backend_id: &str) -> String {
        format!("{PLACEMENT_BY_BACKEND_PREFIX}{backend_id}:")
    }

    /// Prefix for all placement indexes assigned to one node / 某个节点下全部 placement 索引的前缀
    pub fn placement_by_node_prefix(node_uuid: &str) -> String {
        format!("{PLACEMENT_BY_NODE_PREFIX}{node_uuid}:")
    }

    /// Prefix for all status indexes owned by one backend / 某个 backend 下全部状态索引的前缀
    pub fn status_by_backend_prefix(backend_id: &str) -> String {
        format!("{STATUS_BY_BACKEND_PREFIX}{backend_id}:")
    }
}

/// Empty marker payload used for secondary indexes / 二级索引使用的空标记载荷
const INDEX_VALUE: &[u8] = b"1";

/// KV-backed repository for AI backend records / AI backend 记录的 KV 仓储实现
#[derive(Debug, Clone)]
pub struct KvAiBackendRepository {
    kv: Arc<dyn KvStore>,
}

impl KvAiBackendRepository {
    /// Create a repository from a KV store / 基于 KV 存储创建仓储
    pub fn new(kv: Arc<dyn KvStore>) -> Self {
        Self { kv }
    }

    /// Load a typed record from one key / 从单个键加载强类型记录
    async fn load_record<T>(&self, key: &str) -> Result<Option<T>, SmsError>
    where
        T: DeserializeOwned,
    {
        match self.kv.get(&key.to_string()).await? {
            Some(bytes) => serialization::deserialize(&bytes).map(Some),
            None => Ok(None),
        }
    }

    /// Persist a typed record to one key / 把强类型记录写入单个键
    async fn store_record<T>(&self, key: &str, value: &T) -> Result<(), SmsError>
    where
        T: serde::Serialize,
    {
        let bytes = serialization::serialize(value)?;
        self.kv.put(&key.to_string(), &bytes).await
    }

    /// Load records from a prefix of primary keys / 从主键前缀加载记录
    async fn list_records_by_prefix<T>(&self, prefix: &str) -> Result<Vec<T>, SmsError>
    where
        T: DeserializeOwned,
    {
        let pairs = self.kv.scan_prefix(prefix).await?;
        let mut values = Vec::with_capacity(pairs.len());
        for pair in pairs {
            values.push(serialization::deserialize::<T>(&pair.value)?);
        }
        Ok(values)
    }

    /// Load records by following index entries / 通过二级索引跟随读取主记录
    async fn load_records_via_index<T, F>(
        &self,
        index_prefix: &str,
        key_builder: F,
    ) -> Result<Vec<T>, SmsError>
    where
        T: DeserializeOwned,
        F: Fn(&str) -> String,
    {
        let index_keys = self.kv.keys_with_prefix(index_prefix).await?;
        let mut records = Vec::with_capacity(index_keys.len());
        for index_key in index_keys {
            let entity_id = index_key
                .rsplit(':')
                .next()
                .ok_or_else(|| SmsError::Serialization("invalid index key".to_string()))?;
            if let Some(record) = self.load_record::<T>(&key_builder(entity_id)).await? {
                records.push(record);
            }
        }
        Ok(records)
    }
}

#[async_trait]
impl AiBackendRepository for KvAiBackendRepository {
    async fn upsert_backend(
        &self,
        record: AiBackendRecordModel,
    ) -> Result<AiBackendRecordModel, SmsError> {
        self.store_record(&keys::backend(&record.backend_id), &record)
            .await?;
        Ok(record)
    }

    async fn get_backend(&self, backend_id: &str) -> Result<Option<AiBackendRecordModel>, SmsError> {
        self.load_record(&keys::backend(backend_id)).await
    }

    async fn list_backends(&self) -> Result<Vec<AiBackendRecordModel>, SmsError> {
        self.list_records_by_prefix(keys::BACKEND_PREFIX).await
    }

    async fn delete_backend(&self, backend_id: &str) -> Result<bool, SmsError> {
        self.kv.delete(&keys::backend(backend_id)).await
    }

    async fn upsert_placement(
        &self,
        record: AiBackendPlacementRecordModel,
    ) -> Result<AiBackendPlacementRecordModel, SmsError> {
        if let Some(previous) = self.get_placement(&record.placement_id).await? {
            self.kv
                .delete(&keys::placement_by_backend(
                    &previous.backend_id,
                    &previous.placement_id,
                ))
                .await?;
            self.kv
                .delete(&keys::placement_by_node(
                    &previous.node_uuid,
                    &previous.placement_id,
                ))
                .await?;
        }

        self.store_record(&keys::placement(&record.placement_id), &record)
            .await?;
        self.kv
            .put(
                &keys::placement_by_backend(&record.backend_id, &record.placement_id),
                &INDEX_VALUE.to_vec(),
            )
            .await?;
        self.kv
            .put(
                &keys::placement_by_node(&record.node_uuid, &record.placement_id),
                &INDEX_VALUE.to_vec(),
            )
            .await?;
        Ok(record)
    }

    async fn get_placement(
        &self,
        placement_id: &str,
    ) -> Result<Option<AiBackendPlacementRecordModel>, SmsError> {
        self.load_record(&keys::placement(placement_id)).await
    }

    async fn list_placements(&self) -> Result<Vec<AiBackendPlacementRecordModel>, SmsError> {
        self.list_records_by_prefix(keys::PLACEMENT_PREFIX).await
    }

    async fn list_placements_by_backend(
        &self,
        backend_id: &str,
    ) -> Result<Vec<AiBackendPlacementRecordModel>, SmsError> {
        self.load_records_via_index(&keys::placement_by_backend_prefix(backend_id), keys::placement)
            .await
    }

    async fn list_placements_by_node(
        &self,
        node_uuid: &str,
    ) -> Result<Vec<AiBackendPlacementRecordModel>, SmsError> {
        self.load_records_via_index(&keys::placement_by_node_prefix(node_uuid), keys::placement)
            .await
    }

    async fn delete_placement(&self, placement_id: &str) -> Result<bool, SmsError> {
        if let Some(previous) = self.get_placement(placement_id).await? {
            self.kv
                .delete(&keys::placement_by_backend(
                    &previous.backend_id,
                    &previous.placement_id,
                ))
                .await?;
            self.kv
                .delete(&keys::placement_by_node(
                    &previous.node_uuid,
                    &previous.placement_id,
                ))
                .await?;
        }
        self.kv.delete(&keys::placement(placement_id)).await
    }

    async fn upsert_node_status(
        &self,
        record: AiBackendNodeStatusRecordModel,
    ) -> Result<AiBackendNodeStatusRecordModel, SmsError> {
        self.store_record(&keys::status(&record.backend_id, &record.node_uuid), &record)
            .await?;
        self.kv
            .put(
                &keys::status_by_backend(&record.backend_id, &record.node_uuid),
                &INDEX_VALUE.to_vec(),
            )
            .await?;
        Ok(record)
    }

    async fn get_node_status(
        &self,
        backend_id: &str,
        node_uuid: &str,
    ) -> Result<Option<AiBackendNodeStatusRecordModel>, SmsError> {
        self.load_record(&keys::status(backend_id, node_uuid)).await
    }

    async fn list_node_statuses(&self) -> Result<Vec<AiBackendNodeStatusRecordModel>, SmsError> {
        self.list_records_by_prefix(keys::STATUS_PREFIX).await
    }

    async fn list_node_statuses_by_backend(
        &self,
        backend_id: &str,
    ) -> Result<Vec<AiBackendNodeStatusRecordModel>, SmsError> {
        self.load_records_via_index(&keys::status_by_backend_prefix(backend_id), |node_uuid| {
            keys::status(backend_id, node_uuid)
        })
        .await
    }

    async fn delete_node_status(&self, backend_id: &str, node_uuid: &str) -> Result<bool, SmsError> {
        self.kv
            .delete(&keys::status_by_backend(backend_id, node_uuid))
            .await?;
        self.kv.delete(&keys::status(backend_id, node_uuid)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sms::ai_backends::model::{
        AiBackendDesiredStateModel, AiBackendHostingModel, AiBackendManagementModeModel,
        AiBackendNodeRuntimeStatusModel, AiBackendSpecModel,
    };
    use crate::storage::MemoryKvStore;
    use std::collections::BTreeMap;

    fn sample_backend() -> AiBackendRecordModel {
        AiBackendRecordModel {
            backend_id: "backend-1".to_string(),
            display_name: "Backend 1".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4.1".to_string(),
            hosting: AiBackendHostingModel::Remote,
            backend_kind: "openai-compatible".to_string(),
            desired_state: AiBackendDesiredStateModel::Enabled,
            management_mode: AiBackendManagementModeModel::SmsRemote,
            credential_ref: Some("cred-1".to_string()),
            spec: AiBackendSpecModel {
                name: "backend-backend-1".to_string(),
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
            generation: 1,
            created_at_ms: 1,
            updated_at_ms: 1,
        }
    }

    #[tokio::test]
    async fn stores_and_indexes_placements() {
        let repo = KvAiBackendRepository::new(Arc::new(MemoryKvStore::new()));
        let placement = AiBackendPlacementRecordModel {
            placement_id: "placement-1".to_string(),
            backend_id: "backend-1".to_string(),
            node_uuid: "node-1".to_string(),
            desired_state: AiBackendDesiredStateModel::Enabled,
            weight_override: Some(10),
            priority_override: Some(5),
            generation: 1,
            created_at_ms: 1,
            updated_at_ms: 1,
        };

        repo.upsert_placement(placement.clone()).await.unwrap();

        assert_eq!(repo.list_placements().await.unwrap(), vec![placement.clone()]);
        assert_eq!(
            repo.list_placements_by_backend("backend-1").await.unwrap(),
            vec![placement.clone()]
        );
        assert_eq!(
            repo.list_placements_by_node("node-1").await.unwrap(),
            vec![placement]
        );
    }

    #[tokio::test]
    async fn stores_and_indexes_statuses() {
        let repo = KvAiBackendRepository::new(Arc::new(MemoryKvStore::new()));
        let status = AiBackendNodeStatusRecordModel {
            backend_id: "backend-1".to_string(),
            node_uuid: "node-1".to_string(),
            observed_generation: 2,
            status: AiBackendNodeRuntimeStatusModel::Ready,
            status_reason: String::new(),
            runtime_backend_name: "backend-backend-1".to_string(),
            endpoint: "https://example.com".to_string(),
            available: true,
            operations: vec!["chat".to_string()],
            features: vec!["stream".to_string()],
            transports: vec!["http".to_string()],
            last_heartbeat_at_ms: 2,
        };

        repo.upsert_node_status(status.clone()).await.unwrap();

        assert_eq!(
            repo.list_node_statuses_by_backend("backend-1").await.unwrap(),
            vec![status.clone()]
        );
        assert_eq!(
            repo.get_node_status("backend-1", "node-1").await.unwrap(),
            Some(status)
        );
    }

    #[tokio::test]
    async fn stores_backend_records() {
        let repo = KvAiBackendRepository::new(Arc::new(MemoryKvStore::new()));
        let backend = sample_backend();

        repo.upsert_backend(backend.clone()).await.unwrap();

        assert_eq!(repo.get_backend("backend-1").await.unwrap(), Some(backend));
        assert_eq!(repo.list_backends().await.unwrap().len(), 1);
    }
}
