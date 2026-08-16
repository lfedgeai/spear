//! HTTP gateway implementation for SMS (SPEAR Metadata Server)
//! SMS（SPEAR元数据服务器）的HTTP网关实现

use anyhow::Result;
use std::net::SocketAddr;
use std::sync::Arc;

use tracing::{error, info};

use super::gateway::{create_gateway_router, GatewayState};
use crate::proto::sms::{
    admin_credential_service_client::AdminCredentialServiceClient,
    backend_registry_service_client::BackendRegistryServiceClient,
    execution_index_service_client::ExecutionIndexServiceClient,
    execution_registry_service_client::ExecutionRegistryServiceClient,
    instance_registry_service_client::InstanceRegistryServiceClient,
    mcp_registry_service_client::McpRegistryServiceClient,
    node_service_client::NodeServiceClient, placement_service_client::PlacementServiceClient,
    task_service_client::TaskServiceClient,
};
use tokio_util::sync::CancellationToken;

/// SMS HTTP gateway / SMS HTTP网关
pub struct HttpGateway {
    config: Arc<crate::sms::config::SmsConfig>,
}

impl HttpGateway {
    /// Create a new HTTP gateway / 创建新的HTTP网关
    pub fn new(config: Arc<crate::sms::config::SmsConfig>) -> Self {
        Self { config }
    }

    /// Get the HTTP address / 获取HTTP地址
    pub fn addr(&self) -> SocketAddr {
        self.config.http.addr
    }

    /// Get the gRPC address / 获取gRPC地址
    pub fn grpc_addr(&self) -> SocketAddr {
        self.config.grpc.addr
    }

    /// Check if Swagger is enabled / 检查是否启用Swagger
    pub fn enable_swagger(&self) -> bool {
        self.config.enable_swagger
    }

    /// Start the HTTP gateway / 启动HTTP网关
    pub async fn start(self) -> Result<()> {
        let (listener, app) = self.prepare().await?;
        if let Err(e) = axum::serve(listener, app).await {
            error!("SMS HTTP gateway error: {}", e);
            return Err(e.into());
        }
        Ok(())
    }

    /// Start HTTP gateway with shutdown signal / 使用关闭信号启动HTTP网关
    pub async fn start_with_shutdown<F>(self, shutdown: F) -> Result<()>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let (listener, app) = self.prepare().await?;
        if let Err(e) = axum::serve(listener, app)
            .with_graceful_shutdown(shutdown)
            .await
        {
            error!("SMS HTTP gateway error: {}", e);
            return Err(e.into());
        }
        Ok(())
    }

    async fn prepare(self) -> Result<(tokio::net::TcpListener, axum::Router)> {
        info!("Starting SMS HTTP gateway on {}", self.config.http.addr);
        info!("Connecting to gRPC server at {}", self.config.grpc.addr);

        let grpc_url = format!("http://{}", self.config.grpc.addr);
        let channel = tonic::transport::Channel::from_shared(grpc_url)
            .expect("Invalid gRPC URL")
            .connect_lazy();
        let node_client = NodeServiceClient::new(channel.clone());
        let task_client = TaskServiceClient::new(channel.clone());
        let task_assignment_client = crate::proto::sms::task_placement_assignment_service_client::TaskPlacementAssignmentServiceClient::new(channel.clone());
        let placement_client = PlacementServiceClient::new(channel.clone());
        let instance_registry_client = InstanceRegistryServiceClient::new(channel.clone());
        let execution_registry_client = ExecutionRegistryServiceClient::new(channel.clone());
        let execution_index_client = ExecutionIndexServiceClient::new(channel.clone());
        let mcp_registry_client = McpRegistryServiceClient::new(channel.clone());
        let backend_registry_client = BackendRegistryServiceClient::new(channel.clone());
        let ai_backend_control_plane_client =
            crate::proto::sms::ai_backend_control_plane_service_client::AiBackendControlPlaneServiceClient::new(channel.clone());
        let admin_credential_client = AdminCredentialServiceClient::new(channel.clone());

        let state = GatewayState {
            config: self.config.clone(),
            node_client,
            task_client,
            task_assignment_client,
            placement_client,
            instance_registry_client,
            execution_registry_client,
            execution_index_client,
            mcp_registry_client,
            backend_registry_client,
            ai_backend_control_plane_client,
            admin_credential_client,
            stream_sessions: super::gateway::StreamSessionStore::new(),
            execution_stream_pool: super::gateway::ExecutionStreamPool::new(),
            cancel_token: CancellationToken::new(),
            max_upload_bytes: self.config.max_upload_bytes as usize,
            files_dir: self.config.files_dir.clone(),
        };
        let app = create_gateway_router(state);

        info!("SMS HTTP gateway listening on {}", self.config.http.addr);
        info!("SPEAR Console enabled: {}", self.config.enable_console);
        info!("Swagger UI enabled: {}", self.config.enable_swagger);

        let listener = tokio::net::TcpListener::bind(self.config.http.addr).await?;
        Ok((listener, app))
    }
}
