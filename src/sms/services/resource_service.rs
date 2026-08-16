//! Resource management handler for SPEAR Metadata Server / SPEAR元数据服务器的资源管理处理器
//!
//! This module provides the ResourceHandler which implements all resource monitoring
//! and management operations for nodes in the cluster.
//!
//! 此模块提供ResourceHandler，实现集群中节点的所有资源监控和管理操作。

use crate::sms::services::error::SmsError;
use crate::storage::{serialization, KvStore, MemoryKvStore};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

/// Resource namespace constants / 资源命名空间常量
mod namespace {
    pub const RESOURCE_PREFIX: &str = "resource:";
}

/// Key generation utilities / 键生成工具
mod keys {
    use super::namespace;
    use uuid::Uuid;

    /// Generate resource key / 生成资源键
    pub fn resource_key(uuid: &Uuid) -> String {
        format!("{}{}", namespace::RESOURCE_PREFIX, uuid)
    }
}

/// Node resource information / 节点资源信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeResourceInfo {
    /// Node unique identifier / 节点唯一标识符
    pub node_uuid: Uuid,
    /// CPU usage percentage (0-100) / CPU使用率百分比 (0-100)
    pub cpu_usage_percent: f64,
    /// Memory usage percentage (0-100) / 内存使用率百分比 (0-100)
    pub memory_usage_percent: f64,
    /// Total memory in bytes / 总内存字节数
    pub total_memory_bytes: i64,
    /// Used memory in bytes / 已使用内存字节数
    pub used_memory_bytes: i64,
    /// Available memory in bytes / 可用内存字节数
    pub available_memory_bytes: i64,
    /// Disk usage percentage (0-100) / 磁盘使用率百分比 (0-100)
    pub disk_usage_percent: f64,
    /// Total disk space in bytes / 总磁盘空间字节数
    pub total_disk_bytes: i64,
    /// Used disk space in bytes / 已使用磁盘空间字节数
    pub used_disk_bytes: i64,
    /// Network receive bytes per second / 网络接收字节数每秒
    pub network_rx_bytes_per_sec: i64,
    /// Network transmit bytes per second / 网络发送字节数每秒
    pub network_tx_bytes_per_sec: i64,
    /// Load average (1 minute) / 负载平均值 (1分钟)
    pub load_average_1m: f64,
    /// Load average (5 minutes) / 负载平均值 (5分钟)
    pub load_average_5m: f64,
    /// Load average (15 minutes) / 负载平均值 (15分钟)
    pub load_average_15m: f64,
    /// Resource update timestamp / 资源更新时间戳
    pub updated_at: DateTime<Utc>,
    /// Additional resource metadata / 额外资源元数据
    pub resource_metadata: HashMap<String, String>,
}

impl NodeResourceInfo {
    /// Create new node resource info / 创建新的节点资源信息
    pub fn new(node_uuid: Uuid) -> Self {
        Self {
            node_uuid,
            cpu_usage_percent: 0.0,
            memory_usage_percent: 0.0,
            total_memory_bytes: 0,
            used_memory_bytes: 0,
            available_memory_bytes: 0,
            disk_usage_percent: 0.0,
            total_disk_bytes: 0,
            used_disk_bytes: 0,
            network_rx_bytes_per_sec: 0,
            network_tx_bytes_per_sec: 0,
            load_average_1m: 0.0,
            load_average_5m: 0.0,
            load_average_15m: 0.0,
            updated_at: Utc::now(),
            resource_metadata: HashMap::new(),
        }
    }

    /// Update timestamp / 更新时间戳
    pub fn update_timestamp(&mut self) {
        self.updated_at = Utc::now();
    }

    /// Check if resource info is stale / 检查资源信息是否过期
    pub fn is_stale(&self, max_age_seconds: u64) -> bool {
        let now = Utc::now();
        let max_age = chrono::Duration::seconds(max_age_seconds as i64);
        now.signed_duration_since(self.updated_at) > max_age
    }

    /// Update resource metadata / 更新资源元数据
    pub fn update_metadata(&mut self, key: String, value: String) {
        self.resource_metadata.insert(key, value);
    }

    /// Get memory usage in bytes / 获取内存使用量（字节）
    pub fn get_memory_usage_bytes(&self) -> i64 {
        self.used_memory_bytes
    }

    /// Get available disk space in bytes / 获取可用磁盘空间（字节）
    pub fn get_available_disk_bytes(&self) -> i64 {
        self.total_disk_bytes - self.used_disk_bytes
    }

    /// Check if node is under high load / 检查节点是否处于高负载状态
    pub fn is_high_load(&self) -> bool {
        self.cpu_usage_percent > 80.0
            || self.memory_usage_percent > 85.0
            || self.load_average_1m > 4.0
    }
}

