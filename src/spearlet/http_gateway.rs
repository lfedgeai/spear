//! HTTP gateway implementation for spearlet
//! spearlet的HTTP gateway实现

use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Json},
    routing::{delete, get, post, put},
    Router,
};
use base64::{engine::general_purpose, Engine as _};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::SystemTime;
use tonic::transport::Channel;
use tracing::{debug, error, info};

use crate::proto::spearlet::{
    execution_service_client::ExecutionServiceClient,
    invocation_service_client::InvocationServiceClient, object_service_client::ObjectServiceClient,
    AddObjectRefRequest, DeleteObjectRequest, GetExecutionRequest, GetObjectRequest, InvokeRequest,
    ListObjectsRequest, PinObjectRequest, PutObjectRequest, RemoveObjectRefRequest,
    TerminateExecutionRequest, UnpinObjectRequest,
};
use crate::spearlet::config::SpearletConfig;
use crate::spearlet::ai::credential_sync::global_credential_sync_status_snapshot;
use crate::spearlet::execution::ai::engine_holder;
use crate::spearlet::function_service::FunctionServiceImpl;
use crate::spearlet::grpc_server::HealthService;

/// HTTP gateway server / HTTP网关服务器
pub struct HttpGateway {
    /// Server configuration / 服务器配置
    config: Arc<SpearletConfig>,
    /// Health service / 健康检查服务
    health_service: Arc<HealthService>,
    function_service: Arc<FunctionServiceImpl>,
    object_client: ObjectServiceClient<Channel>,
    invocation_client: InvocationServiceClient<Channel>,
    execution_client: ExecutionServiceClient<Channel>,
}

/// Application state / 应用状态
#[derive(Clone)]
pub(crate) struct AppState {
    object_client: ObjectServiceClient<Channel>,
    invocation_client: InvocationServiceClient<Channel>,
    execution_client: ExecutionServiceClient<Channel>,
    health_service: Arc<HealthService>,
    function_service: Arc<FunctionServiceImpl>,
    config: Arc<SpearletConfig>,
}

pub(crate) fn new_app_state(
    object_client: ObjectServiceClient<Channel>,
    invocation_client: InvocationServiceClient<Channel>,
    execution_client: ExecutionServiceClient<Channel>,
    health_service: Arc<HealthService>,
    function_service: Arc<FunctionServiceImpl>,
    config: Arc<SpearletConfig>,
) -> AppState {
    AppState {
        object_client,
        invocation_client,
        execution_client,
        health_service,
        function_service,
        config,
    }
}

pub(crate) fn build_router(state: AppState, swagger_enabled: bool) -> Router {
    let mut app = Router::new()
        .route("/health", get(health_check))
        .route("/status", get(status_check))
        .route("/objects/{key}", put(put_object))
        .route("/objects/{key}", get(get_object))
        .route("/objects", get(list_objects))
        .route("/objects/{key}/refs", post(add_object_ref))
        .route("/objects/{key}/refs", delete(remove_object_ref))
        .route("/objects/{key}/pin", post(pin_object))
        .route("/objects/{key}/pin", delete(unpin_object))
        .route("/objects/{key}", delete(delete_object))
        .route("/functions/execute", post(execute_function))
        .route(
            "/functions/executions/{execution_id}",
            get(get_execution_status),
        )
        .route(
            "/functions/executions/{execution_id}/cancel",
            post(cancel_execution),
        )
        .route("/tasks", get(list_tasks))
        .route("/tasks/{task_id}", get(get_task))
        .route("/tasks/{task_id}/executions", get(get_task_executions))
        .route("/monitoring/stats", get(get_stats))
        .route("/monitoring/health", get(get_health_status))
        .route("/monitoring/ai/backends", get(get_ai_backends))
        .route("/monitoring/ai/credentials", get(get_ai_credentials))
        .route(
            "/api/v1/executions/{execution_id}/streams/ws",
            get(user_stream_ws),
        );

    if swagger_enabled {
        app = app
            .route("/api-docs", get(api_docs))
            .route("/api/openapi.json", get(api_docs))
            .route("/swagger-ui", get(swagger_ui))
            .route("/docs", get(swagger_ui));
    }

    if std::env::var("SPEAR_E2E").ok().as_deref() == Some("1") {
        app = app.route("/__e2e/llm/router-filter", get(e2e_llm_router_filter));
    }

    app.with_state::<()>(state)
}

async fn user_stream_ws(
    Path(execution_id): Path<String>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    if execution_id.is_empty() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    ws.on_upgrade(move |socket| user_stream_ws_loop(execution_id, socket))
}

async fn user_stream_ws_loop(execution_id: String, socket: WebSocket) {
    let (mut ws_tx, mut ws_rx) = socket.split();
    let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<Message>();
    let mut had_hub = false;

    let writer = tokio::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            if ws_tx.send(msg).await.is_err() {
                break;
            }
        }
    });

    let exit_reason = loop {
        if !had_hub
            && crate::spearlet::execution::host_api::user_stream::ExecutionUserStreamHub::get(
                &execution_id,
            )
            .is_some()
        {
            had_hub = true;
        } else if had_hub
            && crate::spearlet::execution::host_api::user_stream::ExecutionUserStreamHub::get(
                &execution_id,
            )
            .is_none()
        {
            let _ = out_tx.send(Message::Close(None));
            break "execution_user_stream_hub_disappeared".to_string();
        }

        tokio::select! {
            msg = ws_rx.next() => {
                let Some(msg) = msg else {
                    break "browser_ws_eof".to_string();
                };
                let Ok(msg) = msg else {
                    break "browser_ws_error".to_string();
                };
                match msg {
                    Message::Binary(frame) => {
                        let rc = crate::spearlet::execution::host_api::user_stream::ws_push_frame(
                            &execution_id,
                            frame.to_vec(),
                        );
                        if rc < 0 {
                            let _ = out_tx.send(Message::Close(None));
                            break format!("ws_push_frame_error:{rc}");
                        }
                    }
                    Message::Close(_) => {
                        break "browser_ws_close_frame".to_string()
                    },
                    Message::Ping(p) => {
                        let _ = out_tx.send(Message::Pong(p));
                    }
                    _ => {}
                }
            }
            _ = crate::spearlet::execution::host_api::user_stream::ws_wait_any_outbound(&execution_id) => {
                while let Some(frame) =
                    crate::spearlet::execution::host_api::user_stream::ws_pop_any_outbound(
                        &execution_id,
                    )
                {
                    let _ = out_tx.send(Message::Binary(prost::bytes::Bytes::from(frame)));
                }
            }
        }
    };

    info!(
        execution_id = %execution_id,
        exit_reason = %exit_reason,
        "user_stream_ws_loop stopping"
    );
    drop(out_tx);
    let _ = writer.await;
    crate::spearlet::execution::host_api::user_stream::map_ws_close_to_channels(&execution_id);
}

