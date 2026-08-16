//! gRPC server implementation for SMS (SPEAR Metadata Server)
//! SMS（SPEAR元数据服务器）的gRPC服务器实现

use anyhow::Result;
use std::net::SocketAddr;
use tonic::transport::Server;
use tracing::{error, info};

use crate::proto::sms::{
    ai_backend_control_plane_service_server::AiBackendControlPlaneServiceServer,
    admin_credential_service_server::AdminCredentialServiceServer,
    backend_registry_service_server::BackendRegistryServiceServer,
    events_service_server::EventsServiceServer,
    execution_index_service_server::ExecutionIndexServiceServer,
    execution_log_ingest_service_server::ExecutionLogIngestServiceServer,
    execution_registry_service_server::ExecutionRegistryServiceServer,
    instance_registry_service_server::InstanceRegistryServiceServer,
    mcp_registry_service_server::McpRegistryServiceServer,
    node_service_server::NodeServiceServer, placement_service_server::PlacementServiceServer,
    task_placement_assignment_service_server::TaskPlacementAssignmentServiceServer,
    task_service_server::TaskServiceServer,
};
use crate::proto::spearlet::router_filter_service_server::RouterFilterServiceServer;

use crate::sms::service::SmsServiceImpl;
/// SMS gRPC server / SMS gRPC服务器
pub struct GrpcServer {
    addr: SocketAddr,
    sms_service: SmsServiceImpl,
}

impl GrpcServer {
    /// Create a new gRPC server / 创建新的gRPC服务器
    pub fn new(addr: SocketAddr, sms_service: SmsServiceImpl) -> Self {
        Self { addr, sms_service }
    }

    /// Start the gRPC server / 启动gRPC服务器
    pub async fn start(self) -> Result<()> {
        let addr = self.addr;
        let sms_service = self.sms_service;
        info!("Starting SMS gRPC server on {}", addr);
        let server = Server::builder()
            .add_service(NodeServiceServer::new(sms_service.clone()))
            .add_service(TaskServiceServer::new(sms_service.clone()))
            .add_service(EventsServiceServer::new(sms_service.clone()))
            .add_service(InstanceRegistryServiceServer::new(sms_service.clone()))
            .add_service(ExecutionRegistryServiceServer::new(sms_service.clone()))
            .add_service(ExecutionIndexServiceServer::new(sms_service.clone()))
            .add_service(ExecutionLogIngestServiceServer::new(sms_service.clone()))
            .add_service(TaskPlacementAssignmentServiceServer::new(
                sms_service.clone(),
            ))
            .add_service(McpRegistryServiceServer::new(sms_service.clone()))
            .add_service(BackendRegistryServiceServer::new(sms_service.clone()))
            .add_service(AiBackendControlPlaneServiceServer::new(
                sms_service.clone(),
            ))
            .add_service(AdminCredentialServiceServer::new(sms_service.clone()))
            .add_service(RouterFilterServiceServer::new(sms_service.clone()))
            .add_service(PlacementServiceServer::new(sms_service))
            .serve(addr);

        info!("SMS gRPC server listening on {}", addr);

        if let Err(e) = server.await {
            error!("SMS gRPC server error: {}", e);
            return Err(e.into());
        }

        Ok(())
    }

    /// Start the gRPC server with shutdown signal / 使用关闭信号启动gRPC服务器
    pub async fn start_with_shutdown<F>(self, shutdown: F) -> Result<()>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let addr = self.addr;
        let sms_service = self.sms_service;
        info!("Starting SMS gRPC server on {}", addr);
        let server = Server::builder()
            .add_service(NodeServiceServer::new(sms_service.clone()))
            .add_service(TaskServiceServer::new(sms_service.clone()))
            .add_service(EventsServiceServer::new(sms_service.clone()))
            .add_service(InstanceRegistryServiceServer::new(sms_service.clone()))
            .add_service(ExecutionRegistryServiceServer::new(sms_service.clone()))
            .add_service(ExecutionIndexServiceServer::new(sms_service.clone()))
            .add_service(ExecutionLogIngestServiceServer::new(sms_service.clone()))
            .add_service(TaskPlacementAssignmentServiceServer::new(
                sms_service.clone(),
            ))
            .add_service(McpRegistryServiceServer::new(sms_service.clone()))
            .add_service(BackendRegistryServiceServer::new(sms_service.clone()))
            .add_service(AiBackendControlPlaneServiceServer::new(
                sms_service.clone(),
            ))
            .add_service(AdminCredentialServiceServer::new(sms_service.clone()))
            .add_service(RouterFilterServiceServer::new(sms_service.clone()))
            .add_service(PlacementServiceServer::new(sms_service))
            .serve_with_shutdown(addr, shutdown);

        info!("SMS gRPC server listening on {}", addr);

        if let Err(e) = server.await {
            error!("SMS gRPC server error: {}", e);
            return Err(e.into());
        }

        Ok(())
    }
}
