//! SMS Service Implementation / SMS服务实现
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, RwLock};
use tonic::{Request, Response, Status};

use uuid::Uuid;

use crate::sms::config::SmsConfig;
use crate::sms::instance_execution_index::InstanceExecutionIndex;
use crate::sms::placement::outcome::normalize_outcome_class;
use crate::sms::placement::policy::select_top_candidates;
use crate::sms::placement::state::PlacementState;
use crate::sms::registry::mcp::{delete_mcp_record, list_mcp_records, upsert_mcp_record};
use crate::sms::registry::state::{BackendRegistryState, McpRegistryState};
use crate::sms::runtime::{
    build_runtime_stores, start_assignment_reconcile_loop, start_cleanup_loop,
};
use crate::sms::services::{
    node_service::NodeService, resource_service::ResourceService,
    task_assignment_service::TaskAssignmentService as TaskAssignmentServiceImpl,
    task_service::TaskService as TaskServiceImpl,
};
use crate::sms::unified_events::UnifiedEventBus;
use anyhow::Context;
use futures::{stream::unfold, StreamExt};
use tracing::{debug, warn};

use crate::sms::admin_credentials::AdminCredentialsState;
use crate::sms::ai_backends::KvAiBackendRepository;

// Import proto types / 导入proto类型
use crate::proto::sms::{
    admin_credential_service_server::AdminCredentialService as AdminCredentialServiceTrait,
    backend_registry_service_server::BackendRegistryService as BackendRegistryServiceTrait,
    events_service_server::EventsService as EventsServiceTrait,
    execution_index_service_server::ExecutionIndexService as ExecutionIndexServiceTrait,
    execution_log_ingest_service_server::ExecutionLogIngestService as ExecutionLogIngestServiceTrait,
    execution_registry_service_server::ExecutionRegistryService as ExecutionRegistryServiceTrait,
    instance_registry_service_server::InstanceRegistryService as InstanceRegistryServiceTrait,
    mcp_registry_service_server::McpRegistryService as McpRegistryServiceTrait,
    node_service_server::NodeService as NodeServiceTrait,
    placement_service_server::PlacementService as PlacementServiceTrait,
    AppendExecutionLogsRequest,
    AppendExecutionLogsResponse,
    BackendSpec,
    BackendStatus,
    DeleteCredentialRequest,
    DeleteCredentialResponse,
    DeleteInstanceRequest,
    DeleteInstanceResponse,
    DeleteMcpServerRequest,
    DeleteMcpServerResponse,
    DeleteNodeRequest,
    DeleteNodeResponse,
    EventEnvelope,
    EventOp,
    Execution,
    FinalizeExecutionLogsRequest,
    FinalizeExecutionLogsResponse,
    GetExecutionRequest,
    GetExecutionResponse,
    GetInstanceRequest,
    GetInstanceResponse,
    GetNodeBackendsRequest,
    GetNodeBackendsResponse,
    GetNodeRequest,
    GetNodeResourceRequest,
    GetNodeResourceResponse,
    GetNodeResponse,
    GetNodeWithResourceRequest,
    GetNodeWithResourceResponse,
    HeartbeatRequest,
    HeartbeatResponse,
    Instance,
    ListCredentialMaterialsRequest,
    ListCredentialMaterialsResponse,
    ListCredentialsRequest,
    ListCredentialsResponse,
    ListExecutionsRequest,
    ListExecutionsResponse,
    ListInstanceExecutionsRequest,
    ListInstanceExecutionsResponse,
    ListMcpServersRequest,
    ListMcpServersResponse,
    ListNodeBackendSnapshotsRequest,
    ListNodeBackendSnapshotsResponse,
    ListNodeResourcesRequest,
    ListNodeResourcesResponse,
    ListNodesRequest,
    ListNodesResponse,
    ListTaskInstancesRequest,
    ListTaskInstancesResponse,
    McpServerRecord,
    McpTransport,
    NodeBackendSnapshot,
    PlaceInvocationRequest,
    PlaceInvocationResponse,
    // Node service messages / 节点服务消息
    RegisterNodeRequest,
    RegisterNodeResponse,
    ReportExecutionResponse,
    ReportInstanceResponse,
    ReportInvocationOutcomeRequest,
    ReportInvocationOutcomeResponse,
    ReportNodeBackendsRequest,
    ReportNodeBackendsResponse,
    SubscribeEventsRequest,
    UpdateNodeRequest,
    UpdateNodeResourceRequest,
    UpdateNodeResourceResponse,
    UpdateNodeResponse,
    UpsertCredentialRequest,
    UpsertCredentialResponse,
    UpsertMcpServerRequest,
    UpsertMcpServerResponse,
    WatchCredentialMaterialsRequest,
    WatchCredentialMaterialsResponse,
    WatchCredentialsRequest,
    WatchCredentialsResponse,
    WatchMcpServersRequest,
    WatchMcpServersResponse,
};

use crate::proto::spearlet::router_filter_service_server::RouterFilterService as RouterFilterServiceTrait;
use crate::proto::spearlet::{
    FilterRequest as RouterFilterRequest, FilterResponse as RouterFilterResponse,
};

// Note: SpearletRegistrationService is not defined in current proto files
// use crate::proto::spearlet::{
//     spearlet_registration_service_server::SpearletRegistrationService,
//     RegisterSpearletRequest, RegisterSpearletResponse,
//     SpearletHeartbeatRequest, SpearletHeartbeatResponse,
//     UnregisterSpearletRequest, UnregisterSpearletResponse,
// };

#[derive(Debug, Clone)]
pub struct SmsServiceImpl {
    pub(crate) node_service: Arc<RwLock<NodeService>>,
    pub(crate) resource_service: Arc<ResourceService>,
    #[allow(dead_code)]
    pub(crate) config: Arc<SmsConfig>,
    pub(crate) task_service: Arc<RwLock<TaskServiceImpl>>,
    pub(crate) task_assignment_service: Arc<TaskAssignmentServiceImpl>,
    pub(crate) unified_events: Arc<UnifiedEventBus>,
    pub(crate) instance_execution_index: Arc<InstanceExecutionIndex>,
    pub(crate) placement_state: Arc<PlacementState>,
    mcp_registry: Arc<McpRegistryState>,
    backend_registry: Arc<BackendRegistryState>,
    admin_credentials: Arc<AdminCredentialsState>,
    pub(crate) ai_backend_repository: Arc<KvAiBackendRepository>,
    router_filter_engine: Arc<RouterFilterEngine>,
}