#[derive(Deserialize)]
struct E2eLlmRouterFilterQuery {
    content: Option<String>,
    model: Option<String>,
}

async fn e2e_llm_router_filter(
    State(state): State<AppState>,
    Query(q): Query<E2eLlmRouterFilterQuery>,
) -> impl IntoResponse {
    let Some(hub) =
        crate::spearlet::execution::ai::router::grpc_filter_stream::RouterFilterStreamHub::global()
            .filter(|h| h.config.enabled)
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"router_filter_disabled_or_unavailable"})),
        )
            .into_response();
    };

    let content = q.content.unwrap_or_else(|| "my secret is 123".to_string());
    let model = q.model.unwrap_or_else(|| "gpt-4o-mini".to_string());

    let req = crate::spearlet::execution::ai::ir::CanonicalRequestEnvelope {
        version: 1,
        request_id: uuid::Uuid::new_v4().to_string(),
        operation: crate::spearlet::execution::ai::ir::Operation::ChatCompletions,
        meta: HashMap::new(),
        routing: crate::spearlet::execution::ai::ir::RoutingHints::default(),
        requirements: Default::default(),
        timeout_ms: Some(2000),
        payload: crate::spearlet::execution::ai::ir::Payload::ChatCompletions(
            crate::spearlet::execution::ai::ir::ChatCompletionsPayload {
                model,
                messages: vec![crate::spearlet::execution::ai::ir::ChatMessage {
                    role: "user".to_string(),
                    content: serde_json::Value::String(content),
                    tool_call_id: None,
                    tool_calls: None,
                    name: None,
                }],
                tools: vec![],
                params: HashMap::new(),
            },
        ),
        extra: HashMap::new(),
    };

    let runtime_config = crate::spearlet::execution::runtime::RuntimeConfig {
        runtime_type: crate::spearlet::execution::RuntimeType::Wasm,
        settings: HashMap::new(),
        global_environment: HashMap::new(),
        spearlet_config: Some((*state.config).clone()),
        resource_pool: crate::spearlet::execution::runtime::ResourcePoolConfig::default(),
    };

    let (registry, _policy) =
        crate::spearlet::execution::ai::router::builder::build_registry_from_runtime_config(
            &runtime_config,
        );
    let mut candidates = registry.candidates(&req);
    if candidates.len() < 2 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"need_at_least_two_candidates","candidate_count":candidates.len()})),
        )
            .into_response();
    }

    let (resp, _trace) = match tokio::task::block_in_place(|| {
        hub.try_filter_candidates_blocking(&req, &mut candidates, 1500)
    }) {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error":e.code,"message":e.message})),
            )
                .into_response()
        }
    };

    let mut kept: Vec<&crate::spearlet::execution::ai::router::registry::BackendInstance> =
        candidates;
    let outcome =
        crate::spearlet::execution::ai::router::filter_decision::apply_filter_response(
            &resp,
            &mut kept,
        );

    let kept_names: Vec<String> = kept.iter().map(|c| c.spec.name.clone()).collect();
    let dropped_names = outcome.dropped_names;

    if kept.len() != 1 {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error":"unexpected_kept_candidate_count",
                "kept": kept_names,
                "dropped": dropped_names,
                "debug": resp.debug,
            })),
        )
            .into_response();
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "selected_backend": kept[0].spec.name,
            "kept": kept_names,
            "dropped": dropped_names,
            "debug": resp.debug,
        })),
    )
        .into_response()
}

impl HttpGateway {
    /// Create new HTTP gateway / 创建新的HTTP网关
    pub fn new(
        config: Arc<SpearletConfig>,
        health_service: Arc<HealthService>,
        function_service: Arc<FunctionServiceImpl>,
        object_client: ObjectServiceClient<Channel>,
        invocation_client: InvocationServiceClient<Channel>,
        execution_client: ExecutionServiceClient<Channel>,
    ) -> Self {
        Self {
            config,
            health_service,
            function_service,
            object_client,
            invocation_client,
            execution_client,
        }
    }

    /// Start HTTP gateway server / 启动HTTP网关服务器
    pub async fn start(self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let (listener, app) = self.prepare().await?;
        axum::serve(listener, app).await?;
        Ok(())
    }

    /// Start HTTP gateway with shutdown signal / 使用关闭信号启动HTTP网关
    pub async fn start_with_shutdown<F>(
        self,
        shutdown: F,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let (listener, app) = self.prepare().await?;
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown)
            .await?;
        Ok(())
    }

    async fn prepare(
        self,
    ) -> Result<(tokio::net::TcpListener, Router), Box<dyn std::error::Error + Send + Sync>> {
        let addr: SocketAddr = self.config.http.server.addr;
        info!("Starting HTTP gateway on {}", addr);

        let state = new_app_state(
            self.object_client,
            self.invocation_client,
            self.execution_client,
            self.health_service,
            self.function_service,
            self.config.clone(),
        );

        let app = build_router(state, self.config.http.swagger_enabled);

        let listener = tokio::net::TcpListener::bind(addr).await?;
        info!("HTTP gateway listening on {}", addr);
        if self.config.http.swagger_enabled {
            info!("Swagger UI available at:");
            info!("  - http://{}/swagger-ui", addr);
            info!("  - http://{}/docs", addr);
            info!("  - OpenAPI JSON: http://{}/api/openapi.json", addr);
        }

        Ok((listener, app))
    }
}

fn system_time_to_rfc3339(t: SystemTime) -> String {
    chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339()
}