/// Resource management service / 资源管理服务
#[derive(Debug)]
pub struct ResourceService {
    kv_store: Arc<dyn KvStore>,
}

impl Default for ResourceService {
    fn default() -> Self {
        Self::new()
    }
}

impl ResourceService {
    /// Create a new resource service with memory backend / 创建使用内存后端的新资源服务
    pub fn new() -> Self {
        Self::new_with_memory()
    }

    /// Create a new resource service with memory backend / 创建使用内存后端的新资源服务
    pub fn new_with_memory() -> Self {
        let kv_store = Arc::new(MemoryKvStore::new());
        Self { kv_store }
    }

    /// Create a new resource service with RocksDB backend / 创建使用RocksDB后端的新资源服务
    #[cfg(feature = "rocksdb")]
    pub fn new_with_rocksdb(db_path: &str) -> Result<Self, SmsError> {
        let kv_store = crate::storage::create_kv_store(crate::storage::KvStoreType::RocksDb {
            path: db_path.to_string(),
        })?;
        Ok(Self {
            kv_store: Arc::from(kv_store),
        })
    }

    /// Create a new resource service with Sled backend / 创建使用Sled后端的新资源服务
    #[cfg(feature = "sled")]
    pub fn new_with_sled(db_path: &str) -> Result<Self, SmsError> {
        let kv_store = crate::storage::create_kv_store(crate::storage::KvStoreType::Sled {
            path: db_path.to_string(),
        })?;
        Ok(Self {
            kv_store: Arc::from(kv_store),
        })
    }

    /// Create a new resource service with custom KV store / 创建使用自定义KV存储的新资源服务
    pub fn with_kv_store(kv_store: Arc<dyn KvStore>) -> Self {
        Self { kv_store }
    }

    /// Update resource information / 更新资源信息
    pub async fn update_resource(&self, mut resource: NodeResourceInfo) -> Result<(), SmsError> {
        resource.update_timestamp();
        self.store_resource_direct(resource).await
    }

    /// Store resource directly without timestamp update / 直接存储资源而不更新时间戳
    async fn store_resource_direct(&self, resource: NodeResourceInfo) -> Result<(), SmsError> {
        let key = keys::resource_key(&resource.node_uuid);
        let serialized = serialization::serialize(&resource)?;
        self.kv_store.put(&key, &serialized).await?;
        Ok(())
    }

    /// Get resource information for a node / 获取节点的资源信息
    pub async fn get_resource(
        &self,
        node_uuid: &Uuid,
    ) -> Result<Option<NodeResourceInfo>, SmsError> {
        let key = keys::resource_key(node_uuid);

        if let Some(data) = self.kv_store.get(&key).await? {
            let resource: NodeResourceInfo = serialization::deserialize(&data)?;
            Ok(Some(resource))
        } else {
            Ok(None)
        }
    }

    /// Remove resource information for a node / 移除节点的资源信息
    /// Returns the removed resource if it existed / 如果存在则返回被移除的资源
    pub async fn remove_resource(
        &self,
        node_uuid: &Uuid,
    ) -> Result<Option<NodeResourceInfo>, SmsError> {
        let key = keys::resource_key(node_uuid);

        // Get the resource before removing / 移除前获取资源
        let resource = if let Some(data) = self.kv_store.get(&key).await? {
            let resource: NodeResourceInfo = serialization::deserialize(&data)?;
            Some(resource)
        } else {
            None
        };

        // Remove the resource / 移除资源
        self.kv_store.delete(&key).await?;

        Ok(resource)
    }

    /// List all resource information / 列出所有资源信息
    pub async fn list_resources(&self) -> Result<Vec<NodeResourceInfo>, SmsError> {
        let keys = self
            .kv_store
            .keys_with_prefix(namespace::RESOURCE_PREFIX)
            .await?;
        let mut resources = Vec::new();

        for key in keys {
            if let Some(data) = self.kv_store.get(&key).await? {
                let resource: NodeResourceInfo = serialization::deserialize(&data)?;
                resources.push(resource);
            }
        }

        Ok(resources)
    }

    /// List resources for specific nodes / 列出特定节点的资源
    pub async fn list_resources_by_nodes(
        &self,
        node_uuids: &[Uuid],
    ) -> Result<Vec<NodeResourceInfo>, SmsError> {
        let mut resources = Vec::new();

        for uuid in node_uuids {
            if let Some(resource) = self.get_resource(uuid).await? {
                resources.push(resource);
            }
        }

        Ok(resources)
    }