#[derive(Debug, Clone)]
enum RouterFilterEngine {
    Builtin(BuiltinRouterFilterEngine),
}

#[derive(Debug, Clone, Default)]
struct BuiltinRouterFilterEngine {}

impl RouterFilterEngine {
    fn filter(&self, req: RouterFilterRequest) -> RouterFilterResponse {
        match self {
            RouterFilterEngine::Builtin(_) => {
                debug!(
                    correlation_id = %req.correlation_id,
                    request_id = %req.request_id,
                    operation = req.operation,
                    candidates = req.candidates.len(),
                    "router filter builtin noop"
                );
                let mut dbg = std::collections::HashMap::new();
                dbg.insert("engine".to_string(), "sms_builtin".to_string());
                dbg.insert("mode".to_string(), "noop".to_string());
                dbg.insert("candidates".to_string(), req.candidates.len().to_string());
                dbg.insert("operation".to_string(), req.operation.to_string());
                RouterFilterResponse {
                    correlation_id: req.correlation_id,
                    decision_id: "sms_builtin".to_string(),
                    decisions: Vec::new(),
                    final_action: None,
                    debug: dbg,
                }
            }
        }
    }
}

impl SmsServiceImpl {
    pub async fn bootstrap_mcp_from_dir(&self, dir: &str) -> anyhow::Result<usize> {
        if dir.is_empty() {
            return Ok(0);
        }

        fn expand_tilde(s: &str) -> String {
            if let Some(rest) = s.strip_prefix("~/") {
                if let Ok(home) = std::env::var("HOME") {
                    return format!("{}/{}", home, rest);
                }
            }
            s.to_string()
        }

        #[derive(serde::Deserialize)]
        struct FileCfg {
            server_id: String,
            display_name: Option<String>,
            transport: String,
            stdio: Option<FileStdio>,
            http: Option<FileHttp>,
            tool_namespace: Option<String>,
            allowed_tools: Option<Vec<String>>,
            budgets: Option<FileBudgets>,
            approval_policy: Option<FileApprovalPolicy>,
        }
        #[derive(serde::Deserialize)]
        struct FileStdio {
            command: String,
            args: Option<Vec<String>>,
            env: Option<std::collections::HashMap<String, String>>,
            env_from: Option<Vec<String>>,
            cwd: Option<String>,
        }
        #[derive(serde::Deserialize)]
        struct FileHttp {
            url: String,
            headers: Option<std::collections::HashMap<String, String>>,
            auth_ref: Option<String>,
        }
        #[derive(serde::Deserialize)]
        struct FileBudgets {
            tool_timeout_ms: Option<u64>,
            max_concurrency: Option<u64>,
            max_tool_output_bytes: Option<u64>,
        }
        #[derive(serde::Deserialize)]
        struct FileApprovalPolicy {
            default_policy: Option<String>,
            per_tool: Option<std::collections::HashMap<String, String>>,
        }

        let dir = expand_tilde(dir);
        let mut count = 0usize;
        let entries = std::fs::read_dir(&dir).with_context(|| format!("read_dir {}", dir))?;
        for ent in entries {
            let ent = match ent {
                Ok(e) => e,
                Err(_) => continue,
            };
            let path = ent.path();
            if !path.is_file() {
                continue;
            }
            let ext = path
                .extension()
                .and_then(|x| x.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if ext != "toml" && ext != "json" {
                continue;
            }
            let content = match std::fs::read_to_string(&path) {
                Ok(s) => s,
                Err(_) => continue,
            };

            let cfg: FileCfg = if ext == "toml" {
                match toml::from_str(&content) {
                    Ok(v) => v,
                    Err(_) => continue,
                }
            } else {
                match serde_json::from_str(&content) {
                    Ok(v) => v,
                    Err(_) => continue,
                }
            };

            let transport = match cfg.transport.as_str() {
                "stdio" => McpTransport::Stdio as i32,
                "streamable_http" => McpTransport::StreamableHttp as i32,
                _ => continue,
            };

            let record = McpServerRecord {
                server_id: cfg.server_id,
                display_name: cfg.display_name.unwrap_or_default(),
                transport,
                stdio: cfg.stdio.map(|s| {
                    let mut env = s.env.unwrap_or_default();
                    if let Some(from) = s.env_from {
                        for name in from {
                            let name = name.trim();
                            if name.is_empty() {
                                continue;
                            }
                            env.insert(name.to_string(), format!("${{ENV:{}}}", name));
                        }
                    }
                    crate::proto::sms::McpStdioConfig {
                        command: s.command,
                        args: s.args.unwrap_or_default(),
                        env,
                        cwd: s.cwd.unwrap_or_default(),
                    }
                }),
                http: cfg.http.map(|h| crate::proto::sms::McpHttpConfig {
                    url: h.url,
                    headers: h.headers.unwrap_or_default(),
                    auth_ref: h.auth_ref.unwrap_or_default(),
                }),
                tool_namespace: cfg.tool_namespace.unwrap_or_default(),
                allowed_tools: cfg.allowed_tools.unwrap_or_default(),
                approval_policy: cfg.approval_policy.map(|p| {
                    crate::proto::sms::McpApprovalPolicy {
                        default_policy: p.default_policy.unwrap_or_default(),
                        per_tool: p.per_tool.unwrap_or_default(),
                    }
                }),
                budgets: cfg.budgets.map(|b| crate::proto::sms::McpBudgets {
                    tool_timeout_ms: b.tool_timeout_ms.unwrap_or(0),
                    max_concurrency: b.max_concurrency.unwrap_or(0),
                    max_tool_output_bytes: b.max_tool_output_bytes.unwrap_or(0),
                }),
                updated_at_ms: 0,
            };

            if upsert_mcp_record(&self.mcp_registry, record).await.is_ok() {
                count += 1;
            }
        }

        Ok(count)
    }
}

impl SmsServiceImpl {
    /// Create new SMS service implementation / 创建新的SMS服务实现
    pub async fn new(
        node_service: Arc<RwLock<NodeService>>,
        resource_service: Arc<ResourceService>,
        config: Arc<SmsConfig>,
    ) -> Self {
        // Create task service / 创建任务服务
        let task_service = Arc::new(RwLock::new(TaskServiceImpl::new()));
        let task_assignment_service = Arc::new(TaskAssignmentServiceImpl::new());
        let runtime = build_runtime_stores(config.clone()).await;
        let unified_events = runtime.unified_events.clone();
        let instance_execution_index = runtime.instance_execution_index.clone();
        let admin_credentials = runtime.admin_credentials.clone();
        let ai_backend_repository = runtime.ai_backend_repository.clone();

        {
            crate::sms::projectors::start_index_projectors(
                instance_execution_index.clone(),
                unified_events.clone(),
            );
        }

        start_cleanup_loop(
            node_service.clone(),
            resource_service.clone(),
            config.clone(),
            unified_events.clone(),
        );

        let service = Self {
            node_service,
            resource_service,
            config,
            task_service,
            task_assignment_service,
            unified_events,
            instance_execution_index,
            placement_state: Arc::new(PlacementState::new()),
            mcp_registry: Arc::new(McpRegistryState::new(1024, 1024)),
            backend_registry: Arc::new(BackendRegistryState::new()),
            admin_credentials,
            ai_backend_repository,
            router_filter_engine: Arc::new(RouterFilterEngine::Builtin(
                BuiltinRouterFilterEngine::default(),
            )),
        };

        start_assignment_reconcile_loop(service.clone());

        service
    }

