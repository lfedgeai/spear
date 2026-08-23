//! Object service implementation for spearlet
//! spearlet的对象服务实现

use crate::storage::{serialization, KvStore};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tonic::{Request, Response, Status};
use tracing::{debug, error, info, warn};

use crate::proto::spearlet::{
    object_service_server::ObjectService, AddObjectRefRequest, AddObjectRefResponse,
    DeleteObjectRequest, DeleteObjectResponse, GetObjectRequest, GetObjectResponse,
    ListObjectsRequest, ListObjectsResponse, Object, ObjectMeta, PinObjectRequest,
    PinObjectResponse, PutObjectRequest, PutObjectResponse, RemoveObjectRefRequest,
    RemoveObjectRefResponse, UnpinObjectRequest, UnpinObjectResponse,
};

/// Get the current unix timestamp in seconds / 获取当前 Unix 秒级时间戳
fn current_unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// Object key generation helpers / 对象键生成辅助函数
mod object_keys {
    /// Generate object key / 生成对象键
    pub fn object_key(key: &str) -> String {
        format!("object:{}", key)
    }

    /// Object prefix for scanning / 对象扫描前缀
    pub fn object_prefix() -> &'static str {
        "object:"
    }
}

/// Object service statistics / 对象服务统计信息
#[derive(Debug, Clone)]
pub struct ObjectServiceStats {
    pub object_count: usize,
    pub total_size: u64,
    pub pinned_count: usize,
}

/// Internal object representation / 内部对象表示
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredObject {
    key: String,
    value: Vec<u8>,
    created_at: i64,
    updated_at: i64,
    ref_count: i32,
    pinned: bool,
    metadata: HashMap<String, String>,
}

impl StoredObject {
    fn new(key: String, value: Vec<u8>, metadata: HashMap<String, String>) -> Self {
        let now = current_unix_timestamp();

        Self {
            key,
            value,
            created_at: now,
            updated_at: now,
            ref_count: 1,
            pinned: false,
            metadata,
        }
    }

    fn to_object(&self) -> Object {
        Object {
            key: self.key.clone(),
            value: self.value.clone(),
            size: self.value.len() as i64,
            created_at: self.created_at,
            updated_at: self.updated_at,
            ref_count: self.ref_count,
            pinned: self.pinned,
            metadata: self.metadata.clone(),
        }
    }

    fn to_object_meta(&self) -> ObjectMeta {
        ObjectMeta {
            key: self.key.clone(),
            size: self.value.len() as i64,
            created_at: self.created_at,
            updated_at: self.updated_at,
            ref_count: self.ref_count,
            pinned: self.pinned,
            metadata: self.metadata.clone(),
        }
    }

    fn update_value(&mut self, value: Vec<u8>, metadata: HashMap<String, String>) {
        self.value = value;
        self.metadata = metadata;
        self.touch();
    }

    fn touch(&mut self) {
        self.updated_at = current_unix_timestamp();
    }
}

/// Aggregated scan result for object storage / 对象存储扫描后的聚合结果
#[derive(Debug, Default)]
struct ObjectScanStats {
    object_count: usize,
    total_size: u64,
    pinned_count: usize,
}

/// Persistence action after mutating an object / 对象变更后的持久化动作
enum ObjectPersistenceAction {
    Save(StoredObject),
    Delete,
}

/// Object service implementation / 对象服务实现
#[derive(Debug, Clone)]
pub struct ObjectServiceImpl {
    /// KV store for object storage / 对象存储的KV存储
    kv_store: Arc<dyn KvStore>,
    /// Maximum object size in bytes / 最大对象大小（字节）
    max_object_size: u64,
}

impl ObjectServiceImpl {
    /// Create a new object service with KV store / 使用KV存储创建新的对象服务
    pub fn new(kv_store: Arc<dyn KvStore>, max_object_size: u64) -> Self {
        Self {
            kv_store,
            max_object_size,
        }
    }