    /// List nodes with high load / 列出高负载节点
    pub async fn list_high_load_nodes(&self) -> Result<Vec<NodeResourceInfo>, SmsError> {
        let all_resources = self.list_resources().await?;
        Ok(all_resources
            .into_iter()
            .filter(|resource| resource.is_high_load())
            .collect())
    }

    /// Cleanup stale resource information / 清理过期的资源信息
    pub async fn cleanup_stale_resources(
        &self,
        max_age_seconds: u64,
    ) -> Result<Vec<Uuid>, SmsError> {
        let all_resources = self.list_resources().await?;
        let mut removed_uuids = Vec::new();

        for resource in all_resources {
            if resource.is_stale(max_age_seconds) {
                self.remove_resource(&resource.node_uuid).await?;
                removed_uuids.push(resource.node_uuid);
            }
        }

        Ok(removed_uuids)
    }

    /// Get resource count / 获取资源数量
    pub async fn resource_count(&self) -> Result<usize, SmsError> {
        let keys = self
            .kv_store
            .keys_with_prefix(namespace::RESOURCE_PREFIX)
            .await?;
        Ok(keys.len())
    }

    /// Check if resource registry is empty / 检查资源注册表是否为空
    pub async fn is_empty(&self) -> Result<bool, SmsError> {
        Ok(self.resource_count().await? == 0)
    }

    /// Helper function to calculate average of a field across all resources / 计算所有资源中某个字段的平均值的辅助函数
    async fn calculate_average_field<F>(&self, field_extractor: F) -> Result<f64, SmsError>
    where
        F: Fn(&NodeResourceInfo) -> f64,
    {
        let resources = self.list_resources().await?;
        if resources.is_empty() {
            return Ok(0.0);
        }

        let total: f64 = resources.iter().map(field_extractor).sum();
        Ok(total / resources.len() as f64)
    }

    /// Get average CPU usage across all nodes / 获取所有节点的平均CPU使用率
    pub async fn get_average_cpu_usage(&self) -> Result<f64, SmsError> {
        self.calculate_average_field(|r| r.cpu_usage_percent).await
    }

    /// Get average memory usage across all nodes / 获取所有节点的平均内存使用率
    pub async fn get_average_memory_usage(&self) -> Result<f64, SmsError> {
        self.calculate_average_field(|r| r.memory_usage_percent)
            .await
    }

    /// Get total memory across all nodes / 获取所有节点的总内存
    pub async fn get_total_memory_bytes(&self) -> Result<i64, SmsError> {
        let resources = self.list_resources().await?;
        Ok(resources.iter().map(|r| r.total_memory_bytes).sum())
    }

    /// Get total used memory across all nodes / 获取所有节点的总已用内存
    pub async fn get_total_used_memory_bytes(&self) -> Result<i64, SmsError> {
        let resources = self.list_resources().await?;
        Ok(resources.iter().map(|r| r.used_memory_bytes).sum())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_resource_info_creation() {
        let uuid = Uuid::new_v4();
        let resource = NodeResourceInfo::new(uuid);
        assert_eq!(resource.node_uuid, uuid);
        assert_eq!(resource.cpu_usage_percent, 0.0);
        assert_eq!(resource.memory_usage_percent, 0.0);
        assert!(resource.resource_metadata.is_empty());
    }

    #[tokio::test]
    async fn test_resource_service_operations() {
        let service = ResourceService::new();
        let uuid = Uuid::new_v4();

        // Test create / 测试创建
        let resource = NodeResourceInfo::new(uuid);
        let result = service.update_resource(resource.clone()).await;
        assert!(result.is_ok());

        // Test get / 测试获取
        let retrieved = service.get_resource(&uuid).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().node_uuid, uuid);

        // Test update / 测试更新
        let mut updated_resource = NodeResourceInfo::new(uuid);
        updated_resource.cpu_usage_percent = 75.0;
        updated_resource.memory_usage_percent = 60.0;

        let result = service.update_resource(updated_resource).await;
        assert!(result.is_ok());

        let retrieved = service.get_resource(&uuid).await.unwrap().unwrap();
        assert_eq!(retrieved.cpu_usage_percent, 75.0);
        assert_eq!(retrieved.memory_usage_percent, 60.0);

        // Test remove / 测试移除
        let removed = service.remove_resource(&uuid).await.unwrap();
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().node_uuid, uuid);