    /// Convert proto NodeResource to internal NodeResourceInfo / 将proto NodeResource转换为内部NodeResourceInfo
    #[allow(clippy::result_large_err)]
    fn proto_resource_to_resource_info(
        proto_resource: &crate::proto::sms::NodeResource,
    ) -> Result<crate::sms::services::resource_service::NodeResourceInfo, Status> {
        let node_uuid = uuid::Uuid::parse_str(&proto_resource.node_uuid)
            .map_err(|_| Status::invalid_argument("Invalid node UUID format"))?;

        let updated_at = if proto_resource.updated_at > 0 {
            chrono::DateTime::from_timestamp(proto_resource.updated_at, 0)
                .unwrap_or_else(chrono::Utc::now)
        } else {
            chrono::Utc::now()
        };

        Ok(crate::sms::services::resource_service::NodeResourceInfo {
            node_uuid,
            cpu_usage_percent: proto_resource.cpu_usage_percent,
            memory_usage_percent: proto_resource.memory_usage_percent,
            total_memory_bytes: proto_resource.total_memory_bytes,
            used_memory_bytes: proto_resource.used_memory_bytes,
            available_memory_bytes: proto_resource.available_memory_bytes,
            disk_usage_percent: proto_resource.disk_usage_percent,
            total_disk_bytes: proto_resource.total_disk_bytes,
            used_disk_bytes: proto_resource.used_disk_bytes,
            network_rx_bytes_per_sec: proto_resource.network_rx_bytes_per_sec,
            network_tx_bytes_per_sec: proto_resource.network_tx_bytes_per_sec,
            load_average_1m: proto_resource.load_average_1m,
            load_average_5m: proto_resource.load_average_5m,
            load_average_15m: proto_resource.load_average_15m,
            updated_at,
            resource_metadata: proto_resource.resource_metadata.clone(),
        })
    }

    /// Convert internal NodeResourceInfo to proto NodeResource / 将内部NodeResourceInfo转换为proto NodeResource
    fn resource_info_to_proto_resource(
        resource_info: &crate::sms::services::resource_service::NodeResourceInfo,
    ) -> crate::proto::sms::NodeResource {
        crate::proto::sms::NodeResource {
            node_uuid: resource_info.node_uuid.to_string(),
            cpu_usage_percent: resource_info.cpu_usage_percent,
            memory_usage_percent: resource_info.memory_usage_percent,
            total_memory_bytes: resource_info.total_memory_bytes,
            used_memory_bytes: resource_info.used_memory_bytes,
            available_memory_bytes: resource_info.available_memory_bytes,
            disk_usage_percent: resource_info.disk_usage_percent,
            total_disk_bytes: resource_info.total_disk_bytes,
            used_disk_bytes: resource_info.used_disk_bytes,
            network_rx_bytes_per_sec: resource_info.network_rx_bytes_per_sec,
            network_tx_bytes_per_sec: resource_info.network_tx_bytes_per_sec,
            load_average_1m: resource_info.load_average_1m,
            load_average_5m: resource_info.load_average_5m,
            load_average_15m: resource_info.load_average_15m,
            updated_at: resource_info.updated_at.timestamp(),
            resource_metadata: resource_info.resource_metadata.clone(),
        }
    }

    /// Create SMS service with storage configuration / 使用存储配置创建SMS服务
    pub async fn with_storage_config(storage_config: &crate::config::base::StorageConfig) -> Self {
        // Create KV store from storage config / 从存储配置创建KV存储
        use crate::storage::{create_kv_store_from_config, KvStoreConfig};
        let kv_config = KvStoreConfig::from_storage_config(storage_config);
        let _kv_store = create_kv_store_from_config(&kv_config)
            .await
            .expect("Failed to create KV store from storage config");

        let node_service = Arc::new(RwLock::new(NodeService::new()));
        let resource_service = Arc::new(ResourceService::new());
        let config = Arc::new(SmsConfig::default());

        Self::new(node_service, resource_service, config).await
    }

    /// Get node service reference / 获取节点服务引用
    pub fn node_service(&self) -> Arc<RwLock<NodeService>> {
        self.node_service.clone()
    }
}

#[tonic::async_trait]
impl McpRegistryServiceTrait for SmsServiceImpl {
    type WatchMcpServersStream = std::pin::Pin<
        Box<
            dyn tokio_stream::Stream<Item = Result<WatchMcpServersResponse, Status>>
                + Send
                + 'static,
        >,
    >;

    async fn list_mcp_servers(
        &self,
        _request: Request<ListMcpServersRequest>,
    ) -> Result<Response<ListMcpServersResponse>, Status> {
        let (revision, servers) = list_mcp_records(&self.mcp_registry).await;
        Ok(Response::new(ListMcpServersResponse { revision, servers }))
    }

    async fn watch_mcp_servers(
        &self,
        request: Request<WatchMcpServersRequest>,
    ) -> Result<Response<Self::WatchMcpServersStream>, Status> {
        let since_revision = request.into_inner().since_revision;
        let stream = self
            .mcp_registry
            .watch
            .watch(since_revision, |e| e.revision)
            .await?
            .map(|r| r.map(|event| WatchMcpServersResponse { event: Some(event) }));

        Ok(Response::new(Box::pin(stream)))
    }

    async fn upsert_mcp_server(
        &self,
        request: Request<UpsertMcpServerRequest>,
    ) -> Result<Response<UpsertMcpServerResponse>, Status> {
        let record = request
            .into_inner()
            .record
            .ok_or_else(|| Status::invalid_argument("record is required"))?;

        let revision = upsert_mcp_record(&self.mcp_registry, record).await?;
        Ok(Response::new(UpsertMcpServerResponse { revision }))
    }