    /// Create a new object service with memory KV store / 使用内存KV存储创建新的对象服务
    pub fn new_with_memory(max_object_size: u64) -> Self {
        use crate::storage::MemoryKvStore;
        Self {
            kv_store: Arc::new(MemoryKvStore::new()),
            max_object_size,
        }
    }

    /// Load a stored object from KV storage by internal key.
    /// 按内部键从 KV 存储加载对象。
    async fn load_stored_object(&self, kv_key: &String) -> Result<Option<StoredObject>, String> {
        let Some(data) = self
            .kv_store
            .get(kv_key)
            .await
            .map_err(|err| format!("Failed to get object: {err}"))?
        else {
            return Ok(None);
        };

        serialization::deserialize::<StoredObject>(&data)
            .map(Some)
            .map_err(|err| format!("Failed to deserialize object: {err}"))
    }

    /// Store a serialized object back into KV storage by internal key.
    /// 按内部键将序列化后的对象写回 KV 存储。
    async fn store_stored_object(
        &self,
        kv_key: &String,
        object: &StoredObject,
    ) -> Result<(), String> {
        let serialized = serialization::serialize(object)
            .map_err(|err| format!("Failed to serialize object: {err}"))?;
        self.kv_store
            .put(kv_key, &serialized)
            .await
            .map_err(|err| format!("Failed to store object: {err}"))
    }

    /// Delete a stored object by internal key.
    /// 按内部键删除对象。
    async fn delete_stored_object(&self, kv_key: &String) -> Result<(), String> {
        self.kv_store
            .delete(kv_key)
            .await
            .map(|_| ())
            .map_err(|err| format!("Failed to delete object: {err}"))
    }

    /// Persist the mutation result chosen by an object update closure.
    /// 持久化对象更新闭包选择的变更结果。
    async fn persist_object_action(
        &self,
        request_key: &str,
        kv_key: &String,
        action: ObjectPersistenceAction,
    ) -> Result<(), Status> {
        match action {
            ObjectPersistenceAction::Save(object) => self
                .store_stored_object(kv_key, &object)
                .await
                .map_err(|message| {
                    error!("Failed to save updated object {}: {}", request_key, message);
                    Status::internal(message)
                }),
            ObjectPersistenceAction::Delete => {
                self.delete_stored_object(kv_key).await.map_err(|message| {
                    error!("Failed to delete object {}: {}", request_key, message);
                    Status::internal(message)
                })
            }
        }
    }

    /// Load an existing object, apply a mutation, and persist the chosen action.
    /// 加载已有对象，执行一次变更，并持久化对应动作。
    async fn update_existing_object<R, F>(
        &self,
        request_key: &str,
        kv_key: &String,
        not_found_response: R,
        mutate: F,
    ) -> Result<R, Status>
    where
        F: FnOnce(&mut StoredObject) -> Result<(ObjectPersistenceAction, R), R>,
    {
        let Some(mut object) = self.load_stored_object(kv_key).await.map_err(|message| {
            error!("Failed to get object {}: {}", request_key, message);
            Status::internal(message)
        })?
        else {
            return Ok(not_found_response);
        };

        let (action, response) = match mutate(&mut object) {
            Ok(result) => result,
            Err(response) => return Ok(response),
        };

        self.persist_object_action(request_key, kv_key, action)
            .await?;
        Ok(response)
    }

    /// Scan stored objects once and aggregate lightweight statistics.
    /// 单次扫描对象存储并聚合轻量级统计信息。
    async fn scan_object_stats(&self) -> ObjectScanStats {
        match self
            .kv_store
            .scan_prefix(object_keys::object_prefix())
            .await
        {
            Ok(pairs) => {
                let mut stats = ObjectScanStats {
                    object_count: pairs.len(),
                    ..ObjectScanStats::default()
                };

                for pair in pairs {
                    if let Ok(stored_obj) = serialization::deserialize::<StoredObject>(&pair.value)
                    {
                        stats.total_size += stored_obj.value.len() as u64;
                        if stored_obj.pinned {
                            stats.pinned_count += 1;
                        }
                    }
                }

                stats
            }
            Err(_) => ObjectScanStats::default(),
        }
    }