        // Verify removal / 验证移除
        let retrieved = service.get_resource(&uuid).await.unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_high_load_detection() {
        let uuid = Uuid::new_v4();

        // Normal load / 正常负载
        let mut resource = NodeResourceInfo::new(uuid);
        resource.cpu_usage_percent = 50.0;
        resource.memory_usage_percent = 60.0;
        resource.load_average_1m = 2.0;
        assert!(!resource.is_high_load());

        // High CPU / 高CPU
        resource.cpu_usage_percent = 85.0;
        assert!(resource.is_high_load());

        // High memory / 高内存
        resource.cpu_usage_percent = 50.0;
        resource.memory_usage_percent = 90.0;
        assert!(resource.is_high_load());

        // High load average / 高负载平均值
        resource.memory_usage_percent = 60.0;
        resource.load_average_1m = 5.0;
        assert!(resource.is_high_load());
    }

    #[tokio::test]
    async fn test_resource_service_list_operations() {
        let service = ResourceService::new();

        let uuid1 = Uuid::new_v4();
        let uuid2 = Uuid::new_v4();
        let uuid3 = Uuid::new_v4();

        // Add resources / 添加资源
        let mut resource1 = NodeResourceInfo::new(uuid1);
        resource1.cpu_usage_percent = 90.0; // High load / 高负载

        let mut resource2 = NodeResourceInfo::new(uuid2);
        resource2.cpu_usage_percent = 30.0; // Normal load / 正常负载

        let mut resource3 = NodeResourceInfo::new(uuid3);
        resource3.memory_usage_percent = 95.0; // High memory usage / 高内存使用率

        service.update_resource(resource1).await.unwrap();
        service.update_resource(resource2).await.unwrap();
        service.update_resource(resource3).await.unwrap();

        // Test list all / 测试列出所有
        let all_resources = service.list_resources().await.unwrap();
        assert_eq!(all_resources.len(), 3);

        // Test list by nodes / 测试按节点列出
        let specific_resources = service
            .list_resources_by_nodes(&[uuid1, uuid2])
            .await
            .unwrap();
        assert_eq!(specific_resources.len(), 2);

        // Test list high load nodes / 测试列出高负载节点
        let high_load_nodes = service.list_high_load_nodes().await.unwrap();
        assert_eq!(high_load_nodes.len(), 2); // uuid1 (high CPU) and uuid3 (high memory) / uuid1（高CPU）和uuid3（高内存）
    }

    #[tokio::test]
    async fn test_resource_service_statistics() {
        let service = ResourceService::new();

        let uuid1 = Uuid::new_v4();
        let uuid2 = Uuid::new_v4();

        let mut resource1 = NodeResourceInfo::new(uuid1);
        resource1.cpu_usage_percent = 60.0;
        resource1.memory_usage_percent = 70.0;
        resource1.total_memory_bytes = 8_000_000_000; // 8GB
        resource1.used_memory_bytes = 5_600_000_000; // 5.6GB

        let mut resource2 = NodeResourceInfo::new(uuid2);
        resource2.cpu_usage_percent = 40.0;
        resource2.memory_usage_percent = 50.0;
        resource2.total_memory_bytes = 16_000_000_000; // 16GB
        resource2.used_memory_bytes = 8_000_000_000; // 8GB

        service.update_resource(resource1).await.unwrap();
        service.update_resource(resource2).await.unwrap();

        // Test statistics / 测试统计
        let avg_cpu = service.get_average_cpu_usage().await.unwrap();
        let avg_memory = service.get_average_memory_usage().await.unwrap();
        let total_memory = service.get_total_memory_bytes().await.unwrap();
        let used_memory = service.get_total_used_memory_bytes().await.unwrap();

        assert_eq!(avg_cpu, 50.0); // (60 + 40) / 2
        assert_eq!(avg_memory, 60.0); // (70 + 50) / 2
        assert_eq!(total_memory, 24_000_000_000); // 8GB + 16GB
        assert_eq!(used_memory, 13_600_000_000); // 5.6GB + 8GB
    }

    #[tokio::test]
    async fn test_resource_service_cleanup() {
        let service = ResourceService::new();

        let uuid1 = Uuid::new_v4();
        let uuid2 = Uuid::new_v4();

        let resource1 = NodeResourceInfo::new(uuid1);
        let mut resource2 = NodeResourceInfo::new(uuid2);

        // Make resource2 stale / 让resource2过期
        resource2.updated_at = Utc::now() - chrono::Duration::seconds(120);

        service.store_resource_direct(resource1).await.unwrap();
        service.store_resource_direct(resource2).await.unwrap();

        assert_eq!(service.resource_count().await.unwrap(), 2);

        // Cleanup stale resources / 清理过期资源
        let removed = service.cleanup_stale_resources(60).await.unwrap();

        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0], uuid2);
        assert_eq!(service.resource_count().await.unwrap(), 1);

        // Verify only fresh resource remains / 验证只有新鲜资源保留
        assert!(service.get_resource(&uuid1).await.unwrap().is_some());
        assert!(service.get_resource(&uuid2).await.unwrap().is_none());
    }
}