    async fn delete_mcp_server(
        &self,
        request: Request<DeleteMcpServerRequest>,
    ) -> Result<Response<DeleteMcpServerResponse>, Status> {
        let revision =
            delete_mcp_record(&self.mcp_registry, request.into_inner().server_id).await?;
        Ok(Response::new(DeleteMcpServerResponse { revision }))
    }
}

#[tonic::async_trait]
impl BackendRegistryServiceTrait for SmsServiceImpl {
    async fn report_node_backends(
        &self,
        request: Request<ReportNodeBackendsRequest>,
    ) -> Result<Response<ReportNodeBackendsResponse>, Status> {
        let mut snapshot = request
            .into_inner()
            .snapshot
            .ok_or_else(|| Status::invalid_argument("snapshot is required"))?;

        if snapshot.node_uuid.trim().is_empty() {
            return Err(Status::invalid_argument("snapshot.node_uuid is required"));
        }

        if snapshot.reported_at_ms == 0 {
            snapshot.reported_at_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;
        }

        snapshot
            .backends
            .retain(|b| b.spec.as_ref().is_some_and(|s| !s.name.trim().is_empty()));
        snapshot.backends.iter_mut().for_each(|b| {
            let spec = b.spec.get_or_insert_with(BackendSpec::default);
            if spec.kind.trim().is_empty() {
                spec.kind = "unknown".to_string();
            }
            if b.status == BackendStatus::Unspecified as i32 {
                b.status = BackendStatus::Unavailable as i32;
                if b.status_reason.trim().is_empty() {
                    b.status_reason = "unspecified".to_string();
                }
            }
        });

        let node_uuid = snapshot.node_uuid.clone();
        let mut guard = self.backend_registry.snapshots.write().await;
        let accepted_revision = match guard.get(&node_uuid) {
            Some(prev) if prev.revision > snapshot.revision => prev.revision,
            _ => {
                let rev = snapshot.revision;
                guard.insert(node_uuid, snapshot);
                rev
            }
        };

        Ok(Response::new(ReportNodeBackendsResponse {
            success: true,
            message: "ok".to_string(),
            accepted_revision,
        }))
    }

    async fn get_node_backends(
        &self,
        request: Request<GetNodeBackendsRequest>,
    ) -> Result<Response<GetNodeBackendsResponse>, Status> {
        let node_uuid = request.into_inner().node_uuid;
        if node_uuid.trim().is_empty() {
            return Err(Status::invalid_argument("node_uuid is required"));
        }

        let guard = self.backend_registry.snapshots.read().await;
        let snapshot = guard.get(&node_uuid).cloned();
        Ok(Response::new(GetNodeBackendsResponse {
            found: snapshot.is_some(),
            snapshot,
        }))
    }

    async fn list_node_backend_snapshots(
        &self,
        request: Request<ListNodeBackendSnapshotsRequest>,
    ) -> Result<Response<ListNodeBackendSnapshotsResponse>, Status> {
        let req = request.into_inner();
        let limit = if req.limit == 0 { 200 } else { req.limit } as usize;
        let offset = req.offset as usize;

        let guard = self.backend_registry.snapshots.read().await;
        let mut items: Vec<NodeBackendSnapshot> = guard.values().cloned().collect();
        items.sort_by(|a, b| a.node_uuid.cmp(&b.node_uuid));

        let total_count = items.len() as u32;
        let snapshots = if offset >= items.len() {
            Vec::new()
        } else {
            items.into_iter().skip(offset).take(limit).collect()
        };

        Ok(Response::new(ListNodeBackendSnapshotsResponse {
            snapshots,
            total_count,
        }))
    }
}

#[tonic::async_trait]
impl AdminCredentialServiceTrait for SmsServiceImpl {
    type WatchCredentialsStream = crate::sms::registry_watch::WatchStream<WatchCredentialsResponse>;
    type WatchCredentialMaterialsStream =
        crate::sms::registry_watch::WatchStream<WatchCredentialMaterialsResponse>;

    async fn list_credentials(
        &self,
        _request: Request<ListCredentialsRequest>,
    ) -> Result<Response<ListCredentialsResponse>, Status> {
        let (revision, credentials) = self.admin_credentials.list_infos().await?;
        Ok(Response::new(ListCredentialsResponse {
            revision,
            credentials,
        }))
    }

    async fn upsert_credential(
        &self,
        request: Request<UpsertCredentialRequest>,
    ) -> Result<Response<UpsertCredentialResponse>, Status> {
        let req = request.into_inner();
        let revision = self
            .admin_credentials
            .upsert(&req.name, &req.secret, &req.description, req.disabled)
            .await?;
        Ok(Response::new(UpsertCredentialResponse { revision }))
    }

    async fn delete_credential(
        &self,
        request: Request<DeleteCredentialRequest>,
    ) -> Result<Response<DeleteCredentialResponse>, Status> {
        let req = request.into_inner();
        let (revision, deleted) = self.admin_credentials.delete(&req.name).await?;
        Ok(Response::new(DeleteCredentialResponse {
            revision,
            deleted,
        }))
    }

    async fn watch_credentials(
        &self,
        request: Request<WatchCredentialsRequest>,
    ) -> Result<Response<Self::WatchCredentialsStream>, Status> {
        let req = request.into_inner();
        let stream = self
            .admin_credentials
            .watch_infos(req.since_revision)
            .await?;
        Ok(Response::new(stream))
    }

    async fn list_credential_materials(
        &self,
        _request: Request<ListCredentialMaterialsRequest>,
    ) -> Result<Response<ListCredentialMaterialsResponse>, Status> {
        let (revision, credentials) = self.admin_credentials.list_materials().await?;
        Ok(Response::new(ListCredentialMaterialsResponse {
            revision,
            credentials,
        }))
    }

