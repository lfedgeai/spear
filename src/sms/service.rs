//! SMS Service Implementation / SMS服务实现
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, RwLock};
use tonic::{Request, Response, Status};

use uuid::Uuid;

use crate::sms::config::SmsConfig;
use crate::sms::events::TaskEventBus;
use crate::sms::instance_execution_index::InstanceExecutionIndex;
use crate::sms::services::{
    node_service::NodeService, resource_service::ResourceService,
    task_service::TaskService as TaskServiceImpl,
};
use crate::sms::unified_events::UnifiedEventBus;
use crate::storage::kv::{
    create_kv_store_from_config, get_kv_store_factory, serialization, KvStore, KvStoreConfig,
};
use anyhow::Context;
use dashmap::DashMap;
use futures::stream::unfold;
use tokio::time::Duration;
use tokio_stream::StreamExt;
use tracing::{debug, warn};

use crate::sms::registry_watch::RegistryWatchHub;

// Import proto types / 导入proto类型
use crate::proto::sms::{
    admin_llm_config_service_server::AdminLlmConfigService as AdminLlmConfigServiceTrait,
    backend_registry_service_server::BackendRegistryService as BackendRegistryServiceTrait,
    events_service_server::EventsService as EventsServiceTrait,
    execution_index_service_server::ExecutionIndexService as ExecutionIndexServiceTrait,
    execution_log_ingest_service_server::ExecutionLogIngestService as ExecutionLogIngestServiceTrait,
    execution_registry_service_server::ExecutionRegistryService as ExecutionRegistryServiceTrait,
    instance_registry_service_server::InstanceRegistryService as InstanceRegistryServiceTrait,
    mcp_registry_service_server::McpRegistryService as McpRegistryServiceTrait,
    model_deployment_registry_service_server::ModelDeploymentRegistryService as ModelDeploymentRegistryServiceTrait,
    node_service_server::NodeService as NodeServiceTrait,
    placement_service_server::PlacementService as PlacementServiceTrait,
    task_service_server::TaskService as TaskServiceTrait,
    AppendExecutionLogsRequest,
    AppendExecutionLogsResponse,
    BackendStatus,
    DeleteMcpServerRequest,
    DeleteMcpServerResponse,
    DeleteModelDeploymentRequest,
    DeleteModelDeploymentResponse,
    DeleteNodeRequest,
    DeleteNodeResponse,
    DeleteRemoteBackendRequest,
    DeleteRemoteBackendResponse,
    EventEnvelope,
    EventOp,
    Execution,
    FinalizeExecutionLogsRequest,
    FinalizeExecutionLogsResponse,
    GetExecutionRequest,
    GetExecutionResponse,
    GetNodeBackendsRequest,
    GetNodeBackendsResponse,
    GetNodeRequest,
    GetNodeResourceRequest,
    GetNodeResourceResponse,
    GetNodeResponse,
    GetNodeWithResourceRequest,
    GetNodeWithResourceResponse,
    GetTaskRequest,
    GetTaskResponse,
    HeartbeatRequest,
    HeartbeatResponse,
    Instance,
    InvocationOutcomeClass,
    ListInstanceExecutionsRequest,
    ListInstanceExecutionsResponse,
    ListMcpServersRequest,
    ListMcpServersResponse,
    ListModelDeploymentsRequest,
    ListModelDeploymentsResponse,
    ListNodeBackendSnapshotsRequest,
    ListNodeBackendSnapshotsResponse,
    ListNodeResourcesRequest,
    ListNodeResourcesResponse,
    ListNodesRequest,
    ListNodesResponse,
    ListRemoteBackendsRequest,
    ListRemoteBackendsResponse,
    ListTaskInstancesRequest,
    ListTaskInstancesResponse,
    ListTasksRequest,
    ListTasksResponse,
    McpRegistryEvent,
    McpServerRecord,
    McpTransport,
    ModelDeploymentEvent,
    ModelDeploymentRecord,
    ModelDeploymentStatus,
    NodeBackendSnapshot,
    NodeCandidate,
    PlaceInvocationRequest,
    PlaceInvocationResponse,
    // Node service messages / 节点服务消息
    RegisterNodeRequest,
    RegisterNodeResponse,
    // Task service messages / 任务服务消息
    RegisterTaskRequest,
    RegisterTaskResponse,
    RemoteBackendConfig,
    ReportExecutionResponse,
    ReportInstanceResponse,
    ReportInvocationOutcomeRequest,
    ReportInvocationOutcomeResponse,
    ReportModelDeploymentStatusRequest,
    ReportModelDeploymentStatusResponse,
    ReportNodeBackendsRequest,
    ReportNodeBackendsResponse,
    ResolveEndpointRequest,
    ResolveEndpointResponse,
    SubscribeEventsRequest,
    UnregisterTaskRequest,
    UnregisterTaskResponse,
    UpdateNodeRequest,
    UpdateNodeResourceRequest,
    UpdateNodeResourceResponse,
    UpdateNodeResponse,
    UpdateTaskResultRequest,
    UpdateTaskResultResponse,
    UpdateTaskStatusRequest,
    UpdateTaskStatusResponse,
    UpsertMcpServerRequest,
    UpsertMcpServerResponse,
    UpsertModelDeploymentRequest,
    UpsertModelDeploymentResponse,
    UpsertRemoteBackendRequest,
    UpsertRemoteBackendResponse,
    WatchMcpServersRequest,
    WatchMcpServersResponse,
    WatchModelDeploymentsRequest,
    WatchModelDeploymentsResponse,
};

use crate::proto::spearlet::router_filter_service_server::RouterFilterService as RouterFilterServiceTrait;
use crate::proto::spearlet::{
    FilterRequest as RouterFilterRequest, FilterResponse as RouterFilterResponse,
};

#[derive(Debug)]
struct McpRegistryState {
    records: RwLock<HashMap<String, McpServerRecord>>,
    watch: RegistryWatchHub<McpRegistryEvent>,
}

#[derive(Debug)]
struct ModelDeploymentRegistryState {
    records: RwLock<HashMap<String, ModelDeploymentRecord>>,
    watch: RegistryWatchHub<ModelDeploymentEvent>,
    id_to_node: RwLock<HashMap<String, String>>,
}

impl ModelDeploymentRegistryState {
    fn new(event_buffer_size: usize, broadcast_buffer_size: usize) -> Self {
        Self {
            records: RwLock::new(HashMap::new()),
            watch: RegistryWatchHub::new(event_buffer_size, broadcast_buffer_size),
            id_to_node: RwLock::new(HashMap::new()),
        }
    }
}

#[derive(Debug)]
struct BackendRegistryState {
    snapshots: RwLock<HashMap<String, NodeBackendSnapshot>>,
}

impl BackendRegistryState {
    fn new() -> Self {
        Self {
            snapshots: RwLock::new(HashMap::new()),
        }
    }
}