fn proto_execution_status_to_str(status: i32) -> &'static str {
    use crate::proto::spearlet::ExecutionStatus;
    match status {
        x if x == ExecutionStatus::Pending as i32 => "PENDING",
        x if x == ExecutionStatus::Running as i32 => "RUNNING",
        x if x == ExecutionStatus::Completed as i32 => "COMPLETED",
        x if x == ExecutionStatus::Failed as i32 => "FAILED",
        x if x == ExecutionStatus::Terminated as i32 => "TERMINATED",
        x if x == ExecutionStatus::Timeout as i32 => "TIMEOUT",
        _ => "UNSPECIFIED",
    }
}

fn task_status_to_public_str(
    status: &crate::spearlet::execution::task::TaskStatus,
) -> &'static str {
    use crate::spearlet::execution::task::TaskStatus;
    match status {
        TaskStatus::Initializing | TaskStatus::Ready => "PENDING",
        TaskStatus::Running | TaskStatus::Paused | TaskStatus::Scaling | TaskStatus::Stopping => {
            "RUNNING"
        }
        TaskStatus::Stopped => "COMPLETED",
        TaskStatus::Error(_) => "FAILED",
    }
}

/// Health check endpoint / 健康检查端点
/// GET /health
async fn health_check(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let health_status = state.health_service.get_health_status().await;

    Ok(Json(serde_json::json!({
        "status": health_status.status,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "service": "spearlet"
    })))
}

/// Status check endpoint / 状态检查端点
/// GET /status
async fn status_check(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let health_status = state.health_service.get_health_status().await;

    Ok(Json(serde_json::json!({
        "status": health_status.status,
        "object_count": health_status.object_count,
        "total_object_size": health_status.total_object_size,
        "pinned_object_count": health_status.pinned_object_count,
        "node_name": state.config.node_name
    })))
}

#[derive(Deserialize)]
struct PutObjectBody {
    value: String, // Base64 encoded / Base64编码
    metadata: Option<HashMap<String, String>>,
    overwrite: Option<bool>,
}