    async fn watch_credential_materials(
        &self,
        request: Request<WatchCredentialMaterialsRequest>,
    ) -> Result<Response<Self::WatchCredentialMaterialsStream>, Status> {
        let req = request.into_inner();
        let stream = self
            .admin_credentials
            .watch_materials(req.since_revision)
            .await?;
        Ok(Response::new(stream))
    }
}

// Implement NodeService trait / 实现NodeService trait
#[tonic::async_trait]
impl NodeServiceTrait for SmsServiceImpl {
    /// Register a new node / 注册新节点
    async fn register_node(
        &self,
        request: Request<RegisterNodeRequest>,
    ) -> Result<Response<RegisterNodeResponse>, Status> {
        let req = request.into_inner();
        let node = req
            .node
            .ok_or_else(|| Status::invalid_argument("Node is required"))?;

        // Register node directly / 直接注册节点
        let mut node_service = self.node_service.write().await;

        match node_service.register_node(node.clone()).await {
            Ok(()) => {
                tracing::info!(uuid = %node.uuid, ip = %node.ip_address, port = %node.port, "SPEARlet registered");
                self.publish_node_topology_event(
                    &node,
                    EventOp::Create,
                    "Publish unified node create event failed",
                )
                .await;
                let response = RegisterNodeResponse {
                    node_uuid: node.uuid.clone(),
                    success: true,
                    message: "Node registered successfully".to_string(),
                };
                self.reconcile_assignments_after_node_change(
                    "RegisterNode: reconcile task assignments failed",
                )
                .await;
                Ok(Response::new(response))
            }
            Err(e) => {
                let response = RegisterNodeResponse {
                    success: false,
                    message: format!("Failed to register node: {}", e),
                    node_uuid: String::new(),
                };
                Ok(Response::new(response))
            }
        }
    }

