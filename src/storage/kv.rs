//! Key-Value storage abstraction layer for SPEAR Metadata Server
//! SPEAR元数据服务器的键值存储抽象层
//!
//! This module provides a generic interface for different KV storage backends,
//! supporting basic CRUD operations and range queries.
//! 该模块为不同的KV存储后端提供通用接口，支持基本的CRUD操作和范围查询。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Debug;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::config::base::StorageConfig;
use crate::sms::services::error::SmsError;

#[cfg(feature = "sled")]
use crate::{handle_sled_error, spawn_blocking_task};

/// Key type for KV operations / KV操作的键类型
pub type KvKey = String;

/// Value type for KV operations / KV操作的值类型
pub type KvValue = Vec<u8>;

/// Range query result / 范围查询结果
#[derive(Debug, Clone, PartialEq)]
pub struct KvPair {
    pub key: KvKey,
    pub value: KvValue,
}

/// Range query options / 范围查询选项
#[derive(Debug, Clone, Default)]
pub struct RangeOptions {
    /// Start key (inclusive) / 起始键（包含）
    pub start_key: Option<KvKey>,
    /// End key (exclusive) / 结束键（不包含）
    pub end_key: Option<KvKey>,
    /// Maximum number of results / 最大结果数量
    pub limit: Option<usize>,
    /// Reverse order / 逆序
    pub reverse: bool,
}

impl RangeOptions {
    /// Create a new range options / 创建新的范围选项
    pub fn new() -> Self {
        Self::default()
    }

    /// Set start key / 设置起始键
    pub fn start_key(mut self, key: impl Into<KvKey>) -> Self {
        self.start_key = Some(key.into());
        self
    }

    /// Set end key / 设置结束键
    pub fn end_key(mut self, key: impl Into<KvKey>) -> Self {
        self.end_key = Some(key.into());
        self
    }

    /// Set limit / 设置限制
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Set reverse order / 设置逆序
    pub fn reverse(mut self, reverse: bool) -> Self {
        self.reverse = reverse;
        self
    }
}

/// KV storage configuration / KV存储配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KvStoreConfig {
    /// Storage backend type / 存储后端类型
    pub backend: String,
    /// Configuration parameters / 配置参数
    pub params: HashMap<String, String>,
}

impl KvStoreConfig {
    /// Create a new memory store config / 创建新的内存存储配置
    pub fn memory() -> Self {
        Self {
            backend: "memory".to_string(),
            params: HashMap::new(),
        }
    }

    /// Create evmap configuration / 创建 evmap 配置
    #[cfg(feature = "evmap")]
    pub fn evmap() -> Self {
        Self {
            backend: "evmap".to_string(),
            params: HashMap::new(),
        }
    }

    /// Create a new Sled store config / 创建新的Sled存储配置
    #[cfg(feature = "sled")]
    pub fn sled<P: AsRef<str>>(path: P) -> Self {
        let mut params = HashMap::new();
        params.insert("path".to_string(), path.as_ref().to_string());
        Self {
            backend: "sled".to_string(),
            params,
        }
    }

    /// Create a new RocksDB store config / 创建新的RocksDB存储配置
    #[cfg(feature = "rocksdb")]
    pub fn rocksdb<P: AsRef<str>>(path: P) -> Self {
        let mut params = HashMap::new();
        params.insert("path".to_string(), path.as_ref().to_string());
        Self {
            backend: "rocksdb".to_string(),
            params,
        }
    }

    /// Add a configuration parameter / 添加配置参数
    pub fn with_param<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.params.insert(key.into(), value.into());
        self
    }

    /// Get a configuration parameter / 获取配置参数
    pub fn get_param(&self, key: &str) -> Option<&String> {
        self.params.get(key)
    }

    /// Load configuration from environment variables / 从环境变量加载配置
    /// Environment variables:
    /// - KV_STORE_BACKEND or SPEAR_KV_BACKEND: Backend type (memory, sled)
    /// - KV_STORE_*: Generic parameters (converted to lowercase)
    /// - SPEAR_KV_SLED_PATH: Path for sled database (legacy)
    pub fn from_env() -> Result<Self, SmsError> {
        let backend = std::env::var("KV_STORE_BACKEND")
            .or_else(|_| std::env::var("SPEAR_KV_BACKEND"))
            .unwrap_or_else(|_| "memory".to_string());

        let mut params = HashMap::new();

        // Load generic KV_STORE_* parameters / 加载通用KV_STORE_*参数
        for (key, value) in std::env::vars() {
            if let Some(param_name) = key.strip_prefix("KV_STORE_") {
                if param_name != "BACKEND" {
                    params.insert(param_name.to_lowercase(), value);
                }
            }
        }

        // Load backend-specific parameters / 加载后端特定参数
        match backend.as_str() {
            "sled" => {
                // Check for legacy environment variable / 检查遗留环境变量
                if let Ok(path) = std::env::var("SPEAR_KV_SLED_PATH") {
                    params.insert("path".to_string(), path);
                } else if !params.contains_key("path") {
                    params.insert("path".to_string(), "./data/kv.db".to_string());
                }
            }
            "memory" => {
                // Memory store can use generic parameters / 内存存储可以使用通用参数
            }
            _ => {
                return Err(SmsError::Serialization(format!(
                    "Unsupported KV backend: {}",
                    backend
                )));
            }
        }

        Ok(Self { backend, params })
    }

    /// Convert from StorageConfig to KvStoreConfig / 从StorageConfig转换为KvStoreConfig
    pub fn from_storage_config(storage_config: &StorageConfig) -> Self {
        let mut params = HashMap::new();

        // Add data directory parameter / 添加数据目录参数
        params.insert("path".to_string(), storage_config.data_dir.clone());

        // Add max cache size parameter / 添加最大缓存大小参数
        params.insert(
            "max_cache_size_mb".to_string(),
            storage_config.max_cache_size_mb.to_string(),
        );

        // Add compression parameter / 添加压缩参数
        params.insert(
            "compression_enabled".to_string(),
            storage_config.compression_enabled.to_string(),
        );

        // Add pool size parameter / 添加连接池大小参数
        params.insert(
            "pool_size".to_string(),
            storage_config.pool_size.to_string(),
        );

        Self {
            backend: storage_config.backend.clone(),
            params,
        }
    }
}

impl Default for KvStoreConfig {
    fn default() -> Self {
        Self::memory()
    }
}

/// KV storage trait for different backends / 不同后端的KV存储trait
#[async_trait]
pub trait KvStore: Send + Sync + Debug {
    /// Get a value by key / 根据键获取值
    async fn get(&self, key: &KvKey) -> Result<Option<KvValue>, SmsError>;

    /// Put a key-value pair / 存储键值对
    async fn put(&self, key: &KvKey, value: &KvValue) -> Result<(), SmsError>;

    /// Delete a key / 删除键
    async fn delete(&self, key: &KvKey) -> Result<bool, SmsError>;

    /// Check if a key exists / 检查键是否存在
    async fn exists(&self, key: &KvKey) -> Result<bool, SmsError>;

    /// Get all keys with a prefix / 获取具有指定前缀的所有键
    async fn keys_with_prefix(&self, prefix: &str) -> Result<Vec<KvKey>, SmsError>;

    /// Scan all key-value pairs with a prefix / 扫描具有指定前缀的所有键值对
    async fn scan_prefix(&self, prefix: &str) -> Result<Vec<KvPair>, SmsError>;

    /// Range query / 范围查询
    async fn range(&self, options: &RangeOptions) -> Result<Vec<KvPair>, SmsError>;

    /// Get all key-value pairs / 获取所有键值对
    async fn all(&self) -> Result<Vec<KvPair>, SmsError> {
        self.range(&RangeOptions::new()).await
    }

    /// Count total number of keys / 统计键的总数
    async fn count(&self) -> Result<usize, SmsError>;

