use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use base64::{engine::general_purpose, Engine as _};
use serde::Deserialize;
use std::collections::HashMap;
use tracing::{debug, error};

use crate::proto::spearlet::{
    AddObjectRefRequest, DeleteObjectRequest, GetObjectRequest, ListObjectsRequest,
    PinObjectRequest, PutObjectRequest, RemoveObjectRefRequest, UnpinObjectRequest,
};

use super::AppState;

#[derive(Deserialize)]
pub(super) struct PutObjectBody {
    pub(super) value: String,
    pub(super) metadata: Option<HashMap<String, String>>,
    pub(super) overwrite: Option<bool>,
}

#[derive(Deserialize)]
pub(super) struct ListObjectsQuery {
    pub(super) prefix: Option<String>,
    pub(super) limit: Option<i32>,
    pub(super) continuation_token: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct RefCountBody {
    pub(super) count: Option<i32>,
}

#[derive(Deserialize)]
pub(super) struct DeleteObjectQuery {
    pub(super) force: Option<bool>,
}

/// Put object endpoint / 存储对象端点
/// PUT /objects/:key
pub(super) async fn put_object(
    Path(key): Path<String>,
    State(state): State<AppState>,
    Json(body): Json<PutObjectBody>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    debug!("PUT /objects/{}", key);

    let value = match general_purpose::STANDARD.decode(&body.value) {
        Ok(v) => v,
        Err(_) => return Err(StatusCode::BAD_REQUEST),
    };

    let request = PutObjectRequest {
        key: key.clone(),
        value,
        metadata: body.metadata.unwrap_or_default(),
        overwrite: body.overwrite.unwrap_or(false),
    };

    let mut client = state.object_client.clone();
    match client.put_object(request).await {
        Ok(response) => {
            let resp = response.into_inner();
            Ok(Json(serde_json::json!({
                "success": resp.success,
                "message": resp.message,
                "key": key
            })))
        }
        Err(e) => {
            error!("Failed to put object {}: {}", key, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get object endpoint / 获取对象端点
/// GET /objects/:key
pub(super) async fn get_object(
    Path(key): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    debug!("GET /objects/{}", key);

    let request = GetObjectRequest {
        key: key.clone(),
        include_value: true,
    };

    let mut client = state.object_client.clone();
    match client.get_object(request).await {
        Ok(response) => {
            let resp = response.into_inner();
            if resp.found {
                if let Some(object) = resp.object {
                    let encoded_value = general_purpose::STANDARD.encode(&object.value);
                    Ok(Json(serde_json::json!({
                        "found": true,
                        "key": object.key,
                        "value": encoded_value,
                        "metadata": object.metadata,
                        "size": object.value.len(),
                        "created_at": object.created_at,
                        "updated_at": object.updated_at,
                        "ref_count": object.ref_count,
                        "pinned": object.pinned
                    })))
                } else {
                    Err(StatusCode::NOT_FOUND)
                }
            } else {
                Err(StatusCode::NOT_FOUND)
            }
        }
        Err(e) => {
            error!("Failed to get object {}: {}", key, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// List objects endpoint / 列出对象端点
/// GET /objects
pub(super) async fn list_objects(
    Query(params): Query<ListObjectsQuery>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    debug!("GET /objects with prefix: {:?}", params.prefix);

    let request = ListObjectsRequest {
        prefix: params.prefix.unwrap_or_default(),
        limit: params.limit.unwrap_or(100),
        start_after: params.continuation_token.unwrap_or_default(),
        include_values: true,
    };

    let mut client = state.object_client.clone();
    match client.list_objects(request).await {
        Ok(response) => {
            let resp = response.into_inner();
            let objects: Vec<serde_json::Value> = resp
                .objects
                .into_iter()
                .map(|obj| {
                    serde_json::json!({
                        "key": obj.key,
                        "size": obj.size,
                        "created_at": obj.created_at,
                        "updated_at": obj.updated_at,
                        "metadata": obj.metadata,
                        "ref_count": obj.ref_count,
                        "is_pinned": obj.pinned
                    })
                })
                .collect();

            Ok(Json(serde_json::json!({
                "objects": objects,
                "continuation_token": resp.next_start_after,
                "has_more": resp.has_more
            })))
        }
        Err(e) => {
            error!("Failed to list objects: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Add object reference endpoint / 添加对象引用端点
/// POST /objects/:key/refs
pub(super) async fn add_object_ref(
    Path(key): Path<String>,
    State(state): State<AppState>,
    Json(body): Json<RefCountBody>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    debug!("POST /objects/{}/refs", key);

    let request = AddObjectRefRequest {
        key: key.clone(),
        count: body.count.unwrap_or(1),
    };

    let mut client = state.object_client.clone();
    match client.add_object_ref(request).await {
        Ok(response) => {
            let resp = response.into_inner();
            Ok(Json(serde_json::json!({
                "success": resp.success,
                "message": resp.message,
                "new_ref_count": resp.new_ref_count
            })))
        }
        Err(e) => {
            error!("Failed to add object ref {}: {}", key, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Remove object reference endpoint / 移除对象引用端点
/// DELETE /objects/:key/refs
pub(super) async fn remove_object_ref(
    Path(key): Path<String>,
    State(state): State<AppState>,
    Json(body): Json<RefCountBody>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    debug!("DELETE /objects/{}/refs", key);

    let request = RemoveObjectRefRequest {
        key: key.clone(),
        count: body.count.unwrap_or(1),
    };

    let mut client = state.object_client.clone();
    match client.remove_object_ref(request).await {
        Ok(response) => {
            let resp = response.into_inner();
            Ok(Json(serde_json::json!({
                "success": resp.success,
                "message": resp.message,
                "new_ref_count": resp.new_ref_count
            })))
        }
        Err(e) => {
            error!("Failed to remove object ref {}: {}", key, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Pin object endpoint / 固定对象端点
/// POST /objects/:key/pin
pub(super) async fn pin_object(
    Path(key): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    debug!("POST /objects/{}/pin", key);

    let request = PinObjectRequest { key: key.clone() };

    let mut client = state.object_client.clone();
    match client.pin_object(request).await {
        Ok(response) => {
            let resp = response.into_inner();
            Ok(Json(serde_json::json!({
                "success": resp.success,
                "message": resp.message
            })))
        }
        Err(e) => {
            error!("Failed to pin object {}: {}", key, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Unpin object endpoint / 取消固定对象端点
/// DELETE /objects/:key/pin
pub(super) async fn unpin_object(
    Path(key): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    debug!("DELETE /objects/{}/pin", key);

    let request = UnpinObjectRequest { key: key.clone() };

    let mut client = state.object_client.clone();
    match client.unpin_object(request).await {
        Ok(response) => {
            let resp = response.into_inner();
            Ok(Json(serde_json::json!({
                "success": resp.success,
                "message": resp.message
            })))
        }
        Err(e) => {
            error!("Failed to unpin object {}: {}", key, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Delete object endpoint / 删除对象端点
/// DELETE /objects/:key
pub(super) async fn delete_object(
    Path(key): Path<String>,
    Query(params): Query<DeleteObjectQuery>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    debug!("DELETE /objects/{}", key);

    let request = DeleteObjectRequest {
        key: key.clone(),
        force: params.force.unwrap_or(false),
    };

    let mut client = state.object_client.clone();
    match client.delete_object(request).await {
        Ok(response) => {
            let resp = response.into_inner();
            Ok(Json(serde_json::json!({
                "success": resp.success,
                "message": resp.message
            })))
        }
        Err(e) => {
            error!("Failed to delete object {}: {}", key, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
