//! Typed domain models for unified AI backends / 统一 AI backend 的强类型领域模型

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Hosting mode for a backend / backend 的托管模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum AiBackendHostingModel {
    /// Remote endpoint managed by SMS / 由 SMS 管理的远端端点
    Remote,
    /// Local runtime materialized on nodes / 在节点上落地的本地运行时
    Local,
}

/// Desired lifecycle state / 期望生命周期状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum AiBackendDesiredStateModel {
    /// Backend should be active / backend 应处于启用状态
    Enabled,
    /// Backend should be inactive / backend 应处于禁用状态
    Disabled,
}

impl AiBackendDesiredStateModel {
    /// Check whether the state is enabled / 检查该状态是否为启用
    pub fn is_enabled(self) -> bool {
        matches!(self, Self::Enabled)
    }
}

/// Ownership mode for the backend / backend 的管理归属模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum AiBackendManagementModeModel {
    /// SMS owns a remote backend definition / SMS 管理远端 backend 定义
    SmsRemote,
    /// SMS owns a local deployment definition / SMS 管理本地部署定义
    SmsLocal,
}

/// Node-side runtime status / 节点侧运行时状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum AiBackendNodeRuntimeStatusModel {
    /// Waiting for reconcile / 等待收敛
    Pending,
    /// Being reconciled / 正在收敛
    Reconciling,
    /// Ready to serve traffic / 已准备好提供服务
    Ready,
    /// Running with degraded capability / 可用但处于降级状态
    Degraded,
    /// Failed to reconcile or run / 收敛或运行失败
    Error,
    /// Explicitly disabled on the node / 在节点上被显式禁用
    Disabled,
}

/// Backend execution specification mirrored from proto / 从 proto 镜像到领域层的 backend 执行规格
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiBackendSpecModel {
    pub name: String,
    pub kind: String,
    pub operations: Vec<String>,
    pub features: Vec<String>,
    pub transports: Vec<String>,
    pub weight: u32,
    pub priority: i32,
    pub base_url: String,
    pub provider: String,
    pub model: String,
    pub credential_ref: String,
    pub origin: i32,
    pub deployment_id: String,
}

impl AiBackendSpecModel {
    /// Build a deterministic runtime-visible name / 构建稳定的运行时可见名称
    pub fn generated_name(backend_id: &str) -> String {
        format!("backend-{backend_id}")
    }
}

/// Canonical backend control-plane record / 规范的 backend 控制面记录
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiBackendRecordModel {
    pub backend_id: String,
    pub display_name: String,
    pub provider: String,
    pub model: String,
    pub hosting: AiBackendHostingModel,
    pub backend_kind: String,
    pub desired_state: AiBackendDesiredStateModel,
    pub management_mode: AiBackendManagementModeModel,
    pub credential_ref: Option<String>,
    pub spec: AiBackendSpecModel,
    pub labels: BTreeMap<String, String>,
    pub metadata: serde_json::Value,
    pub generation: u64,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

impl AiBackendRecordModel {
    /// Whether the backend should currently be scheduled / backend 当前是否应被调度
    pub fn is_enabled(&self) -> bool {
        self.desired_state.is_enabled()
    }
}

/// Placement record bound to a node / 绑定到节点的 placement 记录
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiBackendPlacementRecordModel {
    pub placement_id: String,
    pub backend_id: String,
    pub node_uuid: String,
    pub desired_state: AiBackendDesiredStateModel,
    pub weight_override: Option<i32>,
    pub priority_override: Option<i32>,
    pub generation: u64,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

impl AiBackendPlacementRecordModel {
    /// Whether the placement should currently be active / placement 当前是否应生效
    pub fn is_enabled(&self) -> bool {
        self.desired_state.is_enabled()
    }
}

/// Latest node-observed status for one backend / 单个 backend 的节点最新观察状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiBackendNodeStatusRecordModel {
    pub backend_id: String,
    pub node_uuid: String,
    pub observed_generation: u64,
    pub status: AiBackendNodeRuntimeStatusModel,
    pub status_reason: String,
    pub runtime_backend_name: String,
    pub endpoint: String,
    pub available: bool,
    pub operations: Vec<String>,
    pub features: Vec<String>,
    pub transports: Vec<String>,
    pub last_heartbeat_at_ms: i64,
}

/// Resolved node assignment used by controllers / 控制器使用的解析后节点分配
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedBackendAssignment {
    pub backend: AiBackendRecordModel,
    pub placement: AiBackendPlacementRecordModel,
}

/// One backend instance projected into the read model / 投影到只读模型中的单个 backend 实例
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiModelViewInstance {
    pub backend_id: String,
    pub node_uuid: String,
    pub placement_state: AiBackendDesiredStateModel,
    pub backend_state: AiBackendDesiredStateModel,
    pub runtime_status: Option<AiBackendNodeRuntimeStatusModel>,
    pub runtime_backend_name: Option<String>,
    pub endpoint: Option<String>,
    pub available: bool,
}

/// Read-only model aggregation used by operator-facing UIs / 面向运维 UI 的只读聚合模型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiModelView {
    pub provider: String,
    pub model: String,
    pub hosting: AiBackendHostingModel,
    pub backend_ids: Vec<String>,
    pub operations: Vec<String>,
    pub features: Vec<String>,
    pub transports: Vec<String>,
    pub enabled_nodes: usize,
    pub ready_nodes: usize,
    pub total_nodes: usize,
    pub instances: Vec<AiModelViewInstance>,
}

impl AiModelView {
    /// Create a deterministic empty read model shell / 创建稳定的空只读模型骨架
    pub fn new(provider: String, model: String, hosting: AiBackendHostingModel) -> Self {
        Self {
            provider,
            model,
            hosting,
            backend_ids: Vec::new(),
            operations: Vec::new(),
            features: Vec::new(),
            transports: Vec::new(),
            enabled_nodes: 0,
            ready_nodes: 0,
            total_nodes: 0,
            instances: Vec::new(),
        }
    }

    /// Normalize sets into sorted vectors / 把集合归一化为稳定排序的数组
    pub fn finalize_sets(
        &mut self,
        backend_ids: BTreeSet<String>,
        operations: BTreeSet<String>,
        features: BTreeSet<String>,
        transports: BTreeSet<String>,
    ) {
        self.backend_ids = backend_ids.into_iter().collect();
        self.operations = operations.into_iter().collect();
        self.features = features.into_iter().collect();
        self.transports = transports.into_iter().collect();
    }
}