    /// Update an existing node / 更新现有节点
    async fn update_node(
        &self,
        request: Request<UpdateNodeRequest>,
    ) -> Result<Response<UpdateNodeResponse>, Status> {
        let req = request.into_inner();
        let node = req
            .node
            .ok_or_else(|| Status::invalid_argument("Node is required"))?;
        let node_for_event = node.clone();

        // Use update_node to update the existing node / 使用update_node来更新现有节点
        let mut node_service = self.node_service.write().await;

        match node_service.update_node(node).await {
            Ok(_) => {
                self.publish_node_topology_event(
                    &node_for_event,
                    EventOp::Update,
                    "Publish unified node update event failed",
                )
                .await;
                let response = UpdateNodeResponse {
                    success: true,
                    message: "Node updated successfully".to_string(),
                };
                self.reconcile_assignments_after_node_change(
                    "UpdateNode: reconcile task assignments failed",
                )
                .await;
                Ok(Response::new(response))
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Delete a node / 删除节点
    async fn delete_node(
        &self,
        request: Request<DeleteNodeRequest>,
    ) -> Result<Response<DeleteNodeResponse>, Status> {
        let req = request.into_inner();
        let node_uuid = Uuid::parse_str(&req.uuid)
            .map_err(|_| Status::invalid_argument("Invalid UUID format"))?;

        let mut node_service = self.node_service.write().await;

        match node_service.remove_node(&node_uuid.to_string()).await {
            Ok(_) => {
                let _ = self.resource_service.remove_resource(&node_uuid).await;
                tracing::info!(uuid = %node_uuid, "SPEARlet unregistered");
                self.publish_node_deleted_event(
                    &node_uuid.to_string(),
                    "Publish unified node delete event failed",
                )
                .await;
                let response = DeleteNodeResponse {
                    success: true,
                    message: "Node deleted successfully".to_string(),
                };
                self.reconcile_assignments_after_node_change(
                    "DeleteNode: reconcile task assignments failed",
                )
                .await;
                Ok(Response::new(response))
            }
            Err(e) => Err(e.into()), // Use SmsError to tonic::Status conversion
        }
    }

    /// Send heartbeat / 发送心跳
    async fn heartbeat(
        &self,
        request: Request<HeartbeatRequest>,
    ) -> Result<Response<HeartbeatResponse>, Status> {
        let req = request.into_inner();
        let mut node_service = self.node_service.write().await;

        match node_service
            .update_heartbeat(&req.uuid, chrono::Utc::now().timestamp())
            .await
        {
            Ok(changed) => {
                if let Some(ref node) = changed {
                    self.publish_node_topology_event(
                        node,
                        EventOp::Update,
                        "Publish unified node online event failed",
                    )
                    .await;
                }
                tracing::debug!(uuid = %req.uuid, "Heartbeat received");
                if changed.is_some() {
                    self.reconcile_assignments_after_node_change(
                        "Heartbeat: reconcile task assignments failed",
                    )
                    .await;
                }
                let response = HeartbeatResponse {
                    success: true,
                    message: "Heartbeat received".to_string(),
                    server_timestamp: chrono::Utc::now().timestamp(),
                };
                Ok(Response::new(response))
            }
            Err(e) => Err(e.into()),
        }
    }

    /// List all nodes / 列出所有节点
    async fn list_nodes(
        &self,
        request: Request<ListNodesRequest>,
    ) -> Result<Response<ListNodesResponse>, Status> {
        let _req = request.into_inner();
        let node_service = self.node_service.read().await;

        let nodes = node_service.list_nodes().await;

        match nodes {
            Ok(node_list) => {
                let response = ListNodesResponse { nodes: node_list };
                Ok(Response::new(response))
            }
            Err(e) => Err(Status::internal(format!("List nodes failed: {}", e))),
        }
    }

    /// Get specific node / 获取特定节点
    async fn get_node(
        &self,
        request: Request<GetNodeRequest>,
    ) -> Result<Response<GetNodeResponse>, Status> {
        let req = request.into_inner();
        let node_service = self.node_service.read().await;

        match node_service.get_node(&req.uuid).await {
            Ok(Some(node)) => {
                let response = GetNodeResponse {
                    found: true,
                    node: Some(node),
                };
                Ok(Response::new(response))
            }
            Ok(None) => Err(Status::not_found("Node not found")),
            Err(e) => Err(e.into()),
        }
    }

    /// Update node resource information / 更新节点资源信息
    async fn update_node_resource(
        &self,
        request: Request<UpdateNodeResourceRequest>,
    ) -> Result<Response<UpdateNodeResourceResponse>, Status> {
        let req = request.into_inner();

        let resource = req
            .resource
            .ok_or_else(|| Status::invalid_argument("Resource is required"))?;

        let resource_info = Self::proto_resource_to_resource_info(&resource)?;

        match self.resource_service.update_resource(resource_info).await {
            Ok(_) => {
                let response = UpdateNodeResourceResponse {
                    success: true,
                    message: "Resource updated successfully".to_string(),
                };
                Ok(Response::new(response))
            }
            Err(e) => {
                let response = UpdateNodeResourceResponse {
                    success: false,
                    message: format!("Update failed: {}", e),
                };
                Ok(Response::new(response))
            }
        }
    }

    /// Get node resource information / 获取节点资源信息
    async fn get_node_resource(
        &self,
        request: Request<GetNodeResourceRequest>,
    ) -> Result<Response<GetNodeResourceResponse>, Status> {
        let req = request.into_inner();
        // Allow non-UUID identifiers: if parsing fails, treat as no resource found
        let res = if let Ok(u) = uuid::Uuid::parse_str(&req.node_uuid) {
            self.resource_service.get_resource(&u).await
        } else {
            Ok(None)
        };
        match res {
            Ok(Some(resource_info)) => {
                let response = GetNodeResourceResponse {
                    found: true,
                    resource: Some(Self::resource_info_to_proto_resource(&resource_info)),
                };
                Ok(Response::new(response))
            }
            Ok(None) => {
                let response = GetNodeResourceResponse {
                    found: false,
                    resource: None,
                };
                Ok(Response::new(response))
            }
            Err(e) => Err(Status::internal(format!("Get resource failed: {}", e))),
        }
    }

    /// List node resources / 列出节点资源信息
    async fn list_node_resources(
        &self,
        request: Request<ListNodeResourcesRequest>,
    ) -> Result<Response<ListNodeResourcesResponse>, Status> {
        let req = request.into_inner();

        let resources = if req.node_uuids.is_empty() {
            // List all resources / 列出所有资源
            self.resource_service.list_resources().await
        } else {
            // Filter by specific node UUIDs / 按特定节点UUID过滤
            let mut node_uuids = Vec::new();
            for uuid_str in &req.node_uuids {
                if let Ok(uuid) = uuid::Uuid::parse_str(uuid_str) {
                    node_uuids.push(uuid);
                }
            }
            self.resource_service
                .list_resources_by_nodes(&node_uuids)
                .await
        };

        match resources {
            Ok(resource_list) => {
                let proto_resources: Vec<_> = resource_list
                    .iter()
                    .map(Self::resource_info_to_proto_resource)
                    .collect();

                let response = ListNodeResourcesResponse {
                    resources: proto_resources,
                };
                Ok(Response::new(response))
            }
            Err(e) => Err(Status::internal(format!("List resources failed: {}", e))),
        }
    }

    /// Get node with resource information / 获取节点及其资源信息
    async fn get_node_with_resource(
        &self,
        request: Request<GetNodeWithResourceRequest>,
    ) -> Result<Response<GetNodeWithResourceResponse>, Status> {
        let req = request.into_inner();
        let node_id = req.uuid;

        // Get node info / 获取节点信息
        let node_service = self.node_service.read().await;
        let node_result = node_service.get_node(&node_id).await;
        drop(node_service);

        // Get resource info / 获取资源信息（如果node_id不是UUID则返回None）
        let resource_result = if let Ok(uuid) = uuid::Uuid::parse_str(&node_id) {
            self.resource_service.get_resource(&uuid).await
        } else {
            Ok(None)
        };

        match (node_result, resource_result) {
            (Ok(Some(node)), Ok(resource_info)) => {
                let response = GetNodeWithResourceResponse {
                    found: true,
                    node: Some(node),
                    resource: resource_info.map(|r| Self::resource_info_to_proto_resource(&r)),
                };
                Ok(Response::new(response))
            }
            (Ok(None), _) => {
                let response = GetNodeWithResourceResponse {
                    found: false,
                    node: None,
                    resource: None,
                };
                Ok(Response::new(response))
            }
            (Err(e), _) => Err(Status::internal(format!("Get node failed: {}", e))),
            (_, Err(e)) => Err(Status::internal(format!("Get resource failed: {}", e))),
        }
    }
}

#[tonic::async_trait]
impl EventsServiceTrait for SmsServiceImpl {
    type SubscribeEventsStream = std::pin::Pin<
        Box<dyn tokio_stream::Stream<Item = Result<EventEnvelope, Status>> + Send + 'static>,
    >;

    async fn subscribe_events(
        &self,
        request: Request<SubscribeEventsRequest>,
    ) -> Result<Response<Self::SubscribeEventsStream>, Status> {
        let req = request.into_inner();
        let selector = req
            .selector
            .and_then(|s| s.selector)
            .ok_or_else(|| Status::invalid_argument("selector is required"))?;

        let stream = match selector {
            crate::proto::sms::subscribe_events_selector::Selector::Stream(s) => {
                if s.is_empty() {
                    return Err(Status::invalid_argument("stream is required"));
                }
                s
            }
            crate::proto::sms::subscribe_events_selector::Selector::NodeUuid(node_uuid) => {
                if node_uuid.is_empty() {
                    return Err(Status::invalid_argument("node_uuid is required"));
                }
                format!("node.{}", node_uuid)
            }
            crate::proto::sms::subscribe_events_selector::Selector::ResourceType(rt) => {
                let s = match crate::proto::sms::ResourceType::try_from(rt) {
                    Ok(crate::proto::sms::ResourceType::Task) => "type.task".to_string(),
                    Ok(crate::proto::sms::ResourceType::Node) => "type.node".to_string(),
                    Ok(crate::proto::sms::ResourceType::Artifact) => "type.artifact".to_string(),
                    Ok(crate::proto::sms::ResourceType::Instance) => "type.instance".to_string(),
                    Ok(crate::proto::sms::ResourceType::Execution) => "type.execution".to_string(),
                    Ok(crate::proto::sms::ResourceType::TaskAssignment) => {
                        "type.task_assignment".to_string()
                    }
                    _ => return Err(Status::invalid_argument("unsupported resource_type")),
                };
                s
            }
            crate::proto::sms::subscribe_events_selector::Selector::All(_) => "all".to_string(),
        };

        let replay_limit = if req.replay_limit == 0 {
            1000usize
        } else {
            (req.replay_limit as usize).min(1000)
        };

        let replay = self
            .unified_events
            .replay_since(&stream, req.after_seq, replay_limit)
            .await
            .map_err(|e| Status::internal(format!("Replay failed: {}", e)))?;
        let replay_stream = tokio_stream::iter(replay.into_iter().map(Ok));

        let rx = self.unified_events.subscribe(&stream).await;
        let live_stream = unfold(rx, |mut r| async move {
            match r.recv().await {
                Ok(ev) => Some((Ok(ev), r)),
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    Some((Err(Status::aborted("watch lagged; resync required")), r))
                }
                Err(broadcast::error::RecvError::Closed) => None,
            }
        });
        let stream = replay_stream.chain(live_stream);

        Ok(Response::new(Box::pin(stream)))
    }
}

#[tonic::async_trait]
impl InstanceRegistryServiceTrait for SmsServiceImpl {
    async fn report_instance(
        &self,
        request: Request<Instance>,
    ) -> Result<Response<ReportInstanceResponse>, Status> {
        let mut inst = request.into_inner();
        if inst.updated_at_ms == 0 {
            inst.updated_at_ms = chrono::Utc::now().timestamp_millis();
        }
        if inst.last_seen_ms == 0 {
            inst.last_seen_ms = inst.updated_at_ms;
        }
        let (accepted, stored_updated_at_ms) = self
            .instance_execution_index
            .upsert_instance_record(inst.clone())
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        if accepted {
            self.instance_execution_index
                .project_instance_views(&inst, inst.updated_at_ms)
                .await
                .map_err(|e| Status::internal(e.to_string()))?;
            if let Err(e) = self
                .unified_events
                .publish_instance_event(&inst, EventOp::Upsert)
                .await
            {
                warn!(error = %e, instance_id = %inst.instance_id, "Publish unified instance event failed");
            }
        }
        Ok(Response::new(ReportInstanceResponse {
            accepted,
            stored_updated_at_ms,
        }))
    }

    async fn delete_instance(
        &self,
        request: Request<DeleteInstanceRequest>,
    ) -> Result<Response<DeleteInstanceResponse>, Status> {
        let mut req = request.into_inner();
        if req.deleted_at_ms == 0 {
            req.deleted_at_ms = chrono::Utc::now().timestamp_millis();
        }
        let (accepted, stored_updated_at_ms) = self
            .instance_execution_index
            .tombstone_instance_record(&req.instance_id, &req.task_id, req.deleted_at_ms)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        if accepted {
            let inst = Instance {
                instance_id: req.instance_id,
                task_id: req.task_id,
                node_uuid: String::new(),
                status: crate::proto::sms::InstanceStatus::Terminated as i32,
                created_at_ms: 0,
                updated_at_ms: req.deleted_at_ms,
                last_seen_ms: req.deleted_at_ms,
                current_execution_id: String::new(),
                metadata: std::collections::HashMap::new(),
            };
            if let Err(e) = self
                .unified_events
                .publish_instance_event(&inst, EventOp::Delete)
                .await
            {
                warn!(
                    error = %e,
                    instance_id = %inst.instance_id,
                    "Publish unified instance delete event failed"
                );
            }
        }
        Ok(Response::new(DeleteInstanceResponse {
            accepted,
            stored_updated_at_ms,
        }))
    }
}

#[tonic::async_trait]
impl ExecutionRegistryServiceTrait for SmsServiceImpl {
    async fn report_execution(
        &self,
        request: Request<Execution>,
    ) -> Result<Response<ReportExecutionResponse>, Status> {
        let mut exe = request.into_inner();
        if exe.updated_at_ms == 0 {
            exe.updated_at_ms = chrono::Utc::now().timestamp_millis();
        }
        if exe.started_at_ms == 0 {
            exe.started_at_ms = exe.updated_at_ms;
        }
        if exe.log_ref.is_none() {
            exe.log_ref = Some(crate::sms::instance_execution_index::make_default_log_ref(
                &exe.execution_id,
            ));
        }
        let (accepted, stored_updated_at_ms) = self
            .instance_execution_index
            .upsert_execution_record(exe.clone())
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        if accepted {
            if let Err(e) = self
                .unified_events
                .publish_execution_event(&exe, EventOp::Upsert)
                .await
            {
                warn!(error = %e, execution_id = %exe.execution_id, "Publish unified execution event failed");
            }
        }
        Ok(Response::new(ReportExecutionResponse {
            accepted,
            stored_updated_at_ms,
        }))
    }
}

#[tonic::async_trait]
impl ExecutionIndexServiceTrait for SmsServiceImpl {
    async fn list_task_instances(
        &self,
        request: Request<ListTaskInstancesRequest>,
    ) -> Result<Response<ListTaskInstancesResponse>, Status> {
        let req = request.into_inner();
        if req.task_id.is_empty() {
            return Err(Status::invalid_argument("task_id is required"));
        }
        let limit = if req.limit <= 0 {
            50
        } else {
            req.limit as usize
        };
        let now_ms = chrono::Utc::now().timestamp_millis();
        let (instances, next_page_token) = self
            .instance_execution_index
            .list_task_instances(&req.task_id, now_ms, limit, &req.page_token)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(ListTaskInstancesResponse {
            instances,
            next_page_token,
        }))
    }

    async fn list_instance_executions(
        &self,
        request: Request<ListInstanceExecutionsRequest>,
    ) -> Result<Response<ListInstanceExecutionsResponse>, Status> {
        let req = request.into_inner();
        if req.instance_id.is_empty() {
            return Err(Status::invalid_argument("instance_id is required"));
        }
        let limit = if req.limit <= 0 {
            50
        } else {
            req.limit as usize
        };
        let (executions, next_page_token) = self
            .instance_execution_index
            .list_instance_executions(&req.instance_id, limit, &req.page_token)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(ListInstanceExecutionsResponse {
            executions,
            next_page_token,
        }))
    }