    /// Get object count / 获取对象数量
    pub async fn object_count(&self) -> usize {
        self.scan_object_stats().await.object_count
    }

    /// Get total object size / 获取对象总大小
    pub async fn total_object_size(&self) -> u64 {
        self.scan_object_stats().await.total_size
    }

    /// Get pinned object count / 获取固定对象数量
    pub async fn pinned_object_count(&self) -> usize {
        self.scan_object_stats().await.pinned_count
    }

    /// Cleanup objects with zero references / 清理零引用对象
    pub async fn cleanup_objects(&self) -> usize {
        match self
            .kv_store
            .scan_prefix(object_keys::object_prefix())
            .await
        {
            Ok(pairs) => {
                let mut cleaned_count = 0;
                for pair in pairs {
                    if let Ok(stored_obj) = serialization::deserialize::<StoredObject>(&pair.value)
                    {
                        if stored_obj.ref_count <= 0
                            && !stored_obj.pinned
                            && self.kv_store.delete(&pair.key).await.is_ok()
                        {
                            cleaned_count += 1;
                        }
                    }
                }
                if cleaned_count > 0 {
                    info!("Cleaned up {} objects with zero references", cleaned_count);
                }
                cleaned_count
            }
            Err(_) => 0,
        }
    }

    /// Get service statistics / 获取服务统计信息
    pub async fn get_stats(&self) -> ObjectServiceStats {
        let stats = self.scan_object_stats().await;
        ObjectServiceStats {
            object_count: stats.object_count,
            total_size: stats.total_size,
            pinned_count: stats.pinned_count,
        }
    }

    fn validate_put_request(&self, req: &PutObjectRequest) -> Result<(), PutObjectResponse> {
        if req.key.is_empty() {
            return Err(PutObjectResponse {
                success: false,
                message: "Object key cannot be empty".to_string(),
                object_meta: None,
            });
        }

        if req.value.len() as u64 > self.max_object_size {
            return Err(PutObjectResponse {
                success: false,
                message: format!(
                    "Object size {} exceeds maximum size {}",
                    req.value.len(),
                    self.max_object_size
                ),
                object_meta: None,
            });
        }

        Ok(())
    }

    fn build_object_for_put(
        &self,
        req: PutObjectRequest,
        existing_object: Option<StoredObject>,
    ) -> Result<StoredObject, PutObjectResponse> {
        match existing_object {
            Some(existing_obj) if !req.overwrite => Err(PutObjectResponse {
                success: false,
                message: "Object already exists and overwrite is false".to_string(),
                object_meta: Some(existing_obj.to_object_meta()),
            }),
            Some(mut existing_obj) => {
                existing_obj.update_value(req.value, req.metadata);
                Ok(existing_obj)
            }
            None => Ok(StoredObject::new(req.key, req.value, req.metadata)),
        }
    }
}

impl From<Arc<ObjectServiceImpl>> for ObjectServiceImpl {
    fn from(value: Arc<ObjectServiceImpl>) -> Self {
        value.as_ref().clone()
    }
}