    /// Clear all data / 清空所有数据
    async fn clear(&self) -> Result<(), SmsError>;

    /// Batch operations / 批量操作
    async fn batch_put(&self, pairs: &[KvPair]) -> Result<(), SmsError> {
        for pair in pairs {
            self.put(&pair.key, &pair.value).await?;
        }
        Ok(())
    }

    /// Batch delete / 批量删除
    async fn batch_delete(&self, keys: &[KvKey]) -> Result<usize, SmsError> {
        let mut deleted = 0;
        for key in keys {
            if self.delete(key).await? {
                deleted += 1;
            }
        }
        Ok(deleted)
    }
}

/// KV store factory trait / KV存储工厂trait
#[async_trait]
pub trait KvStoreFactory: Send + Sync + Debug {
    /// Create a KV store instance from configuration / 从配置创建KV存储实例
    async fn create(&self, config: &KvStoreConfig) -> Result<Box<dyn KvStore>, SmsError>;

    /// Get supported backend types / 获取支持的后端类型
    fn supported_backends(&self) -> Vec<String>;

    /// Validate configuration / 验证配置
    fn validate_config(&self, config: &KvStoreConfig) -> Result<(), SmsError>;
}

/// Default KV store factory implementation / 默认KV存储工厂实现
#[derive(Debug, Default)]
pub struct DefaultKvStoreFactory;

impl DefaultKvStoreFactory {
    /// Create a new factory instance / 创建新的工厂实例
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl KvStoreFactory for DefaultKvStoreFactory {
    async fn create(&self, config: &KvStoreConfig) -> Result<Box<dyn KvStore>, SmsError> {
        self.validate_config(config)?;

        match config.backend.as_str() {
            "memory" => Ok(Box::new(MemoryKvStore::new())),
            #[cfg(feature = "evmap")]
            "evmap" => Ok(Box::new(EvmapKvStore::new())),
            #[cfg(feature = "sled")]
            "sled" => {
                let path = config.get_param("path").ok_or_else(|| {
                    SmsError::Serialization("Sled backend requires 'path' parameter".to_string())
                })?;
                let store = SledKvStore::new(path)?;
                Ok(Box::new(store))
            }
            #[cfg(feature = "rocksdb")]
            "rocksdb" => {
                let path = config.get_param("path").ok_or_else(|| {
                    SmsError::Serialization("RocksDB backend requires 'path' parameter".to_string())
                })?;
                let store = RocksDbKvStore::new(path)?;
                Ok(Box::new(store))
            }
            _ => Err(SmsError::Serialization(format!(
                "Unsupported backend: {}",
                config.backend
            ))),
        }
    }

    fn supported_backends(&self) -> Vec<String> {
        #[cfg(any(feature = "sled", feature = "rocksdb", feature = "evmap"))]
        let mut backends = vec!["memory".to_string()];

        #[cfg(not(any(feature = "sled", feature = "rocksdb", feature = "evmap")))]
        let backends = vec!["memory".to_string()];

        #[cfg(feature = "evmap")]
        backends.push("evmap".to_string());

        #[cfg(feature = "sled")]
        backends.push("sled".to_string());

        #[cfg(feature = "rocksdb")]
        backends.push("rocksdb".to_string());

        backends
    }

    fn validate_config(&self, config: &KvStoreConfig) -> Result<(), SmsError> {
        if !self.supported_backends().contains(&config.backend) {
            return Err(SmsError::Serialization(format!(
                "Unsupported backend: {}. Supported backends: {:?}",
                config.backend,
                self.supported_backends()
            )));
        }

        match config.backend.as_str() {
            "sled" => {
                if config.get_param("path").is_none() {
                    return Err(SmsError::Serialization(
                        "Sled backend requires 'path' parameter".to_string(),
                    ));
                }
            }
            "rocksdb" => {
                if config.get_param("path").is_none() {
                    return Err(SmsError::Serialization(
                        "RocksDB backend requires 'path' parameter".to_string(),
                    ));
                }
            }
            _ => {} // Memory store doesn't need validation / 内存存储不需要验证
        }

        Ok(())
    }
}

/// Global factory instance / 全局工厂实例
static FACTORY: std::sync::OnceLock<Box<dyn KvStoreFactory>> = std::sync::OnceLock::new();

/// Set the global KV store factory / 设置全局KV存储工厂
pub fn set_kv_store_factory(factory: Box<dyn KvStoreFactory>) -> Result<(), SmsError> {
    FACTORY
        .set(factory)
        .map_err(|_| SmsError::Serialization("KV store factory already initialized".to_string()))
}

/// Get the global KV store factory / 获取全局KV存储工厂
pub fn get_kv_store_factory() -> &'static dyn KvStoreFactory {
    FACTORY
        .get_or_init(|| Box::new(DefaultKvStoreFactory::new()))
        .as_ref()
}

/// Create a KV store instance using the global factory / 使用全局工厂创建KV存储实例
pub async fn create_kv_store_from_config(
    config: &KvStoreConfig,
) -> Result<Box<dyn KvStore>, SmsError> {
    get_kv_store_factory().create(config).await
}

/// Create a KV store instance from environment / 从环境变量创建KV存储实例
pub async fn create_kv_store_from_env() -> Result<Box<dyn KvStore>, SmsError> {
    let config = KvStoreConfig::from_env()?;
    create_kv_store_from_config(&config).await
}

/// In-memory KV store implementation / 内存KV存储实现
#[derive(Debug)]
pub struct MemoryKvStore {
    data: Arc<RwLock<BTreeMap<KvKey, KvValue>>>,
}

impl MemoryKvStore {
    /// Create a new memory KV store / 创建新的内存KV存储
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }
}

impl Default for MemoryKvStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl KvStore for MemoryKvStore {
    async fn get(&self, key: &KvKey) -> Result<Option<KvValue>, SmsError> {
        let data = self.data.read().await;
        Ok(data.get(key).cloned())
    }

    async fn put(&self, key: &KvKey, value: &KvValue) -> Result<(), SmsError> {
        let mut data = self.data.write().await;
        data.insert(key.clone(), value.clone());
        Ok(())
    }

    async fn delete(&self, key: &KvKey) -> Result<bool, SmsError> {
        let mut data = self.data.write().await;
        Ok(data.remove(key).is_some())
    }

    async fn exists(&self, key: &KvKey) -> Result<bool, SmsError> {
        let data = self.data.read().await;
        Ok(data.contains_key(key))
    }

    async fn keys_with_prefix(&self, prefix: &str) -> Result<Vec<KvKey>, SmsError> {
        let data = self.data.read().await;
        let keys: Vec<KvKey> = data
            .keys()
            .filter(|key| key.starts_with(prefix))
            .cloned()
            .collect();
        Ok(keys)
    }

    async fn scan_prefix(&self, prefix: &str) -> Result<Vec<KvPair>, SmsError> {
        let data = self.data.read().await;
        let pairs: Vec<KvPair> = data
            .iter()
            .filter(|(key, _)| key.starts_with(prefix))
            .map(|(key, value)| KvPair {
                key: key.clone(),
                value: value.clone(),
            })
            .collect();
        Ok(pairs)
    }

    async fn range(&self, options: &RangeOptions) -> Result<Vec<KvPair>, SmsError> {
        let data = self.data.read().await;

        let mut pairs: Vec<KvPair> = data
            .iter()
            .filter(|(key, _)| {
                // Filter by start key / 按起始键过滤
                if let Some(ref start) = options.start_key {
                    if *key < start {
                        return false;
                    }
                }
                // Filter by end key / 按结束键过滤
                if let Some(ref end) = options.end_key {
                    if *key >= end {
                        return false;
                    }
                }
                true
            })
            .map(|(key, value)| KvPair {
                key: key.clone(),
                value: value.clone(),
            })
            .collect();

        // Apply reverse order / 应用逆序
        if options.reverse {
            pairs.reverse();
        }

        // Apply limit / 应用限制
        if let Some(limit) = options.limit {
            pairs.truncate(limit);
        }

        Ok(pairs)
    }