    async fn list_executions(
        &self,
        request: Request<ListExecutionsRequest>,
    ) -> Result<Response<ListExecutionsResponse>, Status> {
        let req = request.into_inner();
        let limit = if req.limit <= 0 {
            100
        } else {
            req.limit as usize
        };
        let (executions, next_page_token) = self
            .instance_execution_index
            .list_executions(
                Some(&req.task_id),
                Some(&req.status),
                limit,
                &req.page_token,
            )
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(ListExecutionsResponse {
            executions,
            next_page_token,
        }))
    }

    async fn get_instance(
        &self,
        request: Request<GetInstanceRequest>,
    ) -> Result<Response<GetInstanceResponse>, Status> {
        let req = request.into_inner();
        if req.instance_id.is_empty() {
            return Err(Status::invalid_argument("instance_id is required"));
        }
        let now_ms = chrono::Utc::now().timestamp_millis();
        let inst = self
            .instance_execution_index
            .get_instance(&req.instance_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        let active = inst.as_ref().is_some_and(|instance| {
            crate::sms::instance_execution_index::is_instance_active_and_fresh(
                instance.status,
                instance.last_seen_ms,
                now_ms,
                self.instance_execution_index.stale_after_ms(),
            )
        });
        Ok(Response::new(GetInstanceResponse {
            found: inst.is_some(),
            active,
            instance: inst,
        }))
    }

    async fn get_execution(
        &self,
        request: Request<GetExecutionRequest>,
    ) -> Result<Response<GetExecutionResponse>, Status> {
        let req = request.into_inner();
        if req.execution_id.is_empty() {
            return Err(Status::invalid_argument("execution_id is required"));
        }
        let exe = self
            .instance_execution_index
            .get_execution(&req.execution_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(GetExecutionResponse {
            found: exe.is_some(),
            execution: exe,
        }))
    }
}

#[tonic::async_trait]
impl ExecutionLogIngestServiceTrait for SmsServiceImpl {
    async fn append_execution_logs(
        &self,
        request: Request<AppendExecutionLogsRequest>,
    ) -> Result<Response<AppendExecutionLogsResponse>, Status> {
        let req = request.into_inner();
        if req.execution_id.trim().is_empty() {
            return Err(Status::invalid_argument("execution_id is required"));
        }
        if req.lines.is_empty() {
            return Ok(Response::new(AppendExecutionLogsResponse {
                execution_id: req.execution_id,
                acked_seq: 0,
                truncated: false,
                next_seq: 1,
                accepted: 0,
            }));
        }
        let mut lines = Vec::with_capacity(req.lines.len());
        for l in req.lines {
            if l.seq == 0 {
                return Err(Status::invalid_argument("seq must be > 0"));
            }
            lines.push(crate::sms::execution_logs::StoredLogLine {
                ts_ms: l.ts_ms,
                seq: l.seq,
                stream: l.stream,
                level: l.level,
                message: l.message,
            });
        }

        let out = crate::sms::execution_logs::append_logs_with_seq(&req.execution_id, lines).await;
        match out {
            Ok(r) => Ok(Response::new(AppendExecutionLogsResponse {
                execution_id: req.execution_id,
                acked_seq: r.acked_seq,
                truncated: r.truncated,
                next_seq: r.next_seq,
                accepted: r.accepted,
            })),
            Err(crate::sms::execution_logs::AppendWithSeqError::InvalidExecutionId) => {
                Err(Status::invalid_argument("invalid execution_id"))
            }
            Err(crate::sms::execution_logs::AppendWithSeqError::Completed) => {
                Err(Status::failed_precondition("logs already finalized"))
            }
            Err(crate::sms::execution_logs::AppendWithSeqError::Truncated) => {
                Err(Status::resource_exhausted("logs truncated"))
            }
            Err(crate::sms::execution_logs::AppendWithSeqError::InvalidSeq { .. }) => {
                Err(Status::invalid_argument("invalid seq"))
            }
            Err(crate::sms::execution_logs::AppendWithSeqError::OutOfOrder { expected, got }) => {
                Err(Status::failed_precondition(format!(
                    "out_of_order: expected_seq={} got_seq={}",
                    expected, got
                )))
            }
            Err(crate::sms::execution_logs::AppendWithSeqError::Io(e)) => Err(Status::internal(e)),
        }
    }