#[tonic::async_trait]
impl ObjectService for ObjectServiceImpl {
    /// Put or overwrite object content / 写入或覆盖对象内容
    async fn put_object(
        &self,
        request: Request<PutObjectRequest>,
    ) -> Result<Response<PutObjectResponse>, Status> {
        let req = request.into_inner();

        debug!("PutObject request for key: {}", req.key);

        if let Err(response) = self.validate_put_request(&req) {
            return Ok(Response::new(response));
        }

        let kv_key = object_keys::object_key(&req.key);

        let existing_object = match self.load_stored_object(&kv_key).await {
            Ok(object) => object,
            Err(message) => {
                error!("Failed to load object {}: {}", req.key, message);
                return Ok(Response::new(PutObjectResponse {
                    success: false,
                    message,
                    object_meta: None,
                }));
            }
        };
        let stored_obj = match self.build_object_for_put(req, existing_object) {
            Ok(object) => object,
            Err(response) => return Ok(Response::new(response)),
        };

        if let Err(message) = self.store_stored_object(&kv_key, &stored_obj).await {
            error!("Failed to store object {}: {}", stored_obj.key, message);
            return Ok(Response::new(PutObjectResponse {
                success: false,
                message,
                object_meta: None,
            }));
        }

        info!("Successfully put object: {}", stored_obj.key);

        Ok(Response::new(PutObjectResponse {
            success: true,
            message: "Object stored successfully".to_string(),
            object_meta: Some(stored_obj.to_object_meta()),
        }))
    }

    /// Get object content / 读取指定对象的内容
    async fn get_object(
        &self,
        request: Request<GetObjectRequest>,
    ) -> Result<Response<GetObjectResponse>, Status> {
        let req = request.into_inner();

        debug!("GetObject request for key: {}", req.key);

        let key = object_keys::object_key(&req.key);

        match self.load_stored_object(&key).await {
            Ok(Some(obj)) => {
                info!("Successfully retrieved object: {}", req.key);
                Ok(Response::new(GetObjectResponse {
                    found: true,
                    message: "Object retrieved successfully".to_string(),
                    object: Some(obj.to_object()),
                }))
            }
            Ok(None) => {
                warn!("Object not found: {}", req.key);
                Ok(Response::new(GetObjectResponse {
                    found: false,
                    message: "Object not found".to_string(),
                    object: None,
                }))
            }
            Err(message) => {
                error!("Failed to get object {}: {}", req.key, message);
                Err(Status::internal(message))
            }
        }
    }

    /// List objects with specified prefix / 列出指定前缀下的所有对象
    async fn list_objects(
        &self,
        request: Request<ListObjectsRequest>,
    ) -> Result<Response<ListObjectsResponse>, Status> {
        let req = request.into_inner();

        debug!("ListObjects request with prefix: {}", req.prefix);

        let limit = if req.limit <= 0 {
            1000
        } else {
            req.limit as usize
        };

        // Scan all objects from KV store / 从 KV 存储中扫描所有对象
        let mut matching_objects: Vec<Object> = Vec::new();

        match self
            .kv_store
            .scan_prefix(object_keys::object_prefix())
            .await
        {
            Ok(entries) => {
                for pair in entries {
                    match serialization::deserialize::<StoredObject>(&pair.value) {
                        Ok(obj) => {
                            if obj.key.starts_with(&req.prefix) {
                                matching_objects.push(obj.to_object());
                            }
                        }
                        Err(e) => {
                            warn!("Failed to deserialize object during list: {}", e);
                            continue;
                        }
                    }
                }
            }
            Err(e) => {
                error!("Failed to scan objects: {}", e);
                return Err(Status::internal(format!("Failed to scan objects: {}", e)));
            }
        }

        // Sort by key for consistent ordering / 按键排序以保持一致的顺序
        matching_objects.sort_by(|a, b| a.key.cmp(&b.key));

        // Handle pagination / 处理分页
        let start_index = if req.start_after.is_empty() {
            0
        } else {
            // Simple pagination using index (in production, use proper tokens)
            // 简单的索引分页（生产环境中应使用适当的令牌）
            req.start_after.parse::<usize>().unwrap_or(0)
        };

        let end_index = std::cmp::min(start_index + limit, matching_objects.len());
        let page_objects = matching_objects[start_index..end_index].to_vec();

        let has_more = end_index < matching_objects.len();
        let next_token = if has_more {
            end_index.to_string()
        } else {
            String::new()
        };

        info!(
            "Listed {} objects with prefix: {}",
            page_objects.len(),
            req.prefix
        );

        Ok(Response::new(ListObjectsResponse {
            objects: page_objects,
            next_start_after: next_token,
            has_more,
        }))
    }

