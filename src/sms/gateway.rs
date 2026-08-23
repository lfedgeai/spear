//! HTTP gateway for SPEAR Metadata Server gRPC service
//! SPEAR元数据服务器gRPC服务的HTTP网关

use axum::Router;
use tokio_util::sync::CancellationToken;
use tower_http::cors::CorsLayer;

use super::routes::create_routes;
use crate::proto::sms::{
    admin_credential_service_client::AdminCredentialServiceClient,
    ai_backend_control_plane_service_client::AiBackendControlPlaneServiceClient,
    backend_registry_service_client::BackendRegistryServiceClient,
    execution_index_service_client::ExecutionIndexServiceClient,
    execution_registry_service_client::ExecutionRegistryServiceClient,
    instance_registry_service_client::InstanceRegistryServiceClient,
    mcp_registry_service_client::McpRegistryServiceClient, node_service_client::NodeServiceClient,
    placement_service_client::PlacementServiceClient,
    task_placement_assignment_service_client::TaskPlacementAssignmentServiceClient,
    task_service_client::TaskServiceClient,
};

use dashmap::DashMap;
use futures::{SinkExt, StreamExt};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::OnceCell;
use uuid::Uuid;

/// Stream session record / 流会话记录
#[derive(Clone, Debug)]
pub struct StreamSession {
    pub execution_id: String,
    pub expires_at: Instant,
}

/// Stream session store / 流会话存储
#[derive(Clone, Debug)]
pub struct StreamSessionStore {
    sessions: Arc<DashMap<String, StreamSession>>,
}

impl Default for StreamSessionStore {
    fn default() -> Self {
        Self {
            sessions: Arc::new(DashMap::new()),
        }
    }
}

impl StreamSessionStore {
    /// Create a new store / 创建新的存储
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a token / 写入 token
    pub fn insert(&self, token: String, execution_id: String, ttl: Duration) {
        self.sessions.insert(
            token,
            StreamSession {
                execution_id,
                expires_at: Instant::now() + ttl,
            },
        );
    }

    /// Validate token and return execution_id / 校验 token 并返回 execution_id
    pub fn validate(&self, token: &str) -> Option<String> {
        let now = Instant::now();
        let entry = self.sessions.get(token)?;
        if entry.expires_at <= now {
            drop(entry);
            self.sessions.remove(token);
            return None;
        }
        Some(entry.execution_id.clone())
    }

    /// Remove token / 删除 token
    pub fn remove(&self, token: &str) {
        self.sessions.remove(token);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_session_store_validate_and_expire() {
        let store = StreamSessionStore::new();
        store.insert(
            "t1".to_string(),
            "exec-1".to_string(),
            Duration::from_millis(0),
        );
        assert!(store.validate("t1").is_none());

        store.insert(
            "t2".to_string(),
            "exec-2".to_string(),
            Duration::from_secs(60),
        );
        assert_eq!(store.validate("t2").unwrap(), "exec-2");
        store.remove("t2");
        assert!(store.validate("t2").is_none());
    }
}

#[derive(Clone, Debug, Default)]
pub struct ExecutionStreamPool {
    hubs: Arc<DashMap<String, Arc<ExecutionStreamHub>>>,
}

#[derive(Debug)]
struct ExecutionStreamHub {
    execution_id: String,
    router: crate::sms::stream_mux::ExecutionStreamRouter,
    clients: DashMap<String, tokio::sync::mpsc::UnboundedSender<axum::extract::ws::Message>>,
    started: OnceCell<tokio::sync::mpsc::UnboundedSender<tokio_tungstenite::tungstenite::Message>>,
    healthy: std::sync::atomic::AtomicBool,
    cancel: CancellationToken,
}

impl ExecutionStreamPool {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn register_client(
        &self,
        state: &GatewayState,
        execution_id: &str,
        out_tx: tokio::sync::mpsc::UnboundedSender<axum::extract::ws::Message>,
    ) -> Result<String, String> {
        let client_id = Uuid::new_v4().to_string();
        let hub = self.get_or_create_hub(execution_id);
        hub.ensure_started(state).await?;
        hub.clients.insert(client_id.clone(), out_tx);
        Ok(client_id)
    }