/// Put object endpoint / 存储对象端点
/// PUT /objects/:key
async fn put_object(
    Path(key): Path<String>,
    State(state): State<AppState>,
    Json(body): Json<PutObjectBody>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    debug!("PUT /objects/{}", key);

    // Decode base64 value / 解码base64值
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
async fn get_object(
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
                    // Encode value as base64 / 将值编码为base64
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

#[derive(Deserialize)]
struct ListObjectsQuery {
    prefix: Option<String>,
    limit: Option<i32>,
    continuation_token: Option<String>,
}

/// List objects endpoint / 列出对象端点
/// GET /objects
async fn list_objects(
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
            // Manually build JSON response since proto types don't have serde support
            // 手动构建JSON响应，因为proto类型不支持serde
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

#[derive(Deserialize)]
struct RefCountBody {
    count: Option<i32>,
}

/// Add object reference endpoint / 添加对象引用端点
/// POST /objects/:key/refs
async fn add_object_ref(
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
async fn remove_object_ref(
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
async fn pin_object(
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
async fn unpin_object(
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

#[derive(Deserialize)]
struct DeleteObjectQuery {
    force: Option<bool>,
}

/// Delete object endpoint / 删除对象端点
/// DELETE /objects/:key
async fn delete_object(
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

// Function execution endpoints / 函数执行端点

#[derive(Deserialize)]
struct ExecuteFunctionBody {
    task_id: Option<String>,
    function_name: Option<String>,
    invocation_id: Option<String>,
    execution_id: Option<String>,
    session_id: Option<String>,
    mode: Option<String>,
    timeout_ms: Option<u64>,
    force_new_instance: Option<bool>,
    headers: Option<HashMap<String, String>>,
    environment: Option<HashMap<String, String>>,
    metadata: Option<HashMap<String, String>>,
    input_base64: Option<String>,
    input_content_type: Option<String>,
}

/// Execute function endpoint / 执行函数端点
/// POST /functions/execute
async fn execute_function(
    State(state): State<AppState>,
    Json(body): Json<ExecuteFunctionBody>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    debug!("POST /functions/execute");

    let task_id = body.task_id.unwrap_or_default();
    if task_id.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mode = body.mode.unwrap_or_else(|| "sync".to_string());
    let mode = mode.to_ascii_lowercase();
    let proto_mode = match mode.as_str() {
        "sync" => crate::proto::spearlet::ExecutionMode::Sync as i32,
        "async" => crate::proto::spearlet::ExecutionMode::Async as i32,
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    let mut input_data = Vec::new();
    if let Some(b64) = body.input_base64.as_ref() {
        input_data = general_purpose::STANDARD
            .decode(b64)
            .map_err(|_| StatusCode::BAD_REQUEST)?;
    }

    let req = InvokeRequest {
        invocation_id: body.invocation_id.unwrap_or_default(),
        execution_id: body.execution_id.unwrap_or_default(),
        task_id: task_id.clone(),
        function_name: body.function_name.unwrap_or_default(),
        input: Some(crate::proto::spearlet::Payload {
            content_type: body
                .input_content_type
                .unwrap_or_else(|| "application/octet-stream".to_string()),
            data: input_data,
        }),
        headers: body.headers.unwrap_or_default(),
        environment: body.environment.unwrap_or_default(),
        timeout_ms: body.timeout_ms.unwrap_or(0),
        session_id: body.session_id.unwrap_or_default(),
        mode: proto_mode,
        force_new_instance: body.force_new_instance.unwrap_or(false),
        metadata: body.metadata.unwrap_or_default(),
    };

    let mut client = state.invocation_client.clone();
    match client.invoke(req).await {
        Ok(response) => {
            let resp = response.into_inner();
            let output_b64 = resp
                .output
                .as_ref()
                .map(|p| general_purpose::STANDARD.encode(&p.data))
                .unwrap_or_default();
            Ok(Json(serde_json::json!({
                "success": true,
                "invocation_id": resp.invocation_id,
                "execution_id": resp.execution_id,
                "instance_id": resp.instance_id,
                "status": proto_execution_status_to_str(resp.status),
                "output_base64": output_b64,
                "error": resp.error.map(|e| serde_json::json!({"code": e.code, "message": e.message}))
            })))
        }
        Err(e) => {
            error!("Failed to execute function for task {}: {}", task_id, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Deserialize)]
struct ExecutionStatusQuery {
    include_output: Option<bool>,
}

/// Get execution status endpoint / 获取执行状态端点
/// GET /functions/executions/:execution_id
async fn get_execution_status(
    State(state): State<AppState>,
    Path(execution_id): Path<String>,
    Query(params): Query<ExecutionStatusQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    debug!("GET /functions/executions/{}", execution_id);

    let include_output = params.include_output.unwrap_or(false);
    let req = GetExecutionRequest {
        execution_id: execution_id.clone(),
        include_output,
    };

    let mut client = state.execution_client.clone();
    match client.get_execution(req).await {
        Ok(response) => {
            let exec = response.into_inner();
            let output_b64 = exec
                .output
                .as_ref()
                .map(|p| general_purpose::STANDARD.encode(&p.data))
                .unwrap_or_default();
            Ok(Json(serde_json::json!({
                "execution_id": exec.execution_id,
                "invocation_id": exec.invocation_id,
                "task_id": exec.task_id,
                "function_name": exec.function_name,
                "instance_id": exec.instance_id,
                "status": proto_execution_status_to_str(exec.status),
                "output_base64": output_b64,
                "error": exec.error.map(|e| serde_json::json!({"code": e.code, "message": e.message})),
                "started_at": exec.started_at.map(|t| chrono::DateTime::<chrono::Utc>::from_timestamp(t.seconds, t.nanos as u32).map(|dt| dt.to_rfc3339()).unwrap_or_default()),
                "completed_at": exec.completed_at.map(|t| chrono::DateTime::<chrono::Utc>::from_timestamp(t.seconds, t.nanos as u32).map(|dt| dt.to_rfc3339()).unwrap_or_default())
            })))
        }
        Err(e) => {
            if e.code() == tonic::Code::NotFound {
                return Err(StatusCode::NOT_FOUND);
            }
            error!("Failed to get execution {}: {}", execution_id, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Deserialize)]
struct CancelExecutionBody {
    reason: Option<String>,
}

/// Cancel execution endpoint / 取消执行端点
/// POST /functions/executions/:execution_id/cancel
async fn cancel_execution(
    State(state): State<AppState>,
    Path(execution_id): Path<String>,
    body: Option<Json<CancelExecutionBody>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    debug!("POST /functions/executions/{}/cancel", execution_id);

    let reason = body.and_then(|b| b.0.reason).unwrap_or_default();

    let req = TerminateExecutionRequest {
        execution_id: execution_id.clone(),
        reason,
    };

    let mut client = state.execution_client.clone();
    match client.terminate_execution(req).await {
        Ok(response) => {
            let resp = response.into_inner();
            Ok(Json(serde_json::json!({
                "success": resp.success,
                "execution_id": execution_id,
                "final_status": proto_execution_status_to_str(resp.final_status),
                "message": resp.message
            })))
        }
        Err(e) => {
            error!("Failed to terminate execution {}: {}", execution_id, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// Task management endpoints / 任务管理端点

/// List tasks endpoint / 列出任务端点
/// GET /tasks
async fn list_tasks(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    debug!("GET /tasks");

    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(100)
        .min(1000);
    let offset = params
        .get("offset")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    let filter_status = params.get("status").map(|s| s.to_ascii_uppercase());

    let mgr = state.function_service.get_execution_manager();
    let mut tasks = mgr.list_tasks();
    tasks.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    let mut items: Vec<serde_json::Value> = tasks
        .into_iter()
        .filter_map(|t| {
            let st = task_status_to_public_str(&t.status());
            if let Some(fs) = filter_status.as_ref() {
                if fs != st {
                    return None;
                }
            }
            let metrics = t.metrics.read().clone();
            let updated_at = *t.updated_at.read();
            Some(serde_json::json!({
                "task_id": t.id.clone(),
                "function_name": t.spec.name.clone(),
                "status": st,
                "created_at": system_time_to_rfc3339(t.created_at),
                "updated_at": system_time_to_rfc3339(updated_at),
                "execution_count": metrics.total_executions
            }))
        })
        .collect();

    let total = items.len();
    let has_more = offset.saturating_add(limit) < total;
    if offset >= items.len() {
        items.clear();
    } else {
        items = items
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect::<Vec<_>>();
    }

    Ok(Json(serde_json::json!({
        "tasks": items,
        "total": total,
        "limit": limit,
        "offset": offset,
        "has_more": has_more
    })))
}

/// Get task details endpoint / 获取任务详情端点
/// GET /tasks/:task_id
async fn get_task(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    debug!("GET /tasks/{}", task_id);

    let mgr = state.function_service.get_execution_manager();
    let Some(task) = mgr.get_task_by_id(&task_id) else {
        return Err(StatusCode::NOT_FOUND);
    };

    let st = task_status_to_public_str(&task.status());
    let metrics = task.metrics.read().clone();
    let updated_at = *task.updated_at.read();
    let last_exec = mgr
        .list_executions(Some(&task_id), None, 1)
        .into_iter()
        .next();

    Ok(Json(serde_json::json!({
        "task_id": task.id.clone(),
        "function_name": task.spec.name.clone(),
        "status": st,
        "parameters": task.spec.handler_config.clone(),
        "created_at": system_time_to_rfc3339(task.created_at),
        "updated_at": system_time_to_rfc3339(updated_at),
        "execution_count": metrics.total_executions,
        "last_execution": last_exec.map(|e| serde_json::json!({
            "execution_id": e.execution_id,
            "status": e.status,
            "error": e.error_message
        }))
    })))
}

/// Get task executions endpoint / 获取任务执行记录端点
/// GET /tasks/:task_id/executions
async fn get_task_executions(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    debug!("GET /tasks/{}/executions", task_id);

    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(50)
        .min(500);
    let offset = params
        .get("offset")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);

    let mgr = state.function_service.get_execution_manager();

    let items = mgr.list_executions(Some(&task_id), None, limit.saturating_add(offset));
    let total = items.len();
    let has_more = offset.saturating_add(limit) < total;

    let executions = items
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|e| {
            serde_json::json!({
                "execution_id": e.execution_id,
                "invocation_id": e.invocation_id,
                "task_id": e.task_id,
                "function_name": e.function_name,
                "status": e.status,
                "execution_time_ms": e.execution_time_ms,
                "error": e.error_message,
                "timestamp": system_time_to_rfc3339(e.timestamp)
            })
        })
        .collect::<Vec<_>>();

    Ok(Json(serde_json::json!({
        "task_id": task_id,
        "executions": executions,
        "total": total,
        "limit": limit,
        "offset": offset,
        "has_more": has_more
    })))
}

// Monitoring endpoints / 监控端点

/// Get statistics endpoint / 获取统计信息端点
/// GET /monitoring/stats
async fn get_stats(State(state): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    debug!("GET /monitoring/stats");

    let stats = state.function_service.get_stats().await;
    let exec_stats = state
        .function_service
        .get_execution_manager()
        .get_statistics();

    Ok(Json(serde_json::json!({
        "total_executions": exec_stats.total_executions,
        "successful_executions": exec_stats.successful_executions,
        "failed_executions": exec_stats.failed_executions,
        "active_executions": exec_stats.running_executions,
        "queue_size": exec_stats.queue_size,
        "pending_executions": exec_stats.pending_executions,
        "task_count": stats.task_count,
        "artifact_count": stats.artifact_count,
        "instance_count": stats.instance_count,
        "average_response_time_ms": stats.average_response_time_ms
    })))
}

/// Get current in-process AI backend registry snapshot / 获取当前进程内 AI backend registry 快照
/// GET /monitoring/ai/backends
async fn get_ai_backends(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let holder = engine_holder::global().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let engine = holder.get();
    let snap = engine.debug_snapshot();
    Ok(Json(serde_json::json!({
        "engine_revision": holder.revision(),
        "remote_backend_sync": {
            "enabled": state.config.ai.remote_backend_sync.enabled,
            "poll_interval_ms": state.config.ai.remote_backend_sync.poll_interval_ms,
            "merge_policy": state.config.ai.remote_backend_sync.merge_policy,
        },
        "router": snap,
    })))
}

/// Get current in-process credential sync snapshot / 获取当前进程内 credential 同步快照
/// GET /monitoring/ai/credentials
async fn get_ai_credentials() -> Result<Json<serde_json::Value>, StatusCode> {
    let snapshot = global_credential_sync_status_snapshot();
    Ok(Json(serde_json::json!({
        "credential_sync": snapshot,
    })))
}

/// Get health status endpoint / 获取健康状态端点
/// GET /monitoring/health
async fn get_health_status(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    debug!("GET /monitoring/health");

    let health = state.health_service.get_health_status().await;
    let stats = state.function_service.get_stats().await;

    Ok(Json(serde_json::json!({
        "status": health.status,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "details": {
            "node_name": state.config.node_name,
            "object_count": health.object_count,
            "total_object_size": health.total_object_size,
            "pinned_object_count": health.pinned_object_count,
            "task_count": health.task_count,
            "execution_count": health.execution_count,
            "running_executions": health.running_executions,
            "artifact_count": stats.artifact_count,
            "instance_count": stats.instance_count
        }
    })))
}

/// API documentation endpoint / API文档端点
/// GET /api-docs
async fn api_docs() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "openapi": "3.0.0",
        "info": {
            "title": "SPEARlet API",
            "description": "SPEARlet HTTP Gateway API - SPEAR core agent component / SPEARlet HTTP网关API - SPEAR核心代理组件",
            "version": "0.1.0",
            "contact": {
                "name": "SPEAR Team",
                "url": "https://github.com/spear-ai/spear"
            }
        },
        "servers": [
            {
                "url": "/",
                "description": "Local server / 本地服务器"
            }
        ],
        "tags": [
            {
                "name": "System",
                "description": "System health and status endpoints / 系统健康和状态端点"
            },
            {
                "name": "Objects",
                "description": "Object storage, reference and pinning operations / 对象存储、引用和固定操作"
            },
            {
                "name": "Functions",
                "description": "Function execution and task management / 函数执行和任务管理"
            },
            {
                "name": "Monitoring",
                "description": "Service monitoring and statistics / 服务监控和统计"
            }
        ],
        "paths": {
            "/health": {
                "get": {
                    "tags": ["System"],
                    "summary": "Health check / 健康检查",
                    "description": "Check if the SPEARlet service is healthy / 检查SPEARlet服务是否健康",
                    "responses": {
                        "200": {
                            "description": "Service is healthy / 服务健康",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "service": {
                                                "type": "string",
                                                "example": "spearlet"
                                            },
                                            "status": {
                                                "type": "string",
                                                "example": "healthy"
                                            },
                                            "timestamp": {
                                                "type": "string",
                                                "format": "date-time",
                                                "example": "2024-01-01T00:00:00Z"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/status": {
                "get": {
                    "tags": ["System"],
                    "summary": "Get node status / 获取节点状态",
                    "description": "Get detailed status information about the SPEARlet node / 获取SPEARlet节点的详细状态信息",
                    "responses": {
                        "200": {
                            "description": "Node status information / 节点状态信息",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "node_name": {"type": "string"},
                                            "status": {"type": "string"},
                                            "object_count": {"type": "integer"},
                                            "total_object_size": {"type": "integer"},
                                            "pinned_object_count": {"type": "integer"}
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/objects": {
                "get": {
                    "tags": ["Objects"],
                    "summary": "List objects / 列出对象",
                    "description": "List all stored objects / 列出所有存储的对象",
                    "parameters": [
                        {
                            "name": "prefix",
                            "in": "query",
                            "description": "Filter objects by prefix / 按前缀过滤对象",
                            "schema": {"type": "string"}
                        },
                        {
                            "name": "limit",
                            "in": "query",
                            "description": "Maximum number of objects to return / 返回对象的最大数量",
                            "schema": {
                                "type": "integer",
                                "minimum": 1,
                                "maximum": 1000,
                                "default": 100
                            }
                        },
                        {
                            "name": "continuation_token",
                            "in": "query",
                            "description": "Token for pagination / 分页令牌",
                            "schema": {"type": "string"}
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "List of objects / 对象列表",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "objects": {
                                                "type": "array",
                                                "items": {
                                                    "type": "object",
                                                    "properties": {
                                                        "key": {"type": "string"},
                                                        "size": {"type": "integer"},
                                                        "created_at": {"type": "string", "format": "date-time"},
                                                        "updated_at": {"type": "string", "format": "date-time"},
                                                        "ref_count": {"type": "integer"},
                                                        "is_pinned": {"type": "boolean"}
                                                    }
                                                }
                                            },
                                            "continuation_token": {"type": "string"},
                                            "has_more": {"type": "boolean"}
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/objects/{key}": {
                "put": {
                    "tags": ["Objects"],
                    "summary": "Store object / 存储对象",
                    "description": "Store an object with the specified key / 使用指定键存储对象",
                    "parameters": [
                        {
                            "name": "key",
                            "in": "path",
                            "required": true,
                            "description": "Object key / 对象键",
                            "schema": {"type": "string"}
                        }
                    ],
                    "requestBody": {
                        "description": "Object data / 对象数据",
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "properties": {
                                        "value": {
                                            "type": "string",
                                            "description": "Base64 encoded object value / Base64编码的对象值"
                                        },
                                        "metadata": {
                                            "type": "object",
                                            "additionalProperties": {"type": "string"},
                                            "description": "Object metadata / 对象元数据"
                                        },
                                        "overwrite": {
                                            "type": "boolean",
                                            "default": false,
                                            "description": "Whether to overwrite existing object / 是否覆盖现有对象"
                                        }
                                    },
                                    "required": ["value"]
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Object stored successfully / 对象存储成功",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "success": {"type": "boolean"},
                                            "message": {"type": "string"},
                                            "key": {"type": "string"}
                                        }
                                    }
                                }
                            }
                        },
                        "400": {"description": "Bad request / 请求错误"}
                    }
                },
                "get": {
                    "tags": ["Objects"],
                    "summary": "Get object / 获取对象",
                    "description": "Retrieve an object by its key / 通过键检索对象",
                    "parameters": [
                        {
                            "name": "key",
                            "in": "path",
                            "required": true,
                            "description": "Object key / 对象键",
                            "schema": {"type": "string"}
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Object data / 对象数据",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "found": {"type": "boolean"},
                                            "key": {"type": "string"},
                                            "value": {"type": "string", "description": "Base64 encoded value"},
                                            "metadata": {"type": "object"},
                                            "size": {"type": "integer"},
                                            "created_at": {"type": "string", "format": "date-time"},
                                            "updated_at": {"type": "string", "format": "date-time"},
                                            "ref_count": {"type": "integer"},
                                            "pinned": {"type": "boolean"}
                                        }
                                    }
                                }
                            }
                        },
                        "404": {"description": "Object not found / 对象未找到"}
                    }
                },
                "delete": {
                    "tags": ["Objects"],
                    "summary": "Delete object / 删除对象",
                    "description": "Delete an object by its key / 通过键删除对象",
                    "parameters": [
                        {
                            "name": "key",
                            "in": "path",
                            "required": true,
                            "description": "Object key / 对象键",
                            "schema": {"type": "string"}
                        },
                        {
                            "name": "force",
                            "in": "query",
                            "description": "Force delete even if object has references / 即使对象有引用也强制删除",
                            "schema": {"type": "boolean", "default": false}
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Object deleted successfully / 对象删除成功",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "success": {"type": "boolean"},
                                            "message": {"type": "string"}
                                        }
                                    }
                                }
                            }
                        },
                        "404": {"description": "Object not found / 对象未找到"}
                    }
                }
            },
            "/objects/{key}/refs": {
                "post": {
                    "tags": ["Objects"],
                    "summary": "Add object reference / 添加对象引用",
                    "description": "Add a reference to an object / 为对象添加引用",
                    "parameters": [
                        {
                            "name": "key",
                            "in": "path",
                            "required": true,
                            "description": "Object key / 对象键",
                            "schema": {"type": "string"}
                        }
                    ],
                    "requestBody": {
                        "description": "Reference count / 引用计数",
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "properties": {
                                        "count": {"type": "integer", "default": 1}
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {"description": "Reference added / 引用已添加"},
                        "404": {"description": "Object not found / 对象未找到"}
                    }
                },
                "delete": {
                    "tags": ["Objects"],
                    "summary": "Remove object reference / 移除对象引用",
                    "description": "Remove a reference from an object / 从对象移除引用",
                    "parameters": [
                        {
                            "name": "key",
                            "in": "path",
                            "required": true,
                            "description": "Object key / 对象键",
                            "schema": {"type": "string"}
                        }
                    ],
                    "requestBody": {
                        "description": "Reference count to remove / 要移除的引用计数",
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "properties": {
                                        "count": {"type": "integer", "default": 1}
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {"description": "Reference removed / 引用已移除"},
                        "404": {"description": "Object not found / 对象未找到"}
                    }
                }
            },
            "/objects/{key}/pin": {
                "post": {
                    "tags": ["Objects"],
                    "summary": "Pin object / 固定对象",
                    "description": "Pin an object to prevent it from being garbage collected / 固定对象以防止被垃圾回收",
                    "parameters": [
                        {
                            "name": "key",
                            "in": "path",
                            "required": true,
                            "description": "Object key / 对象键",
                            "schema": {"type": "string"}
                        }
                    ],
                    "responses": {
                        "200": {"description": "Object pinned / 对象已固定"},
                        "404": {"description": "Object not found / 对象未找到"}
                    }
                },
                "delete": {
                    "tags": ["Objects"],
                    "summary": "Unpin object / 取消固定对象",
                    "description": "Unpin an object to allow it to be garbage collected / 取消固定对象以允许被垃圾回收",
                    "parameters": [
                        {
                            "name": "key",
                            "in": "path",
                            "required": true,
                            "description": "Object key / 对象键",
                            "schema": {"type": "string"}
                        }
                    ],
                    "responses": {
                        "200": {"description": "Object unpinned / 对象已取消固定"},
                        "404": {"description": "Object not found / 对象未找到"}
                    }
                }
            },
            "/functions/invoke": {
                "post": {
                    "tags": ["Functions"],
                    "summary": "Invoke function / 调用函数",
                    "description": "Execute a function with specified parameters / 使用指定参数执行函数",
                    "requestBody": {
                        "description": "Function invocation request / 函数调用请求",
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "required": ["function_name"],
                                    "properties": {
                                        "function_name": {
                                            "type": "string",
                                            "description": "Name of the function to invoke / 要调用的函数名称"
                                        },
                                        "parameters": {
                                            "type": "object",
                                            "description": "Function parameters / 函数参数",
                                            "additionalProperties": true
                                        },
                                        "invocation_type": {
                                            "type": "string",
                                            "enum": ["SYNC", "ASYNC"],
                                            "default": "SYNC",
                                            "description": "Invocation type / 调用类型"
                                        },
                                        "execution_mode": {
                                            "type": "string",
                                            "enum": ["NORMAL", "DEBUG"],
                                            "default": "NORMAL",
                                            "description": "Execution mode / 执行模式"
                                        },
                                        "timeout_seconds": {
                                            "type": "integer",
                                            "description": "Execution timeout in seconds / 执行超时时间（秒）"
                                        }
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Function execution result / 函数执行结果",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "execution_id": {"type": "string"},
                                            "status": {"type": "string"},
                                            "result": {"type": "object"},
                                            "error": {"type": "string"},
                                            "execution_time_ms": {"type": "integer"}
                                        }
                                    }
                                }
                            }
                        },
                        "400": {"description": "Invalid request / 无效请求"},
                        "500": {"description": "Execution error / 执行错误"}
                    }
                }
            },
            "/functions/executions/{execution_id}/status": {
                "get": {
                    "tags": ["Functions"],
                    "summary": "Get execution status / 获取执行状态",
                    "description": "Get the status of a function execution / 获取函数执行的状态",
                    "parameters": [
                        {
                            "name": "execution_id",
                            "in": "path",
                            "required": true,
                            "description": "Execution ID / 执行ID",
                            "schema": {"type": "string"}
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Execution status / 执行状态",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "execution_id": {"type": "string"},
                                            "status": {
                                                "type": "string",
                                                "enum": ["PENDING", "RUNNING", "COMPLETED", "FAILED", "CANCELLED"]
                                            },
                                            "result": {"type": "object"},
                                            "error": {"type": "string"},
                                            "start_time": {"type": "string", "format": "date-time"},
                                            "end_time": {"type": "string", "format": "date-time"},
                                            "execution_time_ms": {"type": "integer"}
                                        }
                                    }
                                }
                            }
                        },
                        "404": {"description": "Execution not found / 执行未找到"}
                    }
                }
            },
            "/functions/executions/{execution_id}/cancel": {
                "post": {
                    "tags": ["Functions"],
                    "summary": "Cancel execution / 取消执行",
                    "description": "Cancel a running function execution / 取消正在运行的函数执行",
                    "parameters": [
                        {
                            "name": "execution_id",
                            "in": "path",
                            "required": true,
                            "description": "Execution ID / 执行ID",
                            "schema": {"type": "string"}
                        }
                    ],
                    "responses": {
                        "200": {"description": "Execution cancelled / 执行已取消"},
                        "404": {"description": "Execution not found / 执行未找到"},
                        "409": {"description": "Cannot cancel execution / 无法取消执行"}
                    }
                }
            },
            "/functions/stream": {
                "post": {
                    "tags": ["Functions"],
                    "summary": "Stream function execution / 流式函数执行",
                    "description": "Execute a function with streaming results / 执行函数并流式返回结果",
                    "requestBody": {
                        "description": "Function streaming request / 函数流式请求",
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "required": ["function_name"],
                                    "properties": {
                                        "function_name": {"type": "string"},
                                        "parameters": {"type": "object", "additionalProperties": true},
                                        "execution_mode": {
                                            "type": "string",
                                            "enum": ["NORMAL", "DEBUG"],
                                            "default": "NORMAL"
                                        }
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Streaming execution results / 流式执行结果",
                            "content": {
                                "text/event-stream": {
                                    "schema": {
                                        "type": "string",
                                        "description": "Server-sent events stream / 服务器发送事件流"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/tasks": {
                "get": {
                    "tags": ["Functions"],
                    "summary": "List tasks / 列出任务",
                    "description": "Get a list of all tasks / 获取所有任务的列表",
                    "parameters": [
                        {
                            "name": "limit",
                            "in": "query",
                            "description": "Maximum number of tasks to return / 返回的最大任务数",
                            "schema": {"type": "integer", "default": 100}
                        },
                        {
                            "name": "offset",
                            "in": "query",
                            "description": "Number of tasks to skip / 跳过的任务数",
                            "schema": {"type": "integer", "default": 0}
                        },
                        {
                            "name": "status",
                            "in": "query",
                            "description": "Filter by task status / 按任务状态过滤",
                            "schema": {
                                "type": "string",
                                "enum": ["PENDING", "RUNNING", "COMPLETED", "FAILED", "CANCELLED"]
                            }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "List of tasks / 任务列表",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "tasks": {
                                                "type": "array",
                                                "items": {
                                                    "type": "object",
                                                    "properties": {
                                                        "task_id": {"type": "string"},
                                                        "function_name": {"type": "string"},
                                                        "status": {"type": "string"},
                                                        "created_at": {"type": "string", "format": "date-time"},
                                                        "updated_at": {"type": "string", "format": "date-time"},
                                                        "execution_count": {"type": "integer"}
                                                    }
                                                }
                                            },
                                            "total": {"type": "integer"},
                                            "limit": {"type": "integer"},
                                            "offset": {"type": "integer"}
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/tasks/{task_id}": {
                "get": {
                    "tags": ["Functions"],
                    "summary": "Get task details / 获取任务详情",
                    "description": "Get detailed information about a specific task / 获取特定任务的详细信息",
                    "parameters": [
                        {
                            "name": "task_id",
                            "in": "path",
                            "required": true,
                            "description": "Task ID / 任务ID",
                            "schema": {"type": "string"}
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Task details / 任务详情",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "task_id": {"type": "string"},
                                            "function_name": {"type": "string"},
                                            "status": {"type": "string"},
                                            "parameters": {"type": "object"},
                                            "created_at": {"type": "string", "format": "date-time"},
                                            "updated_at": {"type": "string", "format": "date-time"},
                                            "execution_count": {"type": "integer"},
                                            "last_execution": {
                                                "type": "object",
                                                "properties": {
                                                    "execution_id": {"type": "string"},
                                                    "status": {"type": "string"},
                                                    "result": {"type": "object"},
                                                    "error": {"type": "string"}
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        },
                        "404": {"description": "Task not found / 任务未找到"}
                    }
                },
                "delete": {
                    "tags": ["Functions"],
                    "summary": "Delete task / 删除任务",
                    "description": "Delete a specific task / 删除特定任务",
                    "parameters": [
                        {
                            "name": "task_id",
                            "in": "path",
                            "required": true,
                            "description": "Task ID / 任务ID",
                            "schema": {"type": "string"}
                        }
                    ],
                    "responses": {
                        "200": {"description": "Task deleted / 任务已删除"},
                        "404": {"description": "Task not found / 任务未找到"},
                        "409": {"description": "Cannot delete running task / 无法删除正在运行的任务"}
                    }
                }
            },
            "/tasks/{task_id}/executions": {
                "get": {
                    "tags": ["Functions"],
                    "summary": "List task executions / 列出任务执行记录",
                    "description": "Get execution history for a specific task / 获取特定任务的执行历史",
                    "parameters": [
                        {
                            "name": "task_id",
                            "in": "path",
                            "required": true,
                            "description": "Task ID / 任务ID",
                            "schema": {"type": "string"}
                        },
                        {
                            "name": "limit",
                            "in": "query",
                            "description": "Maximum number of executions to return / 返回的最大执行记录数",
                            "schema": {"type": "integer", "default": 50}
                        },
                        {
                            "name": "offset",
                            "in": "query",
                            "description": "Number of executions to skip / 跳过的执行记录数",
                            "schema": {"type": "integer", "default": 0}
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "List of task executions / 任务执行记录列表",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "executions": {
                                                "type": "array",
                                                "items": {
                                                    "type": "object",
                                                    "properties": {
                                                        "execution_id": {"type": "string"},
                                                        "status": {"type": "string"},
                                                        "start_time": {"type": "string", "format": "date-time"},
                                                        "end_time": {"type": "string", "format": "date-time"},
                                                        "execution_time_ms": {"type": "integer"},
                                                        "result": {"type": "object"},
                                                        "error": {"type": "string"}
                                                    }
                                                }
                                            },
                                            "total": {"type": "integer"},
                                            "limit": {"type": "integer"},
                                            "offset": {"type": "integer"}
                                        }
                                    }
                                }
                            }
                        },
                        "404": {"description": "Task not found / 任务未找到"}
                    }
                }
            },
            "/functions/health": {
                "get": {
                    "tags": ["Monitoring"],
                    "summary": "Get function service health / 获取函数服务健康状态",
                    "description": "Get detailed health information about the function service / 获取函数服务的详细健康信息",
                    "responses": {
                        "200": {
                            "description": "Function service health status / 函数服务健康状态",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "service": {"type": "string", "example": "function_service"},
                                            "status": {
                                                "type": "string",
                                                "enum": ["HEALTHY", "UNHEALTHY", "DEGRADED"],
                                                "example": "HEALTHY"
                                            },
                                            "timestamp": {"type": "string", "format": "date-time"},
                                            "details": {
                                                "type": "object",
                                                "properties": {
                                                    "active_executions": {"type": "integer"},
                                                    "pending_tasks": {"type": "integer"},
                                                    "total_memory_usage": {"type": "integer"},
                                                    "cpu_usage_percent": {"type": "number"},
                                                    "uptime_seconds": {"type": "integer"}
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/functions/stats": {
                "get": {
                    "tags": ["Monitoring"],
                    "summary": "Get function service statistics / 获取函数服务统计信息",
                    "description": "Get comprehensive statistics about function executions and performance / 获取函数执行和性能的综合统计信息",
                    "responses": {
                        "200": {
                            "description": "Function service statistics / 函数服务统计信息",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "service_stats": {
                                                "type": "object",
                                                "properties": {
                                                    "total_executions": {"type": "integer"},
                                                    "successful_executions": {"type": "integer"},
                                                    "failed_executions": {"type": "integer"},
                                                    "average_execution_time_ms": {"type": "number"},
                                                    "total_execution_time_ms": {"type": "integer"},
                                                    "active_executions": {"type": "integer"},
                                                    "peak_concurrent_executions": {"type": "integer"}
                                                }
                                            },
                                            "task_stats": {
                                                "type": "object",
                                                "properties": {
                                                    "total_tasks": {"type": "integer"},
                                                    "active_tasks": {"type": "integer"},
                                                    "completed_tasks": {"type": "integer"},
                                                    "failed_tasks": {"type": "integer"},
                                                    "average_task_duration_ms": {"type": "number"}
                                                }
                                            },
                                            "execution_stats": {
                                                "type": "object",
                                                "properties": {
                                                    "executions_per_minute": {"type": "number"},
                                                    "success_rate_percent": {"type": "number"},
                                                    "average_queue_time_ms": {"type": "number"},
                                                    "memory_usage_mb": {"type": "number"},
                                                    "cpu_usage_percent": {"type": "number"}
                                                }
                                            },
                                            "timestamp": {"type": "string", "format": "date-time"}
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }))
}

/// Swagger UI HTML page / Swagger UI HTML页面
async fn swagger_ui() -> impl IntoResponse {
    let html = r#"
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>SPEARlet API Documentation</title>
    <link rel="stylesheet" type="text/css" href="https://unpkg.com/swagger-ui-dist@5.9.0/swagger-ui.css" />
    <style>
        html {
            box-sizing: border-box;
            overflow: -moz-scrollbars-vertical;
            overflow-y: scroll;
        }
        *, *:before, *:after {
            box-sizing: inherit;
        }
        body {
            margin:0;
            background: #fafafa;
        }
        .swagger-ui .topbar {
            background-color: #1f2937;
        }
        .swagger-ui .topbar .download-url-wrapper {
            display: none;
        }
    </style>
</head>
<body>
    <div id="swagger-ui"></div>
    <script src="https://unpkg.com/swagger-ui-dist@5.9.0/swagger-ui-bundle.js"></script>
    <script src="https://unpkg.com/swagger-ui-dist@5.9.0/swagger-ui-standalone-preset.js"></script>
    <script>
        window.onload = function() {
            const ui = SwaggerUIBundle({
                url: '/api/openapi.json',
                dom_id: '#swagger-ui',
                deepLinking: true,
                presets: [
                    SwaggerUIBundle.presets.apis,
                    SwaggerUIStandalonePreset
                ],
                plugins: [
                    SwaggerUIBundle.plugins.DownloadUrl
                ],
                layout: "StandaloneLayout",
                validatorUrl: null,
                docExpansion: "list",
                defaultModelsExpandDepth: 1,
                defaultModelExpandDepth: 1,
                displayRequestDuration: true,
                tryItOutEnabled: true,
                filter: true,
                showExtensions: true,
                showCommonExtensions: true
            });
        };
    </script>
</body>
</html>
    "#;

    Html(html)
}