    /// Add reference count to prevent premature garbage collection / 增加引用计数，防止对象被提前回收
    async fn add_object_ref(
        &self,
        request: Request<AddObjectRefRequest>,
    ) -> Result<Response<AddObjectRefResponse>, Status> {
        let req = request.into_inner();
        let count = if req.count <= 0 { 1 } else { req.count };

        debug!(
            "AddObjectRef request for key: {}, count: {}",
            req.key, count
        );

        let key = object_keys::object_key(&req.key);
        let response = self
            .update_existing_object(
                &req.key,
                &key,
                AddObjectRefResponse {
                    success: false,
                    message: "Object not found".to_string(),
                    new_ref_count: 0,
                },
                |object| {
                    object.ref_count += count;
                    let new_ref_count = object.ref_count;
                    Ok((
                        ObjectPersistenceAction::Save(object.clone()),
                        AddObjectRefResponse {
                            success: true,
                            message: "Reference count added successfully".to_string(),
                            new_ref_count,
                        },
                    ))
                },
            )
            .await?;

        if !response.success {
            warn!("Cannot add reference to non-existent object: {}", req.key);
        } else {
            info!(
                "Added {} references to object: {}, new count: {}",
                count, req.key, response.new_ref_count
            );
        }

        Ok(Response::new(response))
    }

    /// Remove reference count for lifecycle management / 减少引用计数，用于生命周期管理
    async fn remove_object_ref(
        &self,
        request: Request<RemoveObjectRefRequest>,
    ) -> Result<Response<RemoveObjectRefResponse>, Status> {
        let req = request.into_inner();
        let count = if req.count <= 0 { 1 } else { req.count };

        debug!(
            "RemoveObjectRef request for key: {}, count: {}",
            req.key, count
        );

        let key = object_keys::object_key(&req.key);
        let response = self
            .update_existing_object(
                &req.key,
                &key,
                RemoveObjectRefResponse {
                    success: false,
                    message: "Object not found".to_string(),
                    new_ref_count: 0,
                    deleted: false,
                },
                |object| {
                    object.ref_count = std::cmp::max(0, object.ref_count - count);
                    let new_ref_count = object.ref_count;
                    let deleted = new_ref_count == 0 && !object.pinned;
                    let action = if deleted {
                        ObjectPersistenceAction::Delete
                    } else {
                        ObjectPersistenceAction::Save(object.clone())
                    };
                    Ok((
                        action,
                        RemoveObjectRefResponse {
                            success: true,
                            message: if deleted {
                                "Object deleted due to zero references".to_string()
                            } else {
                                "Reference count removed successfully".to_string()
                            },
                            new_ref_count,
                            deleted,
                        },
                    ))
                },
            )
            .await?;

        if !response.success {
            warn!(
                "Cannot remove reference from non-existent object: {}",
                req.key
            );
        } else if response.deleted {
            info!("Removed object {} due to zero references", req.key);
        } else {
            info!(
                "Removed {} references from object: {}, new count: {}",
                count, req.key, response.new_ref_count
            );
        }

        Ok(Response::new(response))
    }

    /// Pin object to disable automatic garbage collection / 将对象标记为常驻，禁用自动回收机制
    async fn pin_object(
        &self,
        request: Request<PinObjectRequest>,
    ) -> Result<Response<PinObjectResponse>, Status> {
        let req = request.into_inner();

        debug!("PinObject request for key: {}", req.key);

        let key = object_keys::object_key(&req.key);
        let mut was_already_pinned = false;
        let response = match self
            .update_existing_object(
                &req.key,
                &key,
                PinObjectResponse {
                    success: false,
                    message: "Object not found".to_string(),
                },
                |object| {
                    was_already_pinned = object.pinned;
                    object.pinned = true;
                    Ok((
                        ObjectPersistenceAction::Save(object.clone()),
                        PinObjectResponse {
                            success: true,
                            message: "Object pinned successfully".to_string(),
                        },
                    ))
                },
            )
            .await
        {
            Ok(response) => response,
            Err(status) => {
                return Ok(Response::new(PinObjectResponse {
                    success: false,
                    message: status.message().to_string(),
                }))
            }
        };

        if !response.success {
            warn!("Cannot pin non-existent object: {}", req.key);
        } else {
            info!(
                "Pinned object: {}, was already pinned: {}",
                req.key, was_already_pinned
            );
        }

        Ok(Response::new(response))
    }