    pub async fn forward_client_binary(
        &self,
        state: &GatewayState,
        execution_id: &str,
        client_id: &str,
        frame: &[u8],
    ) -> Result<(), String> {
        let hub = self.get_or_create_hub(execution_id);
        hub.ensure_started(state).await?;
        hub.forward_client_binary(client_id, frame).await
    }

    pub async fn unregister_client(&self, execution_id: &str, client_id: &str) {
        let Some(hub) = self.hubs.get(execution_id).map(|e| e.clone()) else {
            return;
        };
        hub.clients.remove(client_id);
        hub.router.remove_client(client_id).await;
        if hub.clients.is_empty() {
            hub.cancel.cancel();
            self.hubs.remove(execution_id);
        }
    }

    fn get_or_create_hub(&self, execution_id: &str) -> Arc<ExecutionStreamHub> {
        loop {
            if let Some(existing) = self.hubs.get(execution_id).map(|e| e.clone()) {
                if existing.healthy.load(std::sync::atomic::Ordering::Relaxed) {
                    return existing;
                }
                self.hubs.remove(execution_id);
            }
            let hub = Arc::new(ExecutionStreamHub {
                execution_id: execution_id.to_string(),
                router: crate::sms::stream_mux::ExecutionStreamRouter::new(),
                clients: DashMap::new(),
                started: OnceCell::new(),
                healthy: std::sync::atomic::AtomicBool::new(true),
                cancel: CancellationToken::new(),
            });
            if self
                .hubs
                .insert(execution_id.to_string(), hub.clone())
                .is_none()
            {
                return hub;
            }
        }
    }
}

impl ExecutionStreamHub {
    async fn ensure_started(self: &Arc<Self>, state: &GatewayState) -> Result<(), String> {
        if !self.healthy.load(std::sync::atomic::Ordering::Relaxed) {
            return Err("upstream not healthy".to_string());
        }
        let hub = Arc::clone(self);
        self.started
            .get_or_try_init(|| {
                let hub = Arc::clone(&hub);
                async move {
                    let target_ws = crate::sms::handlers::stream::resolve_spearlet_ws_url(state, &hub.execution_id).await?;
                let (upstream, _) = tokio_tungstenite::connect_async(&target_ws)
                    .await
                    .map_err(|e| {
                        let mut hint = String::new();
                        if let Ok(u) = url::Url::parse(&target_ws) {
                            if let Some(host) = u.host_str() {
                                if host == "0.0.0.0" || host == "::" {
                                    hint = " (hint: upstream host is 0.0.0.0/::; spearlet advertised an unspecified IP; set SPEARLET_ADVERTISE_IP or POD_IP)"
                                        .to_string();
                                }
                            }
                        }
                        match e {
                            tokio_tungstenite::tungstenite::Error::Http(resp) => {
                                let status = resp.status();
                                let server = resp
                                    .headers()
                                    .get("server")
                                    .and_then(|v| v.to_str().ok())
                                    .unwrap_or("");
                                if server.is_empty() {
                                    format!(
                                        "connect upstream failed (url={target_ws}): HTTP {status}{hint}"
                                    )
                                } else {
                                    format!(
                                        "connect upstream failed (url={target_ws}): HTTP {status} (server={server}){hint}"
                                    )
                                }
                            }
                            other => format!("connect upstream failed (url={target_ws}): {other}{hint}"),
                        }
                    })?;
                let (mut up_tx, mut up_rx) = upstream.split();
                let (write_tx, mut write_rx) =
                    tokio::sync::mpsc::unbounded_channel::<tokio_tungstenite::tungstenite::Message>();
                let cancel = hub.cancel.clone();
                tokio::spawn(async move {
                    loop {
                        tokio::select! {
                            _ = cancel.cancelled() => break,
                            msg = write_rx.recv() => {
                                let Some(msg) = msg else { break; };
                                if up_tx.send(msg).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                });

                let hub2 = Arc::clone(&hub);
                tokio::spawn(async move {
                    let exit_reason = loop {
                        tokio::select! {
                            _ = hub2.cancel.cancelled() => {
                                break "hub_cancelled".to_string()
                            },
                            msg = up_rx.next() => {
                                let Some(msg) = msg else {
                                    break "upstream_ws_eof".to_string();
                                };
                                let Ok(msg) = msg else {
                                    break "upstream_ws_error".to_string();
                                };
                                match msg {
                                    tokio_tungstenite::tungstenite::Message::Binary(b) => {
                                        if let Ok(Some((client_id, frame))) = hub2.router.route_upstream_to_client(&b).await {
                                            if let Some(tx) = hub2.clients.get(&client_id).map(|e| e.value().clone()) {
                                                let _ = tx.send(axum::extract::ws::Message::Binary(prost::bytes::Bytes::from(frame)));
                                            }
                                        }
                                    }
                                    tokio_tungstenite::tungstenite::Message::Close(_) => {
                                        break "upstream_close_frame".to_string()
                                    },
                                    _ => {}
                                }
                            }
                        }
                    };
                    tracing::warn!(
                        execution_id = %hub2.execution_id,
                        exit_reason = %exit_reason,
                        client_count = hub2.clients.len(),
                        "execution_stream_hub upstream loop stopping"
                    );
                    hub2.healthy.store(false, std::sync::atomic::Ordering::Relaxed);
                    for entry in hub2.clients.iter() {
                        let _ = entry
                            .value()
                            .send(axum::extract::ws::Message::Close(None));
                    }
                    hub2.cancel.cancel();
                });

                Ok::<_, String>(write_tx)
            }
            })
            .await?;
        Ok(())
    }

    async fn forward_client_binary(&self, client_id: &str, frame: &[u8]) -> Result<(), String> {
        let Some(write_tx) = self.started.get() else {
            return Err("upstream not ready".to_string());
        };
        let rewritten = self
            .router
            .route_client_to_upstream(client_id, frame)
            .await?;
        write_tx
            .send(tokio_tungstenite::tungstenite::Message::Binary(rewritten))
            .map_err(|_| "upstream send failed".to_string())?;
        Ok(())
    }
}

/// HTTP gateway state / HTTP网关状态
#[derive(Clone, Debug)]
pub struct GatewayState {
    pub config: Arc<crate::sms::config::SmsConfig>,
    pub node_client: NodeServiceClient<tonic::transport::Channel>,
    pub task_client: TaskServiceClient<tonic::transport::Channel>,
    pub task_assignment_client: TaskPlacementAssignmentServiceClient<tonic::transport::Channel>,
    pub placement_client: PlacementServiceClient<tonic::transport::Channel>,
    pub instance_registry_client: InstanceRegistryServiceClient<tonic::transport::Channel>,
    pub execution_registry_client: ExecutionRegistryServiceClient<tonic::transport::Channel>,
    pub execution_index_client: ExecutionIndexServiceClient<tonic::transport::Channel>,
    pub mcp_registry_client: McpRegistryServiceClient<tonic::transport::Channel>,
    pub backend_registry_client: BackendRegistryServiceClient<tonic::transport::Channel>,
    pub ai_backend_control_plane_client:
        AiBackendControlPlaneServiceClient<tonic::transport::Channel>,
    pub admin_credential_client: AdminCredentialServiceClient<tonic::transport::Channel>,
    /// Stream sessions for WS proxy / WS 代理的流会话
    pub stream_sessions: StreamSessionStore,
    pub execution_stream_pool: ExecutionStreamPool,
    pub cancel_token: CancellationToken,
    pub max_upload_bytes: usize,
    pub files_dir: String,
}

/// Create HTTP gateway router / 创建HTTP网关路由器
pub fn create_gateway_router(state: GatewayState) -> Router {
    // Use the centralized route creation function / 使用集中的路由创建函数
    create_routes(state).layer(CorsLayer::permissive()) // Add CORS support / 添加CORS支持
}