    async fn count(&self) -> Result<usize, SmsError> {
        let data = self.data.read().await;
        Ok(data.len())
    }

    async fn clear(&self) -> Result<(), SmsError> {
        let mut data = self.data.write().await;
        data.clear();
        Ok(())
    }
}

/// Serialization helpers for common types / 常见类型的序列化辅助函数
pub mod serialization {
    use super::*;
    // Explicitly import serde traits to avoid namespace conflicts
    // 明确导入 serde trait 以避免命名空间冲突
    use serde::{Deserialize, Serialize};

    /// Serialize a value to bytes / 将值序列化为字节
    pub fn serialize<T: Serialize>(value: &T) -> Result<KvValue, SmsError> {
        serde_json::to_vec(value)
            .map_err(|e| SmsError::Serialization(format!("Failed to serialize value: {}", e)))
    }

    /// Deserialize bytes to a value / 将字节反序列化为值
    pub fn deserialize<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T, SmsError> {
        serde_json::from_slice(bytes)
            .map_err(|e| SmsError::Serialization(format!("Failed to deserialize value: {}", e)))
    }

    /// Generate key for node info / 生成节点信息的键
    pub fn node_key(uuid: &Uuid) -> KvKey {
        format!("node:{}", uuid)
    }

    /// Generate key for node resource / 生成节点资源的键
    pub fn resource_key(uuid: &Uuid) -> KvKey {
        format!("resource:{}", uuid)
    }

    /// Generate key prefix for nodes / 生成节点的键前缀
    pub fn node_prefix() -> &'static str {
        "node:"
    }

    /// Generate key prefix for resources / 生成资源的键前缀
    pub fn resource_prefix() -> &'static str {
        "resource:"
    }

    /// Extract UUID from node key / 从节点键中提取UUID
    pub fn extract_uuid_from_node_key(key: &str) -> Result<Uuid, SmsError> {
        if let Some(uuid_str) = key.strip_prefix("node:") {
            Uuid::parse_str(uuid_str)
                .map_err(|e| SmsError::Serialization(format!("Invalid UUID in node key: {}", e)))
        } else {
            Err(SmsError::Serialization(
                "Invalid node key format".to_string(),
            ))
        }
    }

    /// Extract UUID from resource key / 从资源键中提取UUID
    pub fn extract_uuid_from_resource_key(key: &str) -> Result<Uuid, SmsError> {
        if let Some(uuid_str) = key.strip_prefix("resource:") {
            Uuid::parse_str(uuid_str).map_err(|e| {
                SmsError::Serialization(format!("Invalid UUID in resource key: {}", e))
            })
        } else {
            Err(SmsError::Serialization(
                "Invalid resource key format".to_string(),
            ))
        }
    }
}

/// KV store factory for creating different backends / 用于创建不同后端的KV存储工厂
#[derive(Debug, Clone)]
pub enum KvStoreType {
    Memory,
    #[cfg(feature = "sled")]
    Sled {
        path: String,
    },
    #[cfg(feature = "rocksdb")]
    RocksDb {
        path: String,
    },
    #[cfg(feature = "evmap")]
    Evmap,
}

/// Sled KV store implementation / Sled KV存储实现
#[cfg(feature = "sled")]
#[derive(Debug)]
pub struct SledKvStore {
    db: Arc<sled::Db>,
}

#[cfg(feature = "sled")]
impl SledKvStore {
    /// Create a new Sled KV store / 创建新的Sled KV存储
    pub fn new(path: impl AsRef<std::path::Path>) -> Result<Self, SmsError> {
        let db = sled::open(path)
            .map_err(|e| SmsError::Serialization(format!("Failed to open Sled: {}", e)))?;

        Ok(Self { db: Arc::new(db) })
    }
}

#[cfg(feature = "sled")]
#[async_trait]
impl KvStore for SledKvStore {
    async fn get(&self, key: &KvKey) -> Result<Option<KvValue>, SmsError> {
        let db = self.db.clone();
        let key = key.clone();

        spawn_blocking_task!(move || {
            handle_sled_error!(db.get(&key).map(|opt| opt.map(|ivec| ivec.to_vec())), "get")
        })
    }

    async fn put(&self, key: &KvKey, value: &KvValue) -> Result<(), SmsError> {
        let db = self.db.clone();
        let key = key.clone();
        let value = value.clone();

        spawn_blocking_task!(move || {
            handle_sled_error!(db.insert(&key, value).map(|_| ()), "put")
        })
    }

    async fn delete(&self, key: &KvKey) -> Result<bool, SmsError> {
        let db = self.db.clone();
        let key = key.clone();

        tokio::task::spawn_blocking(move || {
            db.remove(&key)
                .map(|opt| opt.is_some())
                .map_err(|e| SmsError::Serialization(format!("Sled delete error: {}", e)))
        })
        .await
        .map_err(|e| SmsError::Serialization(format!("Task join error: {}", e)))?
    }

    async fn exists(&self, key: &KvKey) -> Result<bool, SmsError> {
        let db = self.db.clone();
        let key = key.clone();

        tokio::task::spawn_blocking(move || {
            db.contains_key(&key)
                .map_err(|e| SmsError::Serialization(format!("Sled exists error: {}", e)))
        })
        .await
        .map_err(|e| SmsError::Serialization(format!("Task join error: {}", e)))?
    }

    async fn keys_with_prefix(&self, prefix: &str) -> Result<Vec<KvKey>, SmsError> {
        let db = self.db.clone();
        let prefix = prefix.to_string();

        tokio::task::spawn_blocking(move || {
            let mut keys = Vec::new();

            for item in db.scan_prefix(&prefix) {
                match item {
                    Ok((key, _)) => {
                        if let Ok(key_str) = String::from_utf8(key.to_vec()) {
                            keys.push(key_str);
                        }
                    }
                    Err(e) => {
                        return Err(SmsError::Serialization(format!(
                            "Sled iterator error: {}",
                            e
                        )));
                    }
                }
            }

            Ok(keys)
        })
        .await
        .map_err(|e| SmsError::Serialization(format!("Task join error: {}", e)))?
    }

    async fn scan_prefix(&self, prefix: &str) -> Result<Vec<KvPair>, SmsError> {
        let db = self.db.clone();
        let prefix = prefix.to_string();

        tokio::task::spawn_blocking(move || {
            let mut pairs = Vec::new();

            for item in db.scan_prefix(&prefix) {
                match item {
                    Ok((key, value)) => {
                        if let Ok(key_str) = String::from_utf8(key.to_vec()) {
                            pairs.push(KvPair {
                                key: key_str,
                                value: value.to_vec(),
                            });
                        }
                    }
                    Err(e) => {
                        return Err(SmsError::Serialization(format!(
                            "Sled iterator error: {}",
                            e
                        )));
                    }
                }
            }

            Ok(pairs)
        })
        .await
        .map_err(|e| SmsError::Serialization(format!("Task join error: {}", e)))?
    }