const ADMIN_REMOTE_BACKENDS_KEY: &str = "admin:llm:remote_backends:v1";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
struct RemoteBackendConfigRecord {
    name: String,
    kind: String,
    base_url: String,
    model: String,
    credential_ref: String,
    weight: u32,
    priority: i32,
    operations: Vec<String>,
    features: Vec<String>,
    transports: Vec<String>,
    provider: String,
}

impl From<RemoteBackendConfig> for RemoteBackendConfigRecord {
    fn from(v: RemoteBackendConfig) -> Self {
        Self {
            name: v.name,
            kind: v.kind,
            base_url: v.base_url,
            model: v.model,
            credential_ref: v.credential_ref,
            weight: v.weight,
            priority: v.priority,
            operations: v.operations,
            features: v.features,
            transports: v.transports,
            provider: v.provider,
        }
    }
}

impl From<RemoteBackendConfigRecord> for RemoteBackendConfig {
    fn from(v: RemoteBackendConfigRecord) -> Self {
        Self {
            name: v.name,
            kind: v.kind,
            base_url: v.base_url,
            model: v.model,
            credential_ref: v.credential_ref,
            weight: v.weight,
            priority: v.priority,
            operations: v.operations,
            features: v.features,
            transports: v.transports,
            provider: v.provider,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
struct RemoteBackendConfigSnapshot {
    revision: u64,
    backends: Vec<RemoteBackendConfigRecord>,
}

#[derive(Debug)]
struct AdminLlmConfigState {
    kv: Arc<dyn KvStore>,
    snapshot: RwLock<RemoteBackendConfigSnapshot>,
}

impl AdminLlmConfigState {
    async fn new(kv: Arc<dyn KvStore>) -> Self {
        let snapshot = match kv.get(&ADMIN_REMOTE_BACKENDS_KEY.to_string()).await {
            Ok(Some(bytes)) => serialization::deserialize::<RemoteBackendConfigSnapshot>(&bytes)
                .unwrap_or_default(),
            _ => RemoteBackendConfigSnapshot::default(),
        };
        Self {
            kv,
            snapshot: RwLock::new(snapshot),
        }
    }

    async fn list(&self) -> RemoteBackendConfigSnapshot {
        self.snapshot.read().await.clone()
    }

    async fn upsert(&self, backend: RemoteBackendConfig) -> Result<u64, Status> {
        if backend.name.trim().is_empty() {
            return Err(Status::invalid_argument("missing name"));
        }
        if backend.kind.trim().is_empty() {
            return Err(Status::invalid_argument("missing kind"));
        }
        if backend.base_url.trim().is_empty() {
            return Err(Status::invalid_argument("missing base_url"));
        }
        if backend.operations.is_empty() {
            return Err(Status::invalid_argument("missing operations"));
        }

        let backend: RemoteBackendConfigRecord = backend.into();
        let mut snap = self.snapshot.write().await;
        let mut replaced = false;
        for b in snap.backends.iter_mut() {
            if b.name == backend.name {
                *b = backend.clone();
                replaced = true;
                break;
            }
        }
        if !replaced {
            snap.backends.push(backend);
        }
        snap.backends.sort_by(|a, b| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
        });
        snap.revision = snap.revision.saturating_add(1);

        let bytes =
            serialization::serialize(&*snap).map_err(|e| Status::internal(e.to_string()))?;
        self.kv
            .put(&ADMIN_REMOTE_BACKENDS_KEY.to_string(), &bytes)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(snap.revision)
    }

    async fn delete(&self, name: &str) -> Result<(u64, bool), Status> {
        let n = name.trim();
        if n.is_empty() {
            return Err(Status::invalid_argument("missing name"));
        }
        let mut snap = self.snapshot.write().await;
        let before = snap.backends.len();
        snap.backends.retain(|b| b.name != n);
        let deleted = snap.backends.len() != before;
        if deleted {
            snap.revision = snap.revision.saturating_add(1);
            let bytes =
                serialization::serialize(&*snap).map_err(|e| Status::internal(e.to_string()))?;
            self.kv
                .put(&ADMIN_REMOTE_BACKENDS_KEY.to_string(), &bytes)
                .await
                .map_err(|e| Status::internal(e.to_string()))?;
        }
        Ok((snap.revision, deleted))
    }
}

impl McpRegistryState {
    fn new(event_buffer_size: usize, broadcast_buffer_size: usize) -> Self {
        Self {
            records: RwLock::new(HashMap::new()),
            watch: RegistryWatchHub::new(event_buffer_size, broadcast_buffer_size),
        }
    }

    fn current_revision(&self) -> u64 {
        self.watch.current_revision()
    }

    fn bump_revision(&self) -> u64 {
        self.watch.bump_revision()
    }

    async fn push_event(&self, event: McpRegistryEvent) {
        self.watch.push_event(event).await;
    }
}

// Note: SpearletRegistrationService is not defined in current proto files
// use crate::proto::spearlet::{
//     spearlet_registration_service_server::SpearletRegistrationService,
//     RegisterSpearletRequest, RegisterSpearletResponse,
//     SpearletHeartbeatRequest, SpearletHeartbeatResponse,
//     UnregisterSpearletRequest, UnregisterSpearletResponse,
// };

#[derive(Debug, Clone)]
pub struct SmsServiceImpl {
    node_service: Arc<RwLock<NodeService>>,
    resource_service: Arc<ResourceService>,
    #[allow(dead_code)]
    config: Arc<SmsConfig>,
    task_service: Arc<RwLock<TaskServiceImpl>>,
    events: Arc<TaskEventBus>,
    unified_events: Arc<UnifiedEventBus>,
    instance_execution_index: Arc<InstanceExecutionIndex>,
    placement_state: Arc<PlacementState>,
    mcp_registry: Arc<McpRegistryState>,
    backend_registry: Arc<BackendRegistryState>,
    model_deployment_registry: Arc<ModelDeploymentRegistryState>,
    admin_llm_config: Arc<AdminLlmConfigState>,
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
    async fn upsert_mcp_record_inner(&self, mut record: McpServerRecord) -> Result<u64, Status> {
        if record.server_id.is_empty() {
            return Err(Status::invalid_argument("server_id is required"));
        }
        let server_id = record.server_id.clone();
        if record.tool_namespace.is_empty() {
            record.tool_namespace = format!("mcp.{}", record.server_id);
        }

        match record.transport {
            x if x == McpTransport::Stdio as i32 => {
                let stdio = record
                    .stdio
                    .as_ref()
                    .ok_or_else(|| Status::invalid_argument("stdio config is required"))?;
                if stdio.command.is_empty() {
                    return Err(Status::invalid_argument("stdio.command is required"));
                }
            }
            x if x == McpTransport::StreamableHttp as i32 => {
                let http = record
                    .http
                    .as_ref()
                    .ok_or_else(|| Status::invalid_argument("http config is required"))?;
                if http.url.is_empty() {
                    return Err(Status::invalid_argument("http.url is required"));
                }
            }
            _ => {
                return Err(Status::invalid_argument("invalid transport"));
            }
        }

        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        record.updated_at_ms = now_ms;

        {
            let mut records = self.mcp_registry.records.write().await;
            records.insert(server_id.clone(), record);
        }

        let revision = self.mcp_registry.bump_revision();
        self.mcp_registry
            .push_event(McpRegistryEvent {
                revision,
                upserts: vec![server_id],
                deletes: vec![],
            })
            .await;

        Ok(revision)
    }

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

            if self.upsert_mcp_record_inner(record).await.is_ok() {
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
        // Create KV store for events via factory, allow separate config / 事件KV支持独立配置
        let supported = get_kv_store_factory().supported_backends();
        let kv_cfg = if let Some(ev) = &config.event_kv {
            let backend = if supported.contains(&ev.backend) {
                ev.backend.clone()
            } else {
                "memory".to_string()
            };
            KvStoreConfig {
                backend,
                params: ev.params.clone(),
            }
        } else {
            KvStoreConfig {
                backend: "memory".to_string(),
                params: std::collections::HashMap::new(),
            }
        };
        let kv_box = create_kv_store_from_config(&kv_cfg)
            .await
            .expect("Failed to create KV store from config");
        let kv: Arc<dyn crate::storage::kv::KvStore> = Arc::from(kv_box);
        let unified_events = Arc::new(UnifiedEventBus::new(kv.clone()));
        let events = Arc::new(TaskEventBus::new(kv.clone()));
        let stale_after_ms = (config.heartbeat_timeout as i64).saturating_mul(2_000);
        let instance_execution_index =
            Arc::new(InstanceExecutionIndex::new(kv, 256, 1000, stale_after_ms));

        let admin_kv_cfg = {
            let backend = if supported.contains(&config.database.db_type) {
                config.database.db_type.clone()
            } else {
                "memory".to_string()
            };
            let mut params = std::collections::HashMap::new();
            if backend != "memory" {
                params.insert("path".to_string(), config.database.path.clone());
            }
            KvStoreConfig { backend, params }
        };
        let admin_kv_box = match create_kv_store_from_config(&admin_kv_cfg).await {
            Ok(v) => v,
            Err(_) => create_kv_store_from_config(&KvStoreConfig {
                backend: "memory".to_string(),
                params: std::collections::HashMap::new(),
            })
            .await
            .expect("Failed to create admin KV store"),
        };
        let admin_kv: Arc<dyn crate::storage::kv::KvStore> = Arc::from(admin_kv_box);
        let admin_llm_config = Arc::new(AdminLlmConfigState::new(admin_kv).await);

        {
            let idx = instance_execution_index.clone();
            let bus = unified_events.clone();
            tokio::spawn(async move {
                run_instance_projector(idx, bus).await;
            });
        }
        {
            let idx = instance_execution_index.clone();
            let bus = unified_events.clone();
            tokio::spawn(async move {
                run_execution_projector(idx, bus).await;
            });
        }

        let cleanup_node_service = node_service.clone();
        let cleanup_resource_service = resource_service.clone();
        let cleanup_config = config.clone();
        let cleanup_unified_events = unified_events.clone();
        tokio::spawn(async move {
            let mut t = tokio::time::interval(Duration::from_secs(cleanup_config.cleanup_interval));
            loop {
                t.tick().await;
                let updated_nodes = {
                    let mut svc = cleanup_node_service.write().await;
                    svc.mark_unhealthy_nodes_offline(cleanup_config.heartbeat_timeout)
                        .await
                        .unwrap_or_default()
                };
                if !updated_nodes.is_empty() {
                    tracing::info!(
                        count = updated_nodes.len(),
                        heartbeat_timeout_s = cleanup_config.heartbeat_timeout,
                        nodes = ?updated_nodes,
                        "Marked unhealthy nodes offline"
                    );
                    for mark in updated_nodes.iter() {
                        if mark.previous_status.to_ascii_lowercase() == "offline" {
                            continue;
                        }
                        if let Err(e) = cleanup_unified_events
                            .publish_node_event(&mark.node, EventOp::Update)
                            .await
                        {
                            warn!(error = %e, uuid = %mark.uuid, "Publish unified node offline event failed");
                        }
                    }
                }
                let _ = cleanup_resource_service
                    .cleanup_stale_resources(cleanup_config.heartbeat_timeout)
                    .await;
            }
        });

        Self {
            node_service,
            resource_service,
            config,
            task_service,
            events,
            unified_events,
            instance_execution_index,
            placement_state: Arc::new(PlacementState::new()),
            mcp_registry: Arc::new(McpRegistryState::new(1024, 1024)),
            backend_registry: Arc::new(BackendRegistryState::new()),
            model_deployment_registry: Arc::new(ModelDeploymentRegistryState::new(1024, 1024)),
            admin_llm_config,
            router_filter_engine: Arc::new(RouterFilterEngine::Builtin(
                BuiltinRouterFilterEngine::default(),
            )),
        }
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

    /// Get resource service reference / 获取资源服务引用
    pub fn resource_service(&self) -> Arc<ResourceService> {
        self.resource_service.clone()
    }

    /// Get task service reference / 获取任务服务引用
    pub fn task_service(&self) -> Arc<RwLock<TaskServiceImpl>> {
        self.task_service.clone()
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
        let revision = self.mcp_registry.current_revision();
        let records = self.mcp_registry.records.read().await;
        let servers = records.values().cloned().collect::<Vec<_>>();
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

        let revision = self.upsert_mcp_record_inner(record).await?;
        Ok(Response::new(UpsertMcpServerResponse { revision }))
    }

    async fn delete_mcp_server(
        &self,
        request: Request<DeleteMcpServerRequest>,
    ) -> Result<Response<DeleteMcpServerResponse>, Status> {
        let server_id = request.into_inner().server_id;
        if server_id.is_empty() {
            return Err(Status::invalid_argument("server_id is required"));
        }

        let existed = {
            let mut records = self.mcp_registry.records.write().await;
            records.remove(&server_id).is_some()
        };
        if !existed {
            return Err(Status::not_found("server not found"));
        }

        let revision = self.mcp_registry.bump_revision();
        self.mcp_registry
            .push_event(McpRegistryEvent {
                revision,
                upserts: vec![],
                deletes: vec![server_id],
            })
            .await;

        Ok(Response::new(DeleteMcpServerResponse { revision }))
    }
}

#[tonic::async_trait]
impl ModelDeploymentRegistryServiceTrait for SmsServiceImpl {
    type WatchModelDeploymentsStream = std::pin::Pin<
        Box<
            dyn tokio_stream::Stream<Item = Result<WatchModelDeploymentsResponse, Status>>
                + Send
                + 'static,
        >,
    >;

    async fn list_model_deployments(
        &self,
        request: Request<ListModelDeploymentsRequest>,
    ) -> Result<Response<ListModelDeploymentsResponse>, Status> {
        let req = request.into_inner();
        let limit = if req.limit == 0 {
            200
        } else {
            req.limit.min(500)
        };
        let offset = req.offset;
        let filter_node = req.target_node_uuid.trim().to_string();
        let filter_provider = req.provider.trim().to_string();

        let registry_revision = self.model_deployment_registry.watch.current_revision();
        let guard = self.model_deployment_registry.records.read().await;
        let mut list = guard
            .values()
            .cloned()
            .filter(|r| {
                if !filter_node.is_empty() {
                    r.spec
                        .as_ref()
                        .map(|s| s.target_node_uuid == filter_node)
                        .unwrap_or(false)
                } else {
                    true
                }
            })
            .filter(|r| {
                if !filter_provider.is_empty() {
                    r.spec
                        .as_ref()
                        .map(|s| s.provider == filter_provider)
                        .unwrap_or(false)
                } else {
                    true
                }
            })
            .collect::<Vec<_>>();
        list.sort_by(|a, b| b.updated_at_ms.cmp(&a.updated_at_ms));

        let total_count = list.len() as u32;
        let start = offset as usize;
        let end = start.saturating_add(limit as usize);
        let page = if start >= list.len() {
            Vec::new()
        } else {
            list[start..list.len().min(end)].to_vec()
        };

        Ok(Response::new(ListModelDeploymentsResponse {
            revision: registry_revision,
            records: page,
            total_count,
        }))
    }

    async fn watch_model_deployments(
        &self,
        request: Request<WatchModelDeploymentsRequest>,
    ) -> Result<Response<Self::WatchModelDeploymentsStream>, Status> {
        let req = request.into_inner();
        if req.target_node_uuid.trim().is_empty() {
            return Err(Status::invalid_argument("target_node_uuid is required"));
        }
        let node_filter = req.target_node_uuid.clone();
        let registry = self.model_deployment_registry.clone();
        let watch_cursor_revision = req.since_revision;

        let base = self
            .model_deployment_registry
            .watch
            .watch(watch_cursor_revision, |e| e.revision)
            .await?;
        let stream = base
            .then(move |r| {
                let node_filter = node_filter.clone();
                let registry = registry.clone();
                async move {
                    match r {
                        Ok(mut event) => {
                            let map = registry.id_to_node.read().await;
                            event.upserts.retain(|id| {
                                map.get(id).map(|n| n == &node_filter).unwrap_or(false)
                            });
                            event.deletes.retain(|id| {
                                map.get(id).map(|n| n == &node_filter).unwrap_or(true)
                            });
                            if event.upserts.is_empty() && event.deletes.is_empty() {
                                None
                            } else {
                                Some(Ok(WatchModelDeploymentsResponse { event: Some(event) }))
                            }
                        }
                        Err(e) => Some(Err(e)),
                    }
                }
            })
            .filter_map(|x| x);

        Ok(Response::new(Box::pin(stream)))
    }

    async fn upsert_model_deployment(
        &self,
        request: Request<UpsertModelDeploymentRequest>,
    ) -> Result<Response<UpsertModelDeploymentResponse>, Status> {
        let mut record = request
            .into_inner()
            .record
            .ok_or_else(|| Status::invalid_argument("record is required"))?;
        let target_node_uuid = {
            let spec = record
                .spec
                .as_ref()
                .ok_or_else(|| Status::invalid_argument("spec is required"))?;
            if spec.target_node_uuid.trim().is_empty() {
                return Err(Status::invalid_argument(
                    "spec.target_node_uuid is required",
                ));
            }
            if spec.provider.trim().is_empty() {
                return Err(Status::invalid_argument("spec.provider is required"));
            }
            if spec.model.trim().is_empty() {
                return Err(Status::invalid_argument("spec.model is required"));
            }
            spec.target_node_uuid.clone()
        };

        let now_ms = chrono::Utc::now().timestamp_millis();
        let deployment_id = if record.deployment_id.trim().is_empty() {
            uuid::Uuid::new_v4().to_string()
        } else {
            record.deployment_id.clone()
        };

        let mut guard = self.model_deployment_registry.records.write().await;
        let created_at_ms = guard
            .get(&deployment_id)
            .map(|r| r.created_at_ms)
            .filter(|v| *v > 0)
            .unwrap_or(now_ms);

        let new_registry_revision = self.model_deployment_registry.watch.bump_revision();
        record.deployment_id = deployment_id.clone();
        record.revision = new_registry_revision;
        record.created_at_ms = created_at_ms;
        record.updated_at_ms = now_ms;
        if record.status.is_none() {
            record.status = Some(ModelDeploymentStatus {
                phase: crate::proto::sms::ModelDeploymentPhase::Pending as i32,
                message: String::new(),
                updated_at_ms: now_ms,
            });
        }

        guard.insert(deployment_id.clone(), record);
        drop(guard);

        self.model_deployment_registry
            .id_to_node
            .write()
            .await
            .insert(deployment_id.clone(), target_node_uuid);

        self.model_deployment_registry
            .watch
            .push_event(ModelDeploymentEvent {
                revision: new_registry_revision,
                upserts: vec![deployment_id.clone()],
                deletes: Vec::new(),
            })
            .await;

        Ok(Response::new(UpsertModelDeploymentResponse {
            revision: new_registry_revision,
            deployment_id,
        }))
    }

    async fn delete_model_deployment(
        &self,
        request: Request<DeleteModelDeploymentRequest>,
    ) -> Result<Response<DeleteModelDeploymentResponse>, Status> {
        let deployment_id = request.into_inner().deployment_id;
        if deployment_id.trim().is_empty() {
            return Err(Status::invalid_argument("deployment_id is required"));
        }
        let existed = {
            let mut guard = self.model_deployment_registry.records.write().await;
            guard.remove(&deployment_id).is_some()
        };
        if !existed {
            return Err(Status::not_found("deployment not found"));
        }

        {
            let mut map = self.model_deployment_registry.id_to_node.write().await;
            map.remove(&deployment_id);
        }

        let new_registry_revision = self.model_deployment_registry.watch.bump_revision();
        self.model_deployment_registry
            .watch
            .push_event(ModelDeploymentEvent {
                revision: new_registry_revision,
                upserts: Vec::new(),
                deletes: vec![deployment_id],
            })
            .await;

        Ok(Response::new(DeleteModelDeploymentResponse {
            revision: new_registry_revision,
        }))
    }

    async fn report_model_deployment_status(
        &self,
        request: Request<ReportModelDeploymentStatusRequest>,
    ) -> Result<Response<ReportModelDeploymentStatusResponse>, Status> {
        let req = request.into_inner();
        if req.deployment_id.trim().is_empty() {
            return Err(Status::invalid_argument("deployment_id is required"));
        }
        if req.node_uuid.trim().is_empty() {
            return Err(Status::invalid_argument("node_uuid is required"));
        }
        let mut status = req
            .status
            .ok_or_else(|| Status::invalid_argument("status is required"))?;
        if status.updated_at_ms == 0 {
            status.updated_at_ms = chrono::Utc::now().timestamp_millis();
        }

        let mut guard = self.model_deployment_registry.records.write().await;
        let Some(mut rec) = guard.get(&req.deployment_id).cloned() else {
            return Err(Status::not_found("deployment not found"));
        };
        let target_node_uuid = {
            let Some(spec) = rec.spec.as_ref() else {
                return Err(Status::failed_precondition("spec missing"));
            };
            if spec.target_node_uuid != req.node_uuid {
                return Err(Status::permission_denied("node_uuid mismatch"));
            }
            spec.target_node_uuid.clone()
        };
        let observed_record_revision = req.observed_revision;
        let current_record_revision = rec.revision;
        if observed_record_revision > 0 && observed_record_revision < current_record_revision {
            return Ok(Response::new(ReportModelDeploymentStatusResponse {
                success: false,
            }));
        }

        let new_registry_revision = self.model_deployment_registry.watch.bump_revision();
        rec.updated_at_ms = status.updated_at_ms;
        rec.status = Some(status);
        guard.insert(req.deployment_id.clone(), rec);
        drop(guard);

        self.model_deployment_registry
            .id_to_node
            .write()
            .await
            .insert(req.deployment_id.clone(), target_node_uuid);

        self.model_deployment_registry
            .watch
            .push_event(ModelDeploymentEvent {
                revision: new_registry_revision,
                upserts: vec![req.deployment_id],
                deletes: Vec::new(),
            })
            .await;

        Ok(Response::new(ReportModelDeploymentStatusResponse {
            success: true,
        }))
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

        snapshot.backends.retain(|b| !b.name.trim().is_empty());
        snapshot.backends.iter_mut().for_each(|b| {
            if b.kind.trim().is_empty() {
                b.kind = "unknown".to_string();
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
impl AdminLlmConfigServiceTrait for SmsServiceImpl {
    async fn list_remote_backends(
        &self,
        _request: Request<ListRemoteBackendsRequest>,
    ) -> Result<Response<ListRemoteBackendsResponse>, Status> {
        let snap = self.admin_llm_config.list().await;
        let backends = snap
            .backends
            .into_iter()
            .map(RemoteBackendConfig::from)
            .collect();
        Ok(Response::new(ListRemoteBackendsResponse {
            revision: snap.revision,
            backends,
        }))
    }

    async fn upsert_remote_backend(
        &self,
        request: Request<UpsertRemoteBackendRequest>,
    ) -> Result<Response<UpsertRemoteBackendResponse>, Status> {
        let backend = request
            .into_inner()
            .backend
            .ok_or_else(|| Status::invalid_argument("backend is required"))?;
        let revision = self.admin_llm_config.upsert(backend).await?;
        Ok(Response::new(UpsertRemoteBackendResponse { revision }))
    }

    async fn delete_remote_backend(
        &self,
        request: Request<DeleteRemoteBackendRequest>,
    ) -> Result<Response<DeleteRemoteBackendResponse>, Status> {
        let name = request.into_inner().name;
        let (revision, deleted) = self.admin_llm_config.delete(&name).await?;
        Ok(Response::new(DeleteRemoteBackendResponse {
            revision,
            deleted,
        }))
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
                if let Err(e) = self
                    .unified_events
                    .publish_node_event(&node, EventOp::Create)
                    .await
                {
                    warn!(error = %e, uuid = %node.uuid, "Publish unified node create event failed");
                }
                let response = RegisterNodeResponse {
                    node_uuid: node.uuid.clone(),
                    success: true,
                    message: "Node registered successfully".to_string(),
                };
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
                if let Err(e) = self
                    .unified_events
                    .publish_node_event(&node_for_event, EventOp::Update)
                    .await
                {
                    warn!(error = %e, uuid = %node_for_event.uuid, "Publish unified node update event failed");
                }
                let response = UpdateNodeResponse {
                    success: true,
                    message: "Node updated successfully".to_string(),
                };
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
                if let Err(e) = self
                    .unified_events
                    .publish_node_deleted(&node_uuid.to_string())
                    .await
                {
                    warn!(error = %e, uuid = %node_uuid, "Publish unified node delete event failed");
                }
                let response = DeleteNodeResponse {
                    success: true,
                    message: "Node deleted successfully".to_string(),
                };
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
                if let Some(node) = changed {
                    if let Err(e) = self
                        .unified_events
                        .publish_node_event(&node, EventOp::Update)
                        .await
                    {
                        warn!(error = %e, uuid = %node.uuid, "Publish unified node online event failed");
                    }
                }
                tracing::debug!(uuid = %req.uuid, "Heartbeat received");
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

// Implement TaskService trait / 实现TaskService trait
#[tonic::async_trait]
impl TaskServiceTrait for SmsServiceImpl {
    type SubscribeTaskEventsStream = std::pin::Pin<
        Box<
            dyn tokio_stream::Stream<Item = Result<crate::proto::sms::TaskEvent, Status>>
                + Send
                + 'static,
        >,
    >;
    /// Register a new task / 注册新任务
    async fn register_task(
        &self,
        request: Request<RegisterTaskRequest>,
    ) -> Result<Response<RegisterTaskResponse>, Status> {
        let req = request.into_inner();

        // Create task from request fields
        let task = crate::proto::sms::Task {
            task_id: uuid::Uuid::new_v4().to_string(),
            name: req.name,
            description: req.description,
            status: crate::proto::sms::TaskStatus::Registered as i32,
            priority: req.priority,
            node_uuid: req.node_uuid,
            endpoint: req.endpoint,
            version: req.version,
            capabilities: req.capabilities,
            registered_at: chrono::Utc::now().timestamp(),
            last_heartbeat: chrono::Utc::now().timestamp(),
            metadata: req.metadata,
            config: req.config,
            executable: req.executable,
            result_uris: Vec::new(),
            last_result_uri: String::new(),
            last_result_status: String::new(),
            last_completed_at: 0,
            last_result_metadata: std::collections::HashMap::new(),
        };

        let mut task_service = self.task_service.write().await;
        match task_service.register_task(task.clone()).await {
            Ok(_) => {
                debug!(task_id = %task.task_id, node_uuid = %task.node_uuid, "RegisterTask: publishing create event");
                // Publish create event / 发布创建事件
                if let Err(e) = self.events.publish_create(&task).await {
                    warn!(error = %e, "Publish create event failed");
                }
                if let Err(e) = self
                    .unified_events
                    .publish_task_event(&task, crate::proto::sms::TaskEventKind::Create)
                    .await
                {
                    warn!(error = %e, "Publish unified create event failed");
                }
                let response = RegisterTaskResponse {
                    success: true,
                    message: "Task registered successfully".to_string(),
                    task_id: task.task_id.clone(),
                    task: Some(task),
                };
                Ok(Response::new(response))
            }
            Err(e) => {
                let response = RegisterTaskResponse {
                    success: false,
                    message: format!("Failed to register task: {}", e),
                    task_id: String::new(),
                    task: None,
                };
                Ok(Response::new(response))
            }
        }
    }

    /// List tasks with optional filtering / 列出任务（可选过滤）
    async fn list_tasks(
        &self,
        request: Request<ListTasksRequest>,
    ) -> Result<Response<ListTasksResponse>, Status> {
        let req = request.into_inner();

        let task_service = self.task_service.read().await;

        // Convert filter parameters / 转换过滤参数
        let node_uuid = if req.node_uuid.is_empty() {
            None
        } else {
            Some(req.node_uuid.as_str())
        };
        let status_filter = if req.status_filter < 0 {
            None
        } else {
            Some(req.status_filter)
        };
        let priority_filter = if req.priority_filter < 0 {
            None
        } else {
            Some(req.priority_filter)
        };
        let limit = if req.limit <= 0 {
            None
        } else {
            Some(req.limit)
        };
        let offset = if req.offset < 0 {
            None
        } else {
            Some(req.offset)
        };

        match task_service
            .list_tasks_with_filters(node_uuid, status_filter, priority_filter, limit, offset)
            .await
        {
            Ok(tasks) => {
                // Get total count before filtering for pagination / 获取过滤前的总数用于分页
                let all_tasks = task_service.list_tasks().await.unwrap_or_default();
                let response = ListTasksResponse {
                    tasks: tasks.clone(),
                    total_count: all_tasks.len() as i32,
                };
                Ok(Response::new(response))
            }
            Err(e) => Err(Status::internal(format!("Failed to list tasks: {}", e))),
        }
    }

    /// Get task details by ID / 根据ID获取任务详情
    async fn get_task(
        &self,
        request: Request<GetTaskRequest>,
    ) -> Result<Response<GetTaskResponse>, Status> {
        let req = request.into_inner();

        let task_service = self.task_service.read().await;
        match task_service.get_task(&req.task_id).await {
            Ok(Some(task)) => {
                let response = GetTaskResponse {
                    found: true,
                    task: Some(task),
                };
                Ok(Response::new(response))
            }
            Ok(None) => {
                let response = GetTaskResponse {
                    found: false,
                    task: None,
                };
                Ok(Response::new(response))
            }
            Err(e) => Err(Status::internal(format!("Failed to get task: {}", e))),
        }
    }

    /// Resolve task by endpoint / 通过 endpoint 解析任务
    async fn resolve_endpoint(
        &self,
        request: Request<ResolveEndpointRequest>,
    ) -> Result<Response<ResolveEndpointResponse>, Status> {
        let req = request.into_inner();
        let task_service = self.task_service.read().await;
        match task_service.get_task_by_endpoint(&req.endpoint).await {
            Ok(Some(task)) => Ok(Response::new(ResolveEndpointResponse {
                found: true,
                task: Some(task),
            })),
            Ok(None) => Ok(Response::new(ResolveEndpointResponse {
                found: false,
                task: None,
            })),
            Err(e) => Err(Status::internal(format!(
                "Failed to resolve endpoint: {}",
                e
            ))),
        }
    }

    /// Unregister a task / 注销任务
    async fn unregister_task(
        &self,
        request: Request<UnregisterTaskRequest>,
    ) -> Result<Response<UnregisterTaskResponse>, Status> {
        let req = request.into_inner();

        let mut task_service = self.task_service.write().await;
        match task_service.remove_task(&req.task_id).await {
            Ok(_) => {
                let response = UnregisterTaskResponse {
                    success: true,
                    message: "Task unregistered successfully".to_string(),
                    task_id: req.task_id.clone(),
                };
                Ok(Response::new(response))
            }
            Err(e) => {
                let response = UnregisterTaskResponse {
                    success: false,
                    message: format!("Failed to unregister task: {}", e),
                    task_id: req.task_id.clone(),
                };
                Ok(Response::new(response))
            }
        }
    }

    /// Subscribe task events for a node / 订阅节点任务事件
    async fn subscribe_task_events(
        &self,
        request: Request<crate::proto::sms::SubscribeTaskEventsRequest>,
    ) -> Result<tonic::Response<Self::SubscribeTaskEventsStream>, Status> {
        let req = request.into_inner();
        if req.node_uuid.is_empty() {
            return Err(Status::invalid_argument("node_uuid is required"));
        }
        let node_uuid = req.node_uuid;
        let last = req.last_event_id;
        // Durable replay first
        let replay = self
            .events
            .replay_since(&node_uuid, last, 1000)
            .await
            .map_err(|e| Status::internal(format!("Replay failed: {}", e)))?;
        debug!(node_uuid = %node_uuid, last_event_id = last, replay_count = replay.len(), "Subscribe: prepared replay events");
        let replay_stream = tokio_stream::iter(replay.into_iter().map(Ok));
        // Live broadcast
        let rx = self.events.subscribe(&node_uuid).await;
        debug!(node_uuid = %node_uuid, "Subscribe: live broadcast receiver created");
        let live_stream = unfold(rx, |mut r| async move {
            match r.recv().await {
                Ok(ev) => Some((Ok(ev), r)),
                Err(e) => {
                    warn!(error = %e, "Broadcast receive error, ending live stream");
                    None
                }
            }
        });
        let stream = replay_stream.chain(live_stream);
        debug!(node_uuid = %node_uuid, "Subscribe: returning combined stream");
        Ok(tonic::Response::new(Box::pin(stream)))
    }

    /// Update task status (observed state) / 更新任务状态（观测态）
    async fn update_task_status(
        &self,
        request: Request<UpdateTaskStatusRequest>,
    ) -> Result<Response<UpdateTaskStatusResponse>, Status> {
        let req = request.into_inner();
        debug!(task_id = %req.task_id, node_uuid = %req.node_uuid, status = req.status, status_version = req.status_version, updated_at = req.updated_at, reason = %req.reason, "UpdateTaskStatus: request received");
        if req.task_id.is_empty() {
            return Err(Status::invalid_argument("task_id is required"));
        }

        let mut task_service = self.task_service.write().await;
        match task_service.get_task(&req.task_id).await {
            Ok(Some(mut task)) => {
                // Apply status update / 应用状态更新
                let old_status = task.status;
                task.status = req.status;
                if req.updated_at > 0 {
                    task.last_heartbeat = req.updated_at;
                } else {
                    task.last_heartbeat = chrono::Utc::now().timestamp();
                }
                debug!(task_id = %task.task_id, old_status = old_status, new_status = task.status, last_heartbeat = task.last_heartbeat, "UpdateTaskStatus: applied state change");
                // Persist / 持久化
                match task_service.register_task(task.clone()).await {
                    Ok(_) => {
                        debug!(task_id = %task.task_id, "UpdateTaskStatus: persisted");
                    }
                    Err(e) => {
                        warn!(error = %e.to_string(), task_id = %task.task_id, "UpdateTaskStatus: persist failed");
                    }
                }

                // Optionally publish update event / 可选发布更新事件
                if let Err(e) = self.events.publish_update(&task).await {
                    warn!(error = %e, task_id = %task.task_id, "Publish update event failed");
                }
                if let Err(e) = self
                    .unified_events
                    .publish_task_event(&task, crate::proto::sms::TaskEventKind::Update)
                    .await
                {
                    warn!(error = %e, task_id = %task.task_id, "Publish unified update event failed");
                }

                let resp = UpdateTaskStatusResponse {
                    success: true,
                    message: "Task status updated".to_string(),
                    task: Some(task),
                };
                Ok(Response::new(resp))
            }
            Ok(None) => {
                debug!(task_id = %req.task_id, "UpdateTaskStatus: task not found");
                let resp = UpdateTaskStatusResponse {
                    success: false,
                    message: "Task not found".to_string(),
                    task: None,
                };
                Ok(Response::new(resp))
            }
            Err(e) => Err(Status::internal(format!("Failed to get task: {}", e))),
        }
    }

    /// Update task result fields / 更新任务结果字段
    async fn update_task_result(
        &self,
        request: Request<UpdateTaskResultRequest>,
    ) -> Result<Response<UpdateTaskResultResponse>, Status> {
        let req = request.into_inner();
        if req.task_id.is_empty() {
            return Err(Status::invalid_argument("task_id is required"));
        }
        let mut task_service = self.task_service.write().await;
        match task_service.get_task(&req.task_id).await {
            Ok(Some(mut task)) => {
                if !req.result_uri.is_empty() {
                    task.last_result_uri = req.result_uri.clone();
                    if !task.result_uris.contains(&req.result_uri) {
                        task.result_uris.push(req.result_uri.clone());
                    }
                }
                task.last_result_status = req.result_status.clone();
                task.last_completed_at = if req.completed_at > 0 {
                    req.completed_at
                } else {
                    chrono::Utc::now().timestamp()
                };
                task.last_result_metadata = req.result_metadata.clone();

                match task_service.register_task(task.clone()).await {
                    Ok(_) => {}
                    Err(e) => return Err(Status::internal(format!("Persist failed: {}", e))),
                }

                if let Err(e) = self.events.publish_update(&task).await {
                    warn!(error = %e, task_id = %task.task_id, "Publish update event failed");
                }
                if let Err(e) = self
                    .unified_events
                    .publish_task_event(&task, crate::proto::sms::TaskEventKind::Update)
                    .await
                {
                    warn!(error = %e, task_id = %task.task_id, "Publish unified update event failed");
                }
                let resp = UpdateTaskResultResponse {
                    success: true,
                    message: "Task result updated".to_string(),
                    task: Some(task),
                };
                Ok(Response::new(resp))
            }
            Ok(None) => Ok(Response::new(UpdateTaskResultResponse {
                success: false,
                message: "Task not found".to_string(),
                task: None,
            })),
            Err(e) => Err(Status::internal(format!("Failed to get task: {}", e))),
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
            .upsert_instance(inst.clone())
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        if accepted {
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
            .upsert_execution(exe.clone())
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

#[derive(Debug, Clone)]
struct NodePenalty {
    // Consecutive retryable failures to drive exponential backoff.
    // 连续可重试失败次数，用于指数退避。
    consecutive_failures: u32,
    // If now < blocked_until, the node is temporarily removed from candidate set.
    // 若 now < blocked_until，则节点被临时熔断，不参与候选。
    blocked_until: i64,
    // Timestamp of last failure.
    // 最近一次失败的时间戳。
    last_failure_at: i64,
}

#[derive(Debug)]
struct PlacementState {
    node_penalties: DashMap<String, NodePenalty>,
    penalty_ops: AtomicU64,
}

impl PlacementState {
    fn new() -> Self {
        Self {
            node_penalties: DashMap::new(),
            penalty_ops: AtomicU64::new(0),
        }
    }

    fn maybe_prune_node_penalties(&self, now: i64) {
        const PENALTY_TTL_SECS: i64 = 3600;

        let op = self.penalty_ops.fetch_add(1, Ordering::Relaxed);
        if op % 256 != 0 {
            return;
        }

        let mut to_remove: Vec<String> = Vec::new();
        for item in self.node_penalties.iter() {
            let p = item.value();
            if p.blocked_until > now {
                continue;
            }
            if p.last_failure_at == 0 {
                to_remove.push(item.key().clone());
                continue;
            }
            if now - p.last_failure_at > PENALTY_TTL_SECS {
                to_remove.push(item.key().clone());
            }
        }
        for k in to_remove {
            self.node_penalties.remove(&k);
        }
    }

    fn is_blocked(&self, node_uuid: &str, now: i64) -> bool {
        self.node_penalties
            .get(node_uuid)
            .map(|p| p.blocked_until > now)
            .unwrap_or(false)
    }

    fn penalty_score(&self, node_uuid: &str, now: i64) -> f64 {
        self.node_penalties
            .get(node_uuid)
            .map(|p| {
                if p.blocked_until > now {
                    10.0
                } else {
                    (p.consecutive_failures as f64).min(10.0)
                }
            })
            .unwrap_or(0.0)
    }

    fn apply_outcome(&self, node_uuid: String, outcome_class: InvocationOutcomeClass) {
        let now = chrono::Utc::now().timestamp();
        match outcome_class {
            InvocationOutcomeClass::Success => {
                // Success clears penalty state.
                // 成功会清空惩罚状态。
                self.node_penalties.remove(&node_uuid);
            }
            InvocationOutcomeClass::Overloaded
            | InvocationOutcomeClass::Unavailable
            | InvocationOutcomeClass::Timeout => {
                // Retryable failures trigger exponential backoff with a hard cap.
                // 可重试失败触发指数退避，并设置硬上限。
                self.node_penalties
                    .entry(node_uuid)
                    .and_modify(|p| {
                        p.consecutive_failures = p.consecutive_failures.saturating_add(1);
                        p.last_failure_at = now;
                        let base = 10i64;
                        let backoff = base * (1i64 << (p.consecutive_failures.min(5)));
                        p.blocked_until = (now + backoff).min(now + 300);
                    })
                    .or_insert(NodePenalty {
                        consecutive_failures: 1,
                        blocked_until: (now + 20).min(now + 300),
                        last_failure_at: now,
                    });
            }
            _ => {}
        }

        self.maybe_prune_node_penalties(now);
    }

    #[cfg(test)]
    fn get_node_penalty_snapshot(&self, node_uuid: &str) -> Option<(u32, i64, i64)> {
        self.node_penalties
            .get(node_uuid)
            .map(|p| (p.consecutive_failures, p.blocked_until, p.last_failure_at))
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
        let now = chrono::Utc::now().timestamp();
        self.placement_state.maybe_prune_node_penalties(now);
        let nodes = {
            let svc = self.node_service.read().await;
            svc.list_nodes()
                .await
                .map_err(|e| Status::internal(e.to_string()))?
        };
        let heartbeat_timeout = self.config.heartbeat_timeout as i64;
        let mut candidates: Vec<(NodeCandidate, f64)> = Vec::new();
        for node in nodes {
            // Filter nodes by liveness and heartbeat freshness.
            // 先按存活状态与心跳新鲜度过滤。
            if node.status.to_ascii_lowercase() != "online" {
                continue;
            }
            if now - node.last_heartbeat > heartbeat_timeout {
                continue;
            }
            // Skip nodes in temporary circuit-break.
            // 熔断中的节点不参与候选。
            if self.placement_state.is_blocked(&node.uuid, now) {
                continue;
            }

            let uuid = uuid::Uuid::parse_str(&node.uuid).ok();
            let resource = if let Some(u) = uuid {
                self.resource_service.get_resource(&u).await.ok().flatten()
            } else {
                None
            };
            let (cpu, mem, disk, load) = if let Some(r) = resource {
                (
                    r.cpu_usage_percent,
                    r.memory_usage_percent,
                    r.disk_usage_percent,
                    r.load_average_1m,
                )
            } else {
                (0.0, 0.0, 0.0, 0.0)
            };
            let mut score = 100.0;
            // Simple weighted scoring: lower usage/load => higher score.
            // 简单加权评分：资源占用/负载越低，分数越高。
            score -= cpu.min(100.0) * 0.5;
            score -= mem.min(100.0) * 0.3;
            score -= disk.min(100.0) * 0.1;
            score -= (load.min(16.0) / 16.0) * 10.0;
            // Apply penalty score derived from historical retryable failures.
            // 基于历史可重试失败的惩罚项。
            score -= self.placement_state.penalty_score(&node.uuid, now) * 5.0;
            let candidate = NodeCandidate {
                node_uuid: node.uuid.clone(),
                ip_address: node.ip_address.clone(),
                port: node.port,
                score,
            };
            candidates.push((candidate, score));
        }

        candidates.sort_by(|a, b| b.1.total_cmp(&a.1));
        candidates.truncate(max_candidates as usize);

        let decision_id = Uuid::new_v4().to_string();

        let resp = PlaceInvocationResponse {
            decision_id,
            candidates: candidates.into_iter().map(|(c, _)| c).collect(),
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
        let outcome_class = InvocationOutcomeClass::try_from(req.outcome_class)
            .unwrap_or(InvocationOutcomeClass::Unknown);
        // Feedback loop: update penalty state to influence subsequent placements.
        // 反馈闭环：更新惩罚状态，影响后续 placement。
        self.placement_state
            .apply_outcome(req.node_uuid, outcome_class);
        Ok(Response::new(ReportInvocationOutcomeResponse {
            accepted: true,
        }))
    }
}

async fn run_instance_projector(idx: Arc<InstanceExecutionIndex>, bus: Arc<UnifiedEventBus>) {
    let stream = "type.instance";
    let checkpoint_name = "type.instance";
    let replay_limit = 1000usize;
    let mut last_seq = idx.load_checkpoint(checkpoint_name).await.unwrap_or(0);
    loop {
        let batch = bus.replay_since(stream, last_seq, replay_limit).await;
        let Ok(events) = batch else {
            tokio::time::sleep(Duration::from_millis(200)).await;
            continue;
        };
        if events.is_empty() {
            break;
        }
        for env in events {
            if env.seq <= last_seq {
                continue;
            }
            if let Some(any) = env.payload {
                let now_ms = chrono::Utc::now().timestamp_millis();
                let _ = idx.apply_instance_event(env.op, &any, now_ms).await;
            }
            last_seq = env.seq;
            let _ = idx.store_checkpoint(checkpoint_name, last_seq).await;
        }
    }

    let mut rx = bus.subscribe(stream).await;
    loop {
        match rx.recv().await {
            Ok(env) => {
                if env.seq <= last_seq {
                    continue;
                }
                if let Some(any) = env.payload {
                    let now_ms = chrono::Utc::now().timestamp_millis();
                    let _ = idx.apply_instance_event(env.op, &any, now_ms).await;
                }
                last_seq = env.seq;
                let _ = idx.store_checkpoint(checkpoint_name, last_seq).await;
            }
            Err(broadcast::error::RecvError::Lagged(_)) => {}
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}

async fn run_execution_projector(idx: Arc<InstanceExecutionIndex>, bus: Arc<UnifiedEventBus>) {
    let stream = "type.execution";
    let checkpoint_name = "type.execution";
    let replay_limit = 1000usize;
    let mut last_seq = idx.load_checkpoint(checkpoint_name).await.unwrap_or(0);
    loop {
        let batch = bus.replay_since(stream, last_seq, replay_limit).await;
        let Ok(events) = batch else {
            tokio::time::sleep(Duration::from_millis(200)).await;
            continue;
        };
        if events.is_empty() {
            break;
        }
        for env in events {
            if env.seq <= last_seq {
                continue;
            }
            if let Some(any) = env.payload {
                let now_ms = chrono::Utc::now().timestamp_millis();
                let _ = idx.apply_execution_event(env.op, &any, now_ms).await;
            }
            last_seq = env.seq;
            let _ = idx.store_checkpoint(checkpoint_name, last_seq).await;
        }
    }

    let mut rx = bus.subscribe(stream).await;
    loop {
        match rx.recv().await {
            Ok(env) => {
                if env.seq <= last_seq {
                    continue;
                }
                if let Some(any) = env.payload {
                    let now_ms = chrono::Utc::now().timestamp_millis();
                    let _ = idx.apply_execution_event(env.op, &any, now_ms).await;
                }
                last_seq = env.seq;
                let _ = idx.store_checkpoint(checkpoint_name, last_seq).await;
            }
            Err(broadcast::error::RecvError::Lagged(_)) => {}
            Err(broadcast::error::RecvError::Closed) => break,
        }
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

#[cfg(test)]
impl SmsServiceImpl {
    pub fn test_get_node_penalty_snapshot(&self, node_uuid: &str) -> Option<(u32, i64, i64)> {
        self.placement_state.get_node_penalty_snapshot(node_uuid)
    }
}