    /// Unpin object to restore normal garbage collection / 取消常驻标记，恢复为正常回收状态
    async fn unpin_object(
        &self,
        request: Request<UnpinObjectRequest>,
    ) -> Result<Response<UnpinObjectResponse>, Status> {
        let req = request.into_inner();

        debug!("UnpinObject request for key: {}", req.key);

        let key = object_keys::object_key(&req.key);
        let response = match self
            .update_existing_object(
                &req.key,
                &key,
                UnpinObjectResponse {
                    success: false,
                    message: "Object not found".to_string(),
                },
                |object| {
                    if !object.pinned {
                        return Err(UnpinObjectResponse {
                            success: false,
                            message: "Object is not pinned".to_string(),
                        });
                    }

                    object.pinned = false;
                    object.touch();
                    Ok((
                        ObjectPersistenceAction::Save(object.clone()),
                        UnpinObjectResponse {
                            success: true,
                            message: "Object unpinned successfully".to_string(),
                        },
                    ))
                },
            )
            .await
        {
            Ok(response) => response,
            Err(status) => {
                return Ok(Response::new(UnpinObjectResponse {
                    success: false,
                    message: status.message().to_string(),
                }))
            }
        };

        match response.message.as_str() {
            "Object not found" => warn!("Cannot unpin non-existent object: {}", req.key),
            "Object is not pinned" => warn!("Object {} is not pinned", req.key),
            _ => info!("Unpinned object: {}", req.key),
        }

        Ok(Response::new(response))
    }

    /// Delete object (for debugging and manual cleanup) / 删除对象（用于调试和手动清理）
    async fn delete_object(
        &self,
        request: Request<DeleteObjectRequest>,
    ) -> Result<Response<DeleteObjectResponse>, Status> {
        let req = request.into_inner();

        debug!(
            "DeleteObject request for key: {}, force: {}",
            req.key, req.force
        );

        let key = object_keys::object_key(&req.key);
        let mut was_pinned = false;
        let response = match self
            .update_existing_object(
                &req.key,
                &key,
                DeleteObjectResponse {
                    success: false,
                    message: "Object not found".to_string(),
                    deleted: false,
                },
                |object| {
                    was_pinned = object.pinned;
                    if was_pinned && !req.force {
                        return Err(DeleteObjectResponse {
                            success: false,
                            message: "Cannot delete pinned object without force flag".to_string(),
                            deleted: false,
                        });
                    }

                    Ok((
                        ObjectPersistenceAction::Delete,
                        DeleteObjectResponse {
                            success: true,
                            message: "Object deleted successfully".to_string(),
                            deleted: true,
                        },
                    ))
                },
            )
            .await
        {
            Ok(response) => response,
            Err(status) => {
                return Ok(Response::new(DeleteObjectResponse {
                    success: false,
                    message: status.message().to_string(),
                    deleted: false,
                }))
            }
        };

        match response.message.as_str() {
            "Object not found" => warn!("Cannot delete non-existent object: {}", req.key),
            "Cannot delete pinned object without force flag" => warn!(
                "Cannot delete pinned object without force flag: {}",
                req.key
            ),
            _ => info!("Deleted object: {}, was pinned: {}", req.key, was_pinned),
        }

        Ok(Response::new(response))
    }
}