    async fn range(&self, options: &RangeOptions) -> Result<Vec<KvPair>, SmsError> {
        let db = self.db.clone();
        let options = options.clone();

        tokio::task::spawn_blocking(move || {
            let mut pairs = Vec::new();

            let iter = if let Some(start_key) = &options.start_key {
                db.range::<&[u8], _>(start_key.as_bytes()..)
            } else {
                db.range::<&[u8], _>(..)
            };

            for item in iter {
                match item {
                    Ok((key, value)) => {
                        let key_str = String::from_utf8(key.to_vec()).map_err(|e| {
                            SmsError::Serialization(format!("Invalid UTF-8 key: {}", e))
                        })?;

                        // Check end key boundary / 检查结束键边界
                        if let Some(end_key) = &options.end_key {
                            if key_str >= *end_key {
                                break;
                            }
                        }

                        pairs.push(KvPair {
                            key: key_str,
                            value: value.to_vec(),
                        });

                        // Check limit / 检查限制
                        if let Some(limit) = options.limit {
                            if pairs.len() >= limit {
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        return Err(SmsError::Serialization(format!(
                            "Sled iterator error: {}",
                            e
                        )));
                    }
                }
            }

            if options.reverse {
                pairs.reverse();
            }

            Ok(pairs)
        })
        .await
        .map_err(|e| SmsError::Serialization(format!("Task join error: {}", e)))?
    }

    async fn count(&self) -> Result<usize, SmsError> {
        let db = self.db.clone();

        tokio::task::spawn_blocking(move || Ok(db.len()))
            .await
            .map_err(|e| SmsError::Serialization(format!("Task join error: {}", e)))?
    }

    async fn clear(&self) -> Result<(), SmsError> {
        let db = self.db.clone();

        tokio::task::spawn_blocking(move || {
            db.clear()
                .map_err(|e| SmsError::Serialization(format!("Sled clear error: {}", e)))
        })
        .await
        .map_err(|e| SmsError::Serialization(format!("Task join error: {}", e)))?
    }
}

/// RocksDB-based KV store implementation / 基于RocksDB的KV存储实现
#[cfg(feature = "rocksdb")]
#[derive(Debug)]
pub struct RocksDbKvStore {
    db: Arc<rocksdb::DB>,
}

#[cfg(feature = "rocksdb")]
impl RocksDbKvStore {
    /// Create a new RocksDB store / 创建新的RocksDB存储
    pub fn new(path: impl AsRef<std::path::Path>) -> Result<Self, SmsError> {
        let db = rocksdb::DB::open_default(path)
            .map_err(|e| SmsError::Serialization(format!("RocksDB open error: {}", e)))?;

        Ok(Self { db: Arc::new(db) })
    }
}

#[cfg(feature = "rocksdb")]
#[async_trait]
impl KvStore for RocksDbKvStore {
    async fn get(&self, key: &KvKey) -> Result<Option<KvValue>, SmsError> {
        let db = self.db.clone();
        let key = key.clone();

        tokio::task::spawn_blocking(move || {
            db.get(key.as_bytes())
                .map_err(|e| SmsError::Serialization(format!("RocksDB get error: {}", e)))
        })
        .await
        .map_err(|e| SmsError::Serialization(format!("Task join error: {}", e)))?
    }

    async fn put(&self, key: &KvKey, value: &KvValue) -> Result<(), SmsError> {
        let db = self.db.clone();
        let key = key.clone();
        let value = value.clone();

        tokio::task::spawn_blocking(move || {
            db.put(key.as_bytes(), &value)
                .map_err(|e| SmsError::Serialization(format!("RocksDB put error: {}", e)))
        })
        .await
        .map_err(|e| SmsError::Serialization(format!("Task join error: {}", e)))?
    }

    async fn delete(&self, key: &KvKey) -> Result<bool, SmsError> {
        let db = self.db.clone();
        let key = key.clone();

        tokio::task::spawn_blocking(move || {
            let existed = db
                .get(key.as_bytes())
                .map_err(|e| SmsError::Serialization(format!("RocksDB get error: {}", e)))?
                .is_some();

            db.delete(key.as_bytes())
                .map_err(|e| SmsError::Serialization(format!("RocksDB delete error: {}", e)))?;

            Ok(existed)
        })
        .await
        .map_err(|e| SmsError::Serialization(format!("Task join error: {}", e)))?
    }

    async fn exists(&self, key: &KvKey) -> Result<bool, SmsError> {
        let db = self.db.clone();
        let key = key.clone();

        tokio::task::spawn_blocking(move || {
            db.get(key.as_bytes())
                .map(|opt| opt.is_some())
                .map_err(|e| SmsError::Serialization(format!("RocksDB get error: {}", e)))
        })
        .await
        .map_err(|e| SmsError::Serialization(format!("Task join error: {}", e)))?
    }

    async fn keys_with_prefix(&self, prefix: &str) -> Result<Vec<KvKey>, SmsError> {
        let db = self.db.clone();
        let prefix = prefix.to_string();

        tokio::task::spawn_blocking(move || {
            let mut keys = Vec::new();
            let iter = db.prefix_iterator(prefix.as_bytes());

            for item in iter {
                match item {
                    Ok((key, _)) => {
                        if let Ok(key_str) = String::from_utf8(key.to_vec()) {
                            if key_str.starts_with(&prefix) {
                                keys.push(key_str);
                            } else {
                                break; // RocksDB prefix iterator is ordered
                            }
                        }
                    }
                    Err(e) => {
                        return Err(SmsError::Serialization(format!(
                            "RocksDB iterator error: {}",
                            e
                        )))
                    }
                }
            }

            Ok(keys)
        })
        .await
        .map_err(|e| SmsError::Serialization(format!("Task join error: {}", e)))?
    }

    async fn scan_prefix(&self, prefix: &str) -> Result<Vec<KvPair>, SmsError> {
        let db = self.db.clone();
        let prefix = prefix.to_string();

        tokio::task::spawn_blocking(move || {
            let mut pairs = Vec::new();
            let iter = db.prefix_iterator(prefix.as_bytes());

            for item in iter {
                match item {
                    Ok((key, value)) => {
                        if let Ok(key_str) = String::from_utf8(key.to_vec()) {
                            if key_str.starts_with(&prefix) {
                                pairs.push(KvPair {
                                    key: key_str,
                                    value: value.to_vec(),
                                });
                            } else {
                                break; // RocksDB prefix iterator is ordered
                            }
                        }
                    }
                    Err(e) => {
                        return Err(SmsError::Serialization(format!(
                            "RocksDB iterator error: {}",
                            e
                        )))
                    }
                }
            }

            Ok(pairs)
        })
        .await
        .map_err(|e| SmsError::Serialization(format!("Task join error: {}", e)))?
    }

    async fn range(&self, options: &RangeOptions) -> Result<Vec<KvPair>, SmsError> {
        let db = self.db.clone();
        let options = options.clone();

        tokio::task::spawn_blocking(move || {
            let mut pairs = Vec::new();

            let iter = if options.reverse {
                // For reverse iteration, start from end_key or last key
                if let Some(end_key) = &options.end_key {
                    db.iterator(rocksdb::IteratorMode::From(
                        end_key.as_bytes(),
                        rocksdb::Direction::Reverse,
                    ))
                } else {
                    db.iterator(rocksdb::IteratorMode::End)
                }
            } else {
                // For forward iteration
                if let Some(start_key) = &options.start_key {
                    db.iterator(rocksdb::IteratorMode::From(
                        start_key.as_bytes(),
                        rocksdb::Direction::Forward,
                    ))
                } else {
                    db.iterator(rocksdb::IteratorMode::Start)
                }
            };

            for item in iter {
                match item {
                    Ok((key, value)) => {
                        let key_str = String::from_utf8(key.to_vec()).map_err(|e| {
                            SmsError::Serialization(format!("Invalid UTF-8 key: {}", e))
                        })?;

                        // Check boundary conditions based on direction
                        if options.reverse {
                            // For reverse iteration, check start_key as lower bound
                            if let Some(start_key) = &options.start_key {
                                if key_str < *start_key {
                                    break;
                                }
                            }
                        } else {
                            // For forward iteration, check end_key as upper bound
                            if let Some(end_key) = &options.end_key {
                                if key_str >= *end_key {
                                    break;
                                }
                            }
                        }

                        pairs.push(KvPair {
                            key: key_str,
                            value: value.to_vec(),
                        });

                        // Check limit
                        if let Some(limit) = options.limit {
                            if pairs.len() >= limit {
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        return Err(SmsError::Serialization(format!(
                            "RocksDB iterator error: {}",
                            e
                        )))
                    }
                }
            }

            Ok(pairs)
        })
        .await
        .map_err(|e| SmsError::Serialization(format!("Task join error: {}", e)))?
    }

    async fn count(&self) -> Result<usize, SmsError> {
        let db = self.db.clone();

        tokio::task::spawn_blocking(move || {
            let mut count = 0;
            let iter = db.iterator(rocksdb::IteratorMode::Start);

            for item in iter {
                match item {
                    Ok(_) => count += 1,
                    Err(e) => {
                        return Err(SmsError::Serialization(format!(
                            "RocksDB iterator error: {}",
                            e
                        )))
                    }
                }
            }

            Ok(count)
        })
        .await
        .map_err(|e| SmsError::Serialization(format!("Task join error: {}", e)))?
    }

    async fn clear(&self) -> Result<(), SmsError> {
        let db = self.db.clone();

        tokio::task::spawn_blocking(move || {
            // RocksDB doesn't have a direct clear method, so we need to delete all keys
            let mut keys_to_delete = Vec::new();
            let iter = db.iterator(rocksdb::IteratorMode::Start);

            for item in iter {
                match item {
                    Ok((key, _)) => keys_to_delete.push(key.to_vec()),
                    Err(e) => {
                        return Err(SmsError::Serialization(format!(
                            "RocksDB iterator error: {}",
                            e
                        )))
                    }
                }
            }

            for key in keys_to_delete {
                db.delete(&key)
                    .map_err(|e| SmsError::Serialization(format!("RocksDB delete error: {}", e)))?;
            }

            Ok(())
        })
        .await
        .map_err(|e| SmsError::Serialization(format!("Task join error: {}", e)))?
    }
}

/// High-performance concurrent KV store using evmap / 使用evmap的高性能并发KV存储
///
/// EvmapKvStore provides extremely fast read operations with eventual consistency.
/// It's optimized for read-heavy workloads where writes are less frequent.
/// EvmapKvStore提供极快的读取操作和最终一致性。
/// 它针对读取密集型工作负载进行了优化，其中写入频率较低。
#[cfg(feature = "evmap")]
#[derive(Debug)]
pub struct EvmapKvStore {
    /// Write handle for the evmap / evmap的写入句柄
    writer: Arc<tokio::sync::Mutex<evmap::WriteHandle<KvKey, KvValue>>>,
    /// Read handle for the evmap / evmap的读取句柄
    reader: Arc<tokio::sync::Mutex<evmap::ReadHandle<KvKey, KvValue>>>,
}

#[cfg(feature = "evmap")]
impl EvmapKvStore {
    /// Create a new EvmapKvStore / 创建新的EvmapKvStore
    pub fn new() -> Self {
        let (reader, writer) = evmap::new();
        Self {
            writer: Arc::new(tokio::sync::Mutex::new(writer)),
            reader: Arc::new(tokio::sync::Mutex::new(reader)),
        }
    }
}

#[cfg(feature = "evmap")]
impl Default for EvmapKvStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "evmap")]
#[async_trait]
impl KvStore for EvmapKvStore {
    async fn get(&self, key: &KvKey) -> Result<Option<KvValue>, SmsError> {
        // evmap returns a guard with multiple values, we take the first one
        // evmap返回包含多个值的守卫，我们取第一个值
        let reader = self.reader.lock().await;
        let result = reader.get_one(key).map(|value| value.clone());
        Ok(result)
    }

    async fn put(&self, key: &KvKey, value: &KvValue) -> Result<(), SmsError> {
        let mut writer = self.writer.lock().await;
        // Clear existing values for this key and insert new one
        // 清除此键的现有值并插入新值
        writer.clear(key.clone());
        writer.insert(key.clone(), value.clone());
        writer.refresh();
        Ok(())
    }

    async fn delete(&self, key: &KvKey) -> Result<bool, SmsError> {
        let existed = self.exists(key).await?;
        if existed {
            let mut writer = self.writer.lock().await;
            writer.empty(key.clone());
            writer.refresh();
            // 释放 writer 锁，让 refresh 完全生效
            drop(writer);
            // 给一个很短的时间让 refresh 完全生效
            tokio::task::yield_now().await;
        }
        Ok(existed)
    }

    async fn exists(&self, key: &KvKey) -> Result<bool, SmsError> {
        let reader = self.reader.lock().await;
        Ok(reader.contains_key(key))
    }

    async fn keys_with_prefix(&self, prefix: &str) -> Result<Vec<KvKey>, SmsError> {
        let reader = self.reader.lock().await;
        let mut keys = Vec::new();

        // Read the map and iterate through all keys
        // 读取映射并遍历所有键
        if let Some(map_ref) = reader.read() {
            for (key, _values) in map_ref.iter() {
                if key.starts_with(prefix) {
                    keys.push(key.clone());
                }
            }
        }

        Ok(keys)
    }

    async fn scan_prefix(&self, prefix: &str) -> Result<Vec<KvPair>, SmsError> {
        let reader = self.reader.lock().await;
        let mut pairs = Vec::new();

        // Read the map and iterate through all key-value pairs
        // 读取映射并遍历所有键值对
        if let Some(map_ref) = reader.read() {
            for (key, values) in map_ref.iter() {
                if key.starts_with(prefix) {
                    // evmap can store multiple values per key, we take the first one
                    // evmap可以为每个键存储多个值，我们取第一个
                    if let Some(value) = values.iter().next() {
                        pairs.push(KvPair {
                            key: key.clone(),
                            value: value.clone(),
                        });
                    }
                }
            }
        }

        Ok(pairs)
    }

    async fn range(&self, options: &RangeOptions) -> Result<Vec<KvPair>, SmsError> {
        // evmap doesn't support efficient range queries, so we implement it by iterating all entries
        // evmap不支持高效的范围查询，所以我们通过遍历所有条目来实现
        let reader = self.reader.lock().await;

        let mut pairs: Vec<KvPair> = Vec::new();

        // Read the map and iterate through all keys
        // 读取映射并遍历所有键
        if let Some(map_ref) = reader.read() {
            for (key, values) in map_ref.iter() {
                // evmap can store multiple values per key, we take the first one
                // evmap可以为每个键存储多个值，我们取第一个
                if let Some(value) = values.iter().next() {
                    // Apply range filters / 应用范围过滤器
                    let mut include = true;

                    // Filter by start key (inclusive) / 按起始键过滤（包含）
                    if let Some(ref start) = options.start_key {
                        if key < start {
                            include = false;
                        }
                    }

                    // Filter by end key (exclusive) / 按结束键过滤（不包含）
                    if let Some(ref end) = options.end_key {
                        if key >= end {
                            include = false;
                        }
                    }

                    if include {
                        pairs.push(KvPair {
                            key: key.clone(),
                            value: value.clone(),
                        });
                    }
                }
            }
        }

        // Sort pairs by key for consistent ordering / 按键排序以保证一致的顺序
        pairs.sort_by(|a, b| a.key.cmp(&b.key));

        // Apply reverse order / 应用逆序
        if options.reverse {
            pairs.reverse();
        }

        // Apply limit / 应用限制
        if let Some(limit) = options.limit {
            pairs.truncate(limit);
        }

        Ok(pairs)
    }

    async fn count(&self) -> Result<usize, SmsError> {
        let reader = self.reader.lock().await;

        // Count all keys in the map
        // 计算映射中的所有键
        let count = if let Some(map_ref) = reader.read() {
            map_ref.len()
        } else {
            0
        };

        Ok(count)
    }

    async fn clear(&self) -> Result<(), SmsError> {
        let mut writer = self.writer.lock().await;

        // Get all keys first
        // 首先获取所有键
        let reader = self.reader.lock().await;
        let mut keys_to_remove = Vec::new();

        if let Some(map_ref) = reader.read() {
            for (key, _values) in map_ref.iter() {
                keys_to_remove.push(key.clone());
            }
        }
        drop(reader);

        // Remove all keys
        // 删除所有键
        for key in keys_to_remove {
            writer.empty(key);
        }
        writer.refresh();

        Ok(())
    }
}

/// Create a KV store instance / 创建KV存储实例
pub fn create_kv_store(store_type: KvStoreType) -> Result<Box<dyn KvStore>, SmsError> {
    match store_type {
        KvStoreType::Memory => Ok(Box::new(MemoryKvStore::new())),
        #[cfg(feature = "sled")]
        KvStoreType::Sled { path } => {
            let store = SledKvStore::new(path)?;
            Ok(Box::new(store))
        }
        #[cfg(feature = "rocksdb")]
        KvStoreType::RocksDb { path } => {
            let store = RocksDbKvStore::new(path)?;
            Ok(Box::new(store))
        }
        #[cfg(feature = "evmap")]
        KvStoreType::Evmap => Ok(Box::new(EvmapKvStore::new())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_kv_basic_operations() {
        let store = MemoryKvStore::new();

        // Test put and get / 测试存储和获取
        let key = "test_key".to_string();
        let value = b"test_value".to_vec();

        store.put(&key, &value).await.unwrap();
        let retrieved = store.get(&key).await.unwrap();
        assert_eq!(retrieved, Some(value.clone()));

        // Test exists / 测试存在性检查
        assert!(store.exists(&key).await.unwrap());
        assert!(!store.exists(&"nonexistent".to_string()).await.unwrap());

        // Test delete / 测试删除
        assert!(store.delete(&key).await.unwrap());
        assert!(!store.delete(&key).await.unwrap()); // Already deleted
        assert_eq!(store.get(&key).await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_memory_kv_range_operations() {
        let store = MemoryKvStore::new();

        // Insert test data / 插入测试数据
        let test_data = vec![
            ("key1", "value1"),
            ("key2", "value2"),
            ("key3", "value3"),
            ("prefix_a", "value_a"),
            ("prefix_b", "value_b"),
        ];

        for (key, value) in &test_data {
            store
                .put(&key.to_string(), &value.as_bytes().to_vec())
                .await
                .unwrap();
        }

        // Test range query / 测试范围查询
        let options = RangeOptions::new().start_key("key1").end_key("key3");
        let results = store.range(&options).await.unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].key, "key1");
        assert_eq!(results[1].key, "key2");

        // Test prefix query / 测试前缀查询
        let prefix_keys = store.keys_with_prefix("prefix_").await.unwrap();
        assert_eq!(prefix_keys.len(), 2);
        assert!(prefix_keys.contains(&"prefix_a".to_string()));
        assert!(prefix_keys.contains(&"prefix_b".to_string()));

        // Test count / 测试计数
        assert_eq!(store.count().await.unwrap(), 5);
    }

    #[tokio::test]
    async fn test_memory_kv_scan_prefix() {
        let store = MemoryKvStore::new();

        // Insert test data with different prefixes / 插入不同前缀的测试数据
        store
            .put(&"node:1".to_string(), &b"node1_data".to_vec())
            .await
            .unwrap();
        store
            .put(&"node:2".to_string(), &b"node2_data".to_vec())
            .await
            .unwrap();
        store
            .put(&"resource:1".to_string(), &b"resource1_data".to_vec())
            .await
            .unwrap();
        store
            .put(&"resource:2".to_string(), &b"resource2_data".to_vec())
            .await
            .unwrap();
        store
            .put(&"other:1".to_string(), &b"other1_data".to_vec())
            .await
            .unwrap();

        // Test scan_prefix for "node:" / 测试扫描"node:"前缀
        let node_pairs = store.scan_prefix("node:").await.unwrap();
        assert_eq!(node_pairs.len(), 2);

        let mut node_keys: Vec<String> = node_pairs.iter().map(|p| p.key.clone()).collect();
        node_keys.sort();
        assert_eq!(node_keys, vec!["node:1", "node:2"]);

        // Test scan_prefix for "resource:" / 测试扫描"resource:"前缀
        let resource_pairs = store.scan_prefix("resource:").await.unwrap();
        assert_eq!(resource_pairs.len(), 2);

        let mut resource_keys: Vec<String> = resource_pairs.iter().map(|p| p.key.clone()).collect();
        resource_keys.sort();
        assert_eq!(resource_keys, vec!["resource:1", "resource:2"]);

        // Test scan_prefix for non-existent prefix / 测试不存在的前缀
        let empty_pairs = store.scan_prefix("nonexistent:").await.unwrap();
        assert_eq!(empty_pairs.len(), 0);

        // Test scan_prefix for empty prefix (should return all) / 测试空前缀（应返回所有）
        let all_pairs = store.scan_prefix("").await.unwrap();
        assert_eq!(all_pairs.len(), 5);
    }

    #[tokio::test]
    async fn test_memory_kv_batch_operations() {
        let store = MemoryKvStore::new();

        // Test batch put / 测试批量存储
        let pairs = vec![
            KvPair {
                key: "batch1".to_string(),
                value: b"value1".to_vec(),
            },
            KvPair {
                key: "batch2".to_string(),
                value: b"value2".to_vec(),
            },
            KvPair {
                key: "batch3".to_string(),
                value: b"value3".to_vec(),
            },
        ];

        store.batch_put(&pairs).await.unwrap();
        assert_eq!(store.count().await.unwrap(), 3);

        // Test batch delete / 测试批量删除
        let keys = vec![
            "batch1".to_string(),
            "batch2".to_string(),
            "nonexistent".to_string(),
        ];
        let deleted = store.batch_delete(&keys).await.unwrap();
        assert_eq!(deleted, 2); // Only 2 keys existed
        assert_eq!(store.count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_serialization_helpers() {
        // Import serialization functions directly to avoid namespace conflicts
        // 直接导入序列化函数以避免命名空间冲突
        use crate::storage::serialization::{
            deserialize, extract_uuid_from_node_key, node_key, resource_key, serialize,
        };

        // Create a simple test structure that implements serde traits
        // 创建一个实现 serde trait 的简单测试结构体
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct TestNode {
            uuid: String,
            ip_address: String,
            port: u32,
        }

        let uuid = Uuid::new_v4();
        let uuid_str = uuid.to_string();
        let test_node = TestNode {
            uuid: uuid_str.clone(),
            ip_address: "192.168.1.100".to_string(),
            port: 8080,
        };

        // Test node serialization / 测试节点序列化
        let serialized = serialize(&test_node).unwrap();
        let deserialized: TestNode = deserialize(&serialized).unwrap();
        assert_eq!(deserialized.uuid, uuid_str);
        assert_eq!(deserialized.ip_address, test_node.ip_address);
        assert_eq!(deserialized.port, test_node.port);

        // Test key generation / 测试键生成
        let node_key = node_key(&uuid);
        assert_eq!(node_key, format!("node:{}", uuid));

        let resource_key = resource_key(&uuid);
        assert_eq!(resource_key, format!("resource:{}", uuid));

        // Test UUID extraction / 测试UUID提取
        let extracted_uuid = extract_uuid_from_node_key(&node_key).unwrap();
        assert_eq!(extracted_uuid, uuid);
    }

    #[tokio::test]
    async fn test_range_options() {
        let options = RangeOptions::new()
            .start_key("start")
            .end_key("end")
            .limit(10)
            .reverse(true);

        assert_eq!(options.start_key, Some("start".to_string()));
        assert_eq!(options.end_key, Some("end".to_string()));
        assert_eq!(options.limit, Some(10));
        assert!(options.reverse);
    }

    #[tokio::test]
    async fn test_kv_store_factory() {
        let store = create_kv_store(KvStoreType::Memory).unwrap();

        // Test basic operation / 测试基本操作
        let key = "factory_test".to_string();
        let value = b"factory_value".to_vec();

        store.put(&key, &value).await.unwrap();
        let retrieved = store.get(&key).await.unwrap();
        assert_eq!(retrieved, Some(value));
    }

    #[cfg(feature = "sled")]
    #[tokio::test]
    async fn test_sled_kv_basic_operations() {
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test_db");

        let store = SledKvStore::new(&db_path).unwrap();

        // Test put and get / 测试存储和获取
        let key = "test_key".to_string();
        let value = b"test_value".to_vec();

        store.put(&key, &value).await.unwrap();
        let retrieved = store.get(&key).await.unwrap();
        assert_eq!(retrieved, Some(value.clone()));

        // Test exists / 测试存在性检查
        assert!(store.exists(&key).await.unwrap());
        assert!(!store.exists(&"nonexistent".to_string()).await.unwrap());

        // Test delete / 测试删除
        assert!(store.delete(&key).await.unwrap());
        assert!(!store.delete(&key).await.unwrap()); // Already deleted
        assert_eq!(store.get(&key).await.unwrap(), None);
    }

    #[cfg(feature = "sled")]
    #[tokio::test]
    async fn test_sled_kv_range_operations() {
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test_db_range");

        let store = SledKvStore::new(&db_path).unwrap();

        // Insert test data / 插入测试数据
        let test_data = vec![
            ("key1", "value1"),
            ("key2", "value2"),
            ("key3", "value3"),
            ("prefix_a", "value_a"),
            ("prefix_b", "value_b"),
        ];

        for (key, value) in &test_data {
            store
                .put(&key.to_string(), &value.as_bytes().to_vec())
                .await
                .unwrap();
        }

        // Test range query / 测试范围查询
        let options = RangeOptions::new().start_key("key1").end_key("key3");
        let results = store.range(&options).await.unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].key, "key1");
        assert_eq!(results[1].key, "key2");

        // Test prefix query / 测试前缀查询
        let prefix_keys = store.keys_with_prefix("prefix_").await.unwrap();
        assert_eq!(prefix_keys.len(), 2);
        assert!(prefix_keys.contains(&"prefix_a".to_string()));
        assert!(prefix_keys.contains(&"prefix_b".to_string()));

        // Test count / 测试计数
        assert_eq!(store.count().await.unwrap(), 5);

        // Test clear / 测试清空
        store.clear().await.unwrap();
        assert_eq!(store.count().await.unwrap(), 0);
    }

    #[cfg(feature = "sled")]
    #[tokio::test]
    async fn test_sled_kv_store_factory() {
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().to_string_lossy().to_string();

        let store = create_kv_store(KvStoreType::Sled { path: db_path }).unwrap();

        // Test basic operation / 测试基本操作
        let key = "factory_test".to_string();
        let value = b"factory_value".to_vec();

        store.put(&key, &value).await.unwrap();
        let retrieved = store.get(&key).await.unwrap();
        assert_eq!(retrieved, Some(value));
    }

    #[tokio::test]
    async fn test_new_kv_store_factory() {
        let factory = DefaultKvStoreFactory::new();

        // Test memory store creation / 测试内存存储创建
        let config = KvStoreConfig::memory();
        let store = factory.create(&config).await.unwrap();

        // Basic operation test / 基本操作测试
        store
            .put(&"test_key".to_string(), &"test_value".as_bytes().to_vec())
            .await
            .unwrap();
        let value = store.get(&"test_key".to_string()).await.unwrap();
        assert_eq!(value, Some("test_value".as_bytes().to_vec()));
    }

    #[tokio::test]
    async fn test_kv_store_config() {
        // Test memory config / 测试内存配置
        let memory_config = KvStoreConfig::memory();
        assert_eq!(memory_config.backend, "memory");
        assert!(memory_config.params.is_empty());

        // Test config with parameters / 测试带参数的配置
        let config_with_params = KvStoreConfig::memory()
            .with_param("cache_size", "1000")
            .with_param("timeout", "30");
        assert_eq!(
            config_with_params.get_param("cache_size"),
            Some(&"1000".to_string())
        );
        assert_eq!(
            config_with_params.get_param("timeout"),
            Some(&"30".to_string())
        );
        assert_eq!(config_with_params.get_param("nonexistent"), None);

        #[cfg(feature = "sled")]
        {
            // Test sled config / 测试sled配置
            let sled_config = KvStoreConfig::sled("/tmp/test_db");
            assert_eq!(sled_config.backend, "sled");
            assert_eq!(
                sled_config.get_param("path"),
                Some(&"/tmp/test_db".to_string())
            );
        }
    }

    #[tokio::test]
    async fn test_factory_validation() {
        let factory = DefaultKvStoreFactory::new();

        // Test valid configs / 测试有效配置
        let memory_config = KvStoreConfig::memory();
        assert!(factory.validate_config(&memory_config).is_ok());

        #[cfg(feature = "sled")]
        {
            let sled_config = KvStoreConfig::sled("/tmp/test_db");
            assert!(factory.validate_config(&sled_config).is_ok());
        }

        // Test invalid config / 测试无效配置
        let invalid_config = KvStoreConfig {
            backend: "invalid_backend".to_string(),
            params: HashMap::new(),
        };
        assert!(factory.validate_config(&invalid_config).is_err());

        // Test supported backends / 测试支持的后端
        let backends = factory.supported_backends();
        assert!(backends.contains(&"memory".to_string()));
        #[cfg(feature = "sled")]
        assert!(backends.contains(&"sled".to_string()));
    }

    #[tokio::test]
    async fn test_global_factory() {
        // Test default factory / 测试默认工厂
        let factory = get_kv_store_factory();
        let backends = factory.supported_backends();
        assert!(backends.contains(&"memory".to_string()));

        // Test creating store from config / 测试从配置创建存储
        let config = KvStoreConfig::memory();
        let store = create_kv_store_from_config(&config).await.unwrap();

        // Verify it works / 验证功能正常
        store
            .put(&"global_test".to_string(), &"value".as_bytes().to_vec())
            .await
            .unwrap();
        let value = store.get(&"global_test".to_string()).await.unwrap();
        assert_eq!(value, Some("value".as_bytes().to_vec()));
    }

    #[tokio::test]
    async fn test_config_from_env() {
        // Set environment variables / 设置环境变量
        std::env::set_var("KV_STORE_BACKEND", "memory");
        std::env::set_var("KV_STORE_CACHE_SIZE", "2000");
        std::env::set_var("KV_STORE_TIMEOUT", "60");

        // Test loading from environment / 测试从环境变量加载
        let config = KvStoreConfig::from_env().unwrap();
        assert_eq!(config.backend, "memory");
        assert_eq!(config.get_param("cache_size"), Some(&"2000".to_string()));
        assert_eq!(config.get_param("timeout"), Some(&"60".to_string()));

        // Test creating store from environment / 测试从环境变量创建存储
        let store = create_kv_store_from_env().await.unwrap();
        store
            .put(&"env_test".to_string(), &"env_value".as_bytes().to_vec())
            .await
            .unwrap();
        let value = store.get(&"env_test".to_string()).await.unwrap();
        assert_eq!(value, Some("env_value".as_bytes().to_vec()));

        // Clean up environment variables / 清理环境变量
        std::env::remove_var("KV_STORE_BACKEND");
        std::env::remove_var("KV_STORE_CACHE_SIZE");
        std::env::remove_var("KV_STORE_TIMEOUT");
    }

    #[cfg(feature = "rocksdb")]
    #[tokio::test]
    async fn test_rocksdb_kv_basic_operations() {
        use tempfile::TempDir;

        // Create temporary directory for RocksDB / 为RocksDB创建临时目录
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_rocksdb");

        let store = RocksDbKvStore::new(&db_path).unwrap();

        // Test basic operations / 测试基本操作
        let key = "test_key".to_string();
        let value = b"test_value".to_vec();

        // Test put and get / 测试存储和获取
        store.put(&key, &value).await.unwrap();
        let retrieved = store.get(&key).await.unwrap();
        assert_eq!(retrieved, Some(value.clone()));

        // Test exists / 测试存在性检查
        assert!(store.exists(&key).await.unwrap());
        assert!(!store.exists(&"non_existent".to_string()).await.unwrap());

        // Test delete / 测试删除
        assert!(store.delete(&key).await.unwrap());
        // 给 evmap 一点时间让 refresh 完全生效
        tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
        assert!(!store.exists(&key).await.unwrap());
        assert!(!store.delete(&key).await.unwrap()); // Second delete should return false

        // Test count / 测试计数
        assert_eq!(store.count().await.unwrap(), 0);

        store
            .put(&"key1".to_string(), &b"value1".to_vec())
            .await
            .unwrap();
        store
            .put(&"key2".to_string(), &b"value2".to_vec())
            .await
            .unwrap();
        assert_eq!(store.count().await.unwrap(), 2);

        // Test clear / 测试清空
        store.clear().await.unwrap();
        assert_eq!(store.count().await.unwrap(), 0);
    }

    #[cfg(feature = "rocksdb")]
    #[tokio::test]
    async fn test_rocksdb_kv_range_operations() {
        use tempfile::TempDir;

        // Create temporary directory for RocksDB / 为RocksDB创建临时目录
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_rocksdb_range");

        let store = RocksDbKvStore::new(&db_path).unwrap();

        // Insert test data / 插入测试数据
        let test_data = vec![
            ("prefix_a", "value_a"),
            ("prefix_b", "value_b"),
            ("prefix_c", "value_c"),
            ("other_x", "value_x"),
            ("other_y", "value_y"),
        ];

        for (key, value) in &test_data {
            store
                .put(&key.to_string(), &value.as_bytes().to_vec())
                .await
                .unwrap();
        }

        // Test keys_with_prefix / 测试前缀键查询
        let prefix_keys = store.keys_with_prefix("prefix_").await.unwrap();
        assert_eq!(prefix_keys.len(), 3);
        assert!(prefix_keys.contains(&"prefix_a".to_string()));
        assert!(prefix_keys.contains(&"prefix_b".to_string()));
        assert!(prefix_keys.contains(&"prefix_c".to_string()));

        // Test scan_prefix / 测试前缀扫描
        let prefix_pairs = store.scan_prefix("other_").await.unwrap();
        assert_eq!(prefix_pairs.len(), 2);

        // Test range operations / 测试范围操作
        let range_options = RangeOptions::new()
            .start_key("prefix_a")
            .end_key("prefix_z")
            .limit(2);
        let range_pairs = store.range(&range_options).await.unwrap();
        assert_eq!(range_pairs.len(), 2);

        // Test reverse range / 测试反向范围
        let reverse_options = RangeOptions::new()
            .start_key("prefix_a")
            .end_key("prefix_z")
            .reverse(true)
            .limit(1);
        let reverse_pairs = store.range(&reverse_options).await.unwrap();
        assert_eq!(reverse_pairs.len(), 1);
        assert_eq!(reverse_pairs[0].key, "prefix_c");
    }

    #[cfg(feature = "rocksdb")]
    #[tokio::test]
    async fn test_rocksdb_kv_store_factory() {
        use tempfile::TempDir;

        // Create temporary directory for RocksDB / 为RocksDB创建临时目录
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_factory_rocksdb");

        let factory = DefaultKvStoreFactory::new();
        let config = KvStoreConfig::rocksdb(db_path.to_str().unwrap());

        // Test factory creation / 测试工厂创建
        let store = factory.create(&config).await.unwrap();

        // Test basic operation through factory-created store / 通过工厂创建的存储测试基本操作
        let key = "factory_test_key".to_string();
        let value = b"factory_test_value".to_vec();

        store.put(&key, &value).await.unwrap();
        let retrieved = store.get(&key).await.unwrap();
        assert_eq!(retrieved, Some(value));

        // Test that rocksdb is in supported backends / 测试rocksdb在支持的后端中
        let backends = factory.supported_backends();
        assert!(backends.contains(&"rocksdb".to_string()));

        // Test config validation / 测试配置验证
        assert!(factory.validate_config(&config).is_ok());

        // Test invalid config (missing path) / 测试无效配置（缺少路径）
        let invalid_config = KvStoreConfig {
            backend: "rocksdb".to_string(),
            params: HashMap::new(),
        };
        assert!(factory.validate_config(&invalid_config).is_err());
    }

    #[cfg(feature = "evmap")]
    #[tokio::test]
    async fn test_evmap_kv_basic_operations() {
        let store = EvmapKvStore::new();

        // Test put and get / 测试存储和获取
        let key = "test_key".to_string();
        let value = b"test_value".to_vec();

        store.put(&key, &value).await.unwrap();
        let retrieved = store.get(&key).await.unwrap();
        assert_eq!(retrieved, Some(value.clone()));

        // Test exists / 测试存在性检查
        assert!(store.exists(&key).await.unwrap());
        assert!(!store.exists(&"non_existent".to_string()).await.unwrap());

        // Test delete / 测试删除
        assert!(store.delete(&key).await.unwrap());
        assert!(!store.exists(&key).await.unwrap());
        assert!(!store.delete(&key).await.unwrap()); // Second delete should return false
    }

    #[cfg(feature = "evmap")]
    #[tokio::test]
    async fn test_evmap_kv_range_operations() {
        let store = EvmapKvStore::new();

        // Insert test data / 插入测试数据
        let pairs = vec![
            ("key1".to_string(), b"value1".to_vec()),
            ("key2".to_string(), b"value2".to_vec()),
            ("key3".to_string(), b"value3".to_vec()),
            ("key5".to_string(), b"value5".to_vec()),
        ];

        for (key, value) in &pairs {
            store.put(key, value).await.unwrap();
        }

        // Test range with start and end key / 测试带起始和结束键的范围查询
        let options = RangeOptions::new().start_key("key2").end_key("key4");
        let result = store.range(&options).await.unwrap();
        assert_eq!(result.len(), 2); // key2, key3
        assert_eq!(result[0].key, "key2");
        assert_eq!(result[1].key, "key3");

        // Test range with limit / 测试带限制的范围查询
        let options = RangeOptions::new().limit(2);
        let result = store.range(&options).await.unwrap();
        assert_eq!(result.len(), 2);

        // Test reverse range / 测试逆序范围查询
        let options = RangeOptions::new().reverse(true);
        let result = store.range(&options).await.unwrap();
        assert_eq!(result.len(), 4);
        assert_eq!(result[0].key, "key5"); // Should be in reverse order

        // Test prefix operations / 测试前缀操作
        let prefix_keys = store.keys_with_prefix("key").await.unwrap();
        assert_eq!(prefix_keys.len(), 4);

        let prefix_pairs = store.scan_prefix("key").await.unwrap();
        assert_eq!(prefix_pairs.len(), 4);

        // Test count / 测试计数
        let count = store.count().await.unwrap();
        assert_eq!(count, 4);

        // Test clear / 测试清空
        store.clear().await.unwrap();
        let count_after_clear = store.count().await.unwrap();
        assert_eq!(count_after_clear, 0);
    }

    #[cfg(feature = "evmap")]
    #[tokio::test]
    async fn test_evmap_kv_store_factory() {
        let factory = DefaultKvStoreFactory::new();
        let config = KvStoreConfig::evmap();

        // Test factory creation / 测试工厂创建
        let store = factory.create(&config).await.unwrap();

        // Test basic operation through factory-created store / 通过工厂创建的存储测试基本操作
        let key = "factory_test_key".to_string();
        let value = b"factory_test_value".to_vec();

        store.put(&key, &value).await.unwrap();
        let retrieved = store.get(&key).await.unwrap();
        assert_eq!(retrieved, Some(value));

        // Test that evmap is in supported backends / 测试evmap在支持的后端中
        let backends = factory.supported_backends();
        assert!(backends.contains(&"evmap".to_string()));

        // Test config validation / 测试配置验证
        assert!(factory.validate_config(&config).is_ok());
    }
}