    async fn finalize_execution_logs(
        &self,
        request: Request<FinalizeExecutionLogsRequest>,
    ) -> Result<Response<FinalizeExecutionLogsResponse>, Status> {
        let req = request.into_inner();
        if req.execution_id.trim().is_empty() {
            return Err(Status::invalid_argument("execution_id is required"));
        }
        let meta = crate::sms::execution_logs::finalize_execution_logs(&req.execution_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(FinalizeExecutionLogsResponse {
            execution_id: meta.execution_id,
            truncated: meta.truncated,
            completed: meta.completed,
            next_seq: meta.next_seq,
            updated_at_ms: meta.updated_at_ms as i64,
        }))
    }
}

#[tonic::async_trait]
impl PlacementServiceTrait for SmsServiceImpl {
    async fn place_invocation(
        &self,
        request: Request<PlaceInvocationRequest>,
    ) -> Result<Response<PlaceInvocationResponse>, Status> {
        let req = request.into_inner();
        if req.request_id.is_empty() {
            return Err(Status::invalid_argument("request_id is required"));
        }
        if req.task_id.is_empty() {
            return Err(Status::invalid_argument("task_id is required"));
        }
        let max_candidates = if req.max_candidates == 0 {
            3
        } else {
            req.max_candidates
        };
        let candidates = self
            .list_scored_placement_candidates()
            .await
            .map_err(Status::internal)?;
        let candidates = select_top_candidates(candidates, max_candidates);

        let decision_id = Uuid::new_v4().to_string();

        let resp = PlaceInvocationResponse {
            decision_id,
            candidates,
        };
        Ok(Response::new(resp))
    }

    async fn report_invocation_outcome(
        &self,
        request: Request<ReportInvocationOutcomeRequest>,
    ) -> Result<Response<ReportInvocationOutcomeResponse>, Status> {
        let req = request.into_inner();
        if req.decision_id.is_empty() || req.request_id.is_empty() || req.task_id.is_empty() {
            return Err(Status::invalid_argument(
                "decision_id, request_id, task_id are required",
            ));
        }
        if req.node_uuid.is_empty() {
            return Err(Status::invalid_argument("node_uuid is required"));
        }
        let outcome_class = normalize_outcome_class(req.outcome_class);
        // Feedback loop: update penalty state to influence subsequent placements.
        // 反馈闭环：更新惩罚状态，影响后续 placement。
        self.placement_state
            .apply_outcome(req.node_uuid, outcome_class);
        Ok(Response::new(ReportInvocationOutcomeResponse {
            accepted: true,
        }))
    }
}

#[tonic::async_trait]
impl RouterFilterServiceTrait for SmsServiceImpl {
    async fn filter(
        &self,
        request: Request<RouterFilterRequest>,
    ) -> Result<Response<RouterFilterResponse>, Status> {
        let r = request.into_inner();
        Ok(Response::new(self.router_filter_engine.filter(r)))
    }
}

impl SmsServiceImpl {
    async fn publish_node_topology_event(
        &self,
        node: &crate::proto::sms::Node,
        op: EventOp,
        failure_log: &'static str,
    ) {
        if let Err(e) = self.unified_events.publish_node_event(node, op).await {
            warn!(error = %e, uuid = %node.uuid, "{failure_log}");
        }
    }

    async fn publish_node_deleted_event(&self, node_uuid: &str, failure_log: &'static str) {
        if let Err(e) = self.unified_events.publish_node_deleted(node_uuid).await {
            warn!(error = %e, uuid = %node_uuid, "{failure_log}");
        }
    }

    async fn reconcile_assignments_after_node_change(&self, reason: &'static str) {
        if let Err(error) = self.reconcile_all_task_assignments().await {
            warn!(error = %error, "{reason}");
        }
    }
}

#[cfg(test)]
impl SmsServiceImpl {
    pub fn test_get_node_penalty_snapshot(&self, node_uuid: &str) -> Option<(u32, i64, i64)> {
        self.placement_state.get_node_penalty_snapshot(node_uuid)
    }
}
