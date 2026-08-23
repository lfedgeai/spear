//! Assignment-driven AI backend controller.
//! 基于 assignment 的 AI backend 控制器。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;

use chrono::Utc;
use prost_types::value::Kind as ProstValueKind;
use reqwest::Client;
use tokio::sync::oneshot;
use tokio::time::{interval, MissedTickBehavior};
use tokio_util::sync::CancellationToken;
use tonic::transport::Channel;

use crate::proto::sms::ai_backend_control_plane_service_client::AiBackendControlPlaneServiceClient;
use crate::proto::sms::{
    AiBackendHosting, AiBackendNodeStatus, AiBackendNodeStatusRecord, BackendInfo,
    ListAiBackendAssignmentsRequest, ReportAiBackendNodeStatusesRequest,
    ResolvedAiBackendAssignment,
};
use crate::spearlet::ai::backend_assembly::{apply_runtime_overrides, available_backend_info};
use crate::spearlet::ai::credential_resolver::CredentialResolver;
use crate::spearlet::ai::dynamic_backend_registry::{
    global_dynamic_backends, DynamicBackendSource,
};
use crate::spearlet::ai::remote_preflight::{default_requested_checks, preflight_remote_backend};
use crate::spearlet::backend_reporter::BackendReportTrigger;
use crate::spearlet::config::SpearletConfig;
use crate::spearlet::controller::Controller;
use crate::spearlet::local_models::llamacpp::LlamaCppSupervisor;
use crate::spearlet::local_models::provider::{
    resolve_local_provider_kind_for_assignment, LocalProviderKind,
};
use crate::spearlet::local_models::vllm::{
    VllmPreparedAssignment, VllmSupervisor,
};

/// Controller that materializes control-plane assignments into runtime backends.
/// 将控制面 assignment 落地为运行时 backend 的控制器。
pub struct BackendAssignmentController {
    config: Arc<SpearletConfig>,
    sms_channel: Channel,
    poll_interval: Duration,
    http: Client,
    llamacpp: LlamaCppSupervisor,
    vllm: VllmSupervisor,
    cancel: CancellationToken,
    backend_report_trigger: BackendReportTrigger,
    started: AtomicBool,
    self_weak: Weak<BackendAssignmentController>,
}

/// Typed local runtime family derived from one local assignment.
/// 从单个 local assignment 推导出的强类型运行时家族。
#[derive(Debug, Clone, PartialEq, Eq)]
enum LocalAssignmentRuntimeFamily {
    /// llama.cpp managed local runtime / llama.cpp 托管本地运行时
    LlamaCpp,
    /// vLLM local runtime family / vLLM 本地运行时家族
    Vllm,
    /// Unsupported local runtime family / 不支持的本地运行时家族
    Unsupported(String),
}

impl LocalAssignmentRuntimeFamily {
    /// Classify one local assignment into a runtime family.
    /// 将一个 local assignment 分类为运行时家族。
    fn from_assignment(
        backend: &crate::proto::sms::AiBackendRecord,
        spec: &crate::proto::sms::BackendSpec,
    ) -> Self {
        match resolve_local_provider_kind_for_assignment(
            &backend.backend_kind,
            &spec.kind,
            &spec.provider,
        ) {
            LocalProviderKind::LlamaCpp => Self::LlamaCpp,
            LocalProviderKind::Vllm => Self::Vllm,
            LocalProviderKind::Unsupported(kind) => Self::Unsupported(kind),
        }
    }
}

/// Typed llama.cpp assignment params derived from backend metadata.
/// 从 backend metadata 派生的强类型 llama.cpp assignment 参数。
#[derive(Debug, Clone)]
struct LlamaCppAssignmentParams {
    raw: std::collections::HashMap<String, String>,
    should_pull: bool,
}

impl LlamaCppAssignmentParams {
    /// Build typed llama.cpp params from optional backend metadata.
    /// 从可选 backend metadata 构建强类型 llama.cpp 参数。
    fn from_metadata(metadata: Option<&prost_types::Struct>) -> Self {
        let raw = metadata_to_string_map(metadata);
        let should_pull = compute_llamacpp_should_pull(&raw);
        Self { raw, should_pull }
    }

    /// Borrow the raw metadata map for downstream supervisor calls.
    /// 为下游 supervisor 调用借出原始 metadata 映射。
    fn raw(&self) -> &std::collections::HashMap<String, String> {
        &self.raw
    }

    /// Whether controller reconcile expects a pull/download stage.
    /// 控制器收敛是否预期会经过拉取/下载阶段。
    fn should_pull(&self) -> bool {
        self.should_pull
    }
}

/// Typed vLLM assignment params derived from backend metadata.
/// 从 backend metadata 派生的强类型 vLLM assignment 参数。
#[derive(Debug, Clone)]
struct VllmAssignmentParams {
    raw: std::collections::HashMap<String, String>,
}

impl VllmAssignmentParams {
    /// Build typed vLLM params from spec and optional backend metadata.
    /// 基于 spec 与可选 backend metadata 构建强类型 vLLM 参数。
    fn from_spec_and_metadata(
        _spec: &crate::proto::sms::BackendSpec,
        metadata: Option<&prost_types::Struct>,
    ) -> Self {
        let raw = metadata_to_string_map(metadata);
        Self { raw }
    }

    /// Borrow the raw metadata map for downstream supervisor calls.
    /// 为下游 supervisor 调用借出原始 metadata 映射。
    fn raw(&self) -> &std::collections::HashMap<String, String> {
        &self.raw
    }
}

impl BackendAssignmentController {
    /// Create a new assignment controller.
    /// 创建新的 assignment 控制器。
    pub fn new(
        config: Arc<SpearletConfig>,
        sms_channel: Channel,
        poll_interval: Duration,
        backend_report_trigger: BackendReportTrigger,
    ) -> Arc<Self> {
        Arc::new_cyclic(|weak| Self {
            http: Client::builder()
                .connect_timeout(Duration::from_secs(15))
                .build()
                .unwrap_or_else(|_| Client::new()),
            llamacpp: LlamaCppSupervisor::new(config.as_ref()),
            vllm: VllmSupervisor::new(config.as_ref()),
            config,
            sms_channel,
            poll_interval: poll_interval.max(Duration::from_secs(1)),
            cancel: CancellationToken::new(),
            backend_report_trigger,
            started: AtomicBool::new(false),
            self_weak: weak.clone(),
        })
    }

    /// Start the controller if it is not already running.
    /// 如果控制器尚未运行，则启动它。
    pub fn start(&self) {
        if self.started.swap(true, Ordering::AcqRel) {
            return;
        }
        if let Some(controller) = self.self_weak.upgrade() {
            controller.spawn();
        }
    }

    /// Stop the controller and clear its runtime contribution.
    /// 停止控制器并清理它写入的运行时 backend。
    pub fn shutdown(&self) {
        self.cancel.cancel();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let supervisor = self.llamacpp.clone();
            handle.spawn(async move {
                supervisor.stop_all().await;
            });
        }
        global_dynamic_backends().clear(DynamicBackendSource::AiControlPlane);
    }

    fn spawn(self: &Arc<Self>) {
        let controller = self.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move { controller.run_loop().await });
        } else {
            std::thread::spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("build backend assignment runtime");
                let (tx, rx) = oneshot::channel::<()>();
                let _ = tx;
                runtime.block_on(async move {
                    let _ = rx;
                    controller.run_loop().await;
                });
            });
        }
    }

    async fn run_loop(self: Arc<Self>) {
        let node_uuid = self.config.compute_node_uuid();
        let mut ticker = interval(self.poll_interval);
        let mut backoff_ms: u64 = 200;

        loop {
            if self.cancel.is_cancelled() {
                return;
            }

            let mut client = AiBackendControlPlaneServiceClient::new(self.sms_channel.clone());
            match self.sync_once(&mut client, &node_uuid).await {
                Ok(()) => {
                    backoff_ms = 200;
                    ticker.tick().await;
                }
                Err(error) => {
                    tracing::warn!(error = %error, node_uuid = %node_uuid, "AI backend assignment sync failed");
                    tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                    backoff_ms = (backoff_ms * 2).min(10_000);
                }
            }
        }
    }

    async fn sync_once(
        &self,
        client: &mut AiBackendControlPlaneServiceClient<Channel>,
        node_uuid: &str,
    ) -> Result<(), tonic::Status> {
        let response = client
            .list_ai_backend_assignments(ListAiBackendAssignmentsRequest {
                node_uuid: node_uuid.to_string(),
            })
            .await?
            .into_inner();

        let mut runtime_backends = Vec::new();
        let mut statuses = Vec::new();
        let mut live_local_ids = std::collections::HashSet::new();
        for assignment in response.assignments.iter() {
            let status = self
                .materialize_assignment(
                    client,
                    assignment,
                    &mut runtime_backends,
                    &mut live_local_ids,
                    node_uuid,
                )
                .await;
            if let Some(status) = status {
                statuses.push(status);
            }
        }
        self.llamacpp.stop_removed(&live_local_ids).await;

        global_dynamic_backends()
            .set_backends(DynamicBackendSource::AiControlPlane, runtime_backends);
        self.backend_report_trigger.trigger();

        client
            .report_ai_backend_node_statuses(ReportAiBackendNodeStatusesRequest {
                node_uuid: node_uuid.to_string(),
                statuses,
            })
            .await?;

        Ok(())
    }

    /// Materialize one assignment into runtime state and node status.
    /// 把单个 assignment 落地为运行时状态与节点状态。
    async fn materialize_assignment(
        &self,
        client: &mut AiBackendControlPlaneServiceClient<Channel>,
        assignment: &ResolvedAiBackendAssignment,
        runtime_backends: &mut Vec<BackendInfo>,
        live_local_ids: &mut std::collections::HashSet<String>,
        node_uuid: &str,
    ) -> Option<AiBackendNodeStatusRecord> {
        let backend = assignment.backend.as_ref()?;
        let placement = assignment.placement.as_ref()?;
        let spec = backend.spec.as_ref()?;
        let now_ms = Utc::now().timestamp_millis();

        let hosting =
            AiBackendHosting::try_from(backend.hosting).unwrap_or(AiBackendHosting::Unspecified);

        if hosting == AiBackendHosting::Remote {
            let mut runtime_spec = spec.clone();
            apply_runtime_overrides(
                &mut runtime_spec,
                placement.weight_override,
                placement.priority_override,
            );
            let resolver = crate::spearlet::ai::credential_resolver::global()
                .unwrap_or_else(|| CredentialResolver::from_config(self.config.as_ref()));
            let requested_checks = default_requested_checks(&runtime_spec);
            let report =
                preflight_remote_backend(&self.http, &resolver, &runtime_spec, &requested_checks)
                    .await;
            if report.success {
                runtime_backends.push(available_backend_info(runtime_spec.clone()));
                return Some(ready_status_for_runtime_spec(
                    backend,
                    &runtime_spec,
                    node_uuid,
                    now_ms,
                ));
            }
            let report_message = report.message;
            let failed_status = if report
                .error_code
                .as_deref()
                .is_some_and(|code| code == "credential_not_synced")
            {
                pending_status_for_backend(
                    backend,
                    &runtime_spec,
                    node_uuid,
                    now_ms,
                    report_message.clone(),
                )
            } else {
                error_status_for_backend(backend, &runtime_spec, node_uuid, now_ms, report_message)
            };
            return Some(failed_status);
        }

        let deployment_id = backend.backend_id.clone();
        live_local_ids.insert(deployment_id.clone());

        self.reconcile_local_assignment(
            LocalAssignmentRuntimeFamily::from_assignment(backend, spec),
            client,
            backend,
            placement,
            spec,
            runtime_backends,
            node_uuid,
            now_ms,
        )
        .await
    }

    /// Reconcile one local assignment through the typed runtime family selector.
    /// 通过强类型运行时家族选择器收敛单个 local assignment。
    async fn reconcile_local_assignment(
        &self,
        family: LocalAssignmentRuntimeFamily,
        client: &mut AiBackendControlPlaneServiceClient<Channel>,
        backend: &crate::proto::sms::AiBackendRecord,
        placement: &crate::proto::sms::AiBackendPlacementRecord,
        spec: &crate::proto::sms::BackendSpec,
        runtime_backends: &mut Vec<BackendInfo>,
        node_uuid: &str,
        now_ms: i64,
    ) -> Option<AiBackendNodeStatusRecord> {
        match family {
            LocalAssignmentRuntimeFamily::LlamaCpp => {
                self.reconcile_llamacpp_assignment(
                    client,
                    backend,
                    placement,
                    spec,
                    runtime_backends,
                    node_uuid,
                    now_ms,
                )
                .await
            }
            LocalAssignmentRuntimeFamily::Vllm => self.reconcile_vllm_assignment(
                backend,
                placement,
                spec,
                runtime_backends,
                node_uuid,
                now_ms,
            ),
            LocalAssignmentRuntimeFamily::Unsupported(kind) => Some(pending_status_for_backend(
                backend,
                spec,
                node_uuid,
                now_ms,
                format!(
                    "local ai backend kind/provider {} is not implemented in backend_assignment_controller yet",
                    kind
                ),
            )),
        }
    }

    /// Reconcile one llama.cpp local assignment.
    /// 收敛单个 llama.cpp local assignment。
    async fn reconcile_llamacpp_assignment(
        &self,
        client: &mut AiBackendControlPlaneServiceClient<Channel>,
        backend: &crate::proto::sms::AiBackendRecord,
        placement: &crate::proto::sms::AiBackendPlacementRecord,
        spec: &crate::proto::sms::BackendSpec,
        runtime_backends: &mut Vec<BackendInfo>,
        node_uuid: &str,
        now_ms: i64,
    ) -> Option<AiBackendNodeStatusRecord> {
        let model = spec.model.trim();
        if model.is_empty() {
            return Some(error_status_for_backend(
                backend,
                spec,
                node_uuid,
                now_ms,
                "local ai backend requires spec.model".to_string(),
            ));
        }

        let deployment_id = backend.backend_id.clone();
        let params = LlamaCppAssignmentParams::from_metadata(backend.metadata.as_ref());
        let should_pull = params.should_pull();
        let spec_key = local_assignment_spec_key(backend, placement, params.raw());
        let initial_reason = if should_pull { "pulling" } else { "starting" }.to_string();
        self.report_single_status(
            client,
            node_uuid,
            reconciling_status_for_backend(backend, spec, node_uuid, now_ms, initial_reason),
        )
        .await;

        let start = tokio::time::Instant::now();
        let mut ticker = tokio::time::interval(Duration::from_secs(10));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut fut = std::pin::pin!(self.llamacpp.ensure_server(
            &self.http,
            &deployment_id,
            &spec_key,
            model,
            params.raw()
        ));
        loop {
            tokio::select! {
                res = &mut fut => match res {
            Ok(mut runtime_backend) => {
                if let Some(runtime_spec) = runtime_backend.spec.as_mut() {
                    apply_runtime_overrides(
                        runtime_spec,
                        placement.weight_override,
                        placement.priority_override,
                    );
                    let runtime_spec = runtime_spec.clone();
                    runtime_backends.push(runtime_backend);
                    break Some(ready_status_for_runtime_spec(
                        backend,
                        &runtime_spec,
                        node_uuid,
                        now_ms,
                    ));
                } else {
                    break Some(error_status_for_backend(
                        backend,
                        spec,
                        node_uuid,
                        now_ms,
                        "llamacpp supervisor returned backend without spec".to_string(),
                    ));
                }
            }
            Err(error) => break Some(error_status_for_backend(
                backend,
                spec,
                node_uuid,
                now_ms,
                error,
            )),
                },
                _ = ticker.tick() => {
                    let elapsed_s = start.elapsed().as_secs();
                    let reason = if should_pull {
                        format!("pulling ({elapsed_s}s)")
                    } else {
                        format!("starting ({elapsed_s}s)")
                    };
                    self.report_single_status(
                        client,
                        node_uuid,
                        reconciling_status_for_backend(backend, spec, node_uuid, now_ms, reason),
                    )
                    .await;
                }
            }
        }
    }

    /// Reconcile one vLLM local assignment.
    /// 收敛单个 vLLM local assignment。
    fn reconcile_vllm_assignment(
        &self,
        backend: &crate::proto::sms::AiBackendRecord,
        placement: &crate::proto::sms::AiBackendPlacementRecord,
        spec: &crate::proto::sms::BackendSpec,
        runtime_backends: &mut Vec<BackendInfo>,
        node_uuid: &str,
        now_ms: i64,
    ) -> Option<AiBackendNodeStatusRecord> {
        let params = VllmAssignmentParams::from_spec_and_metadata(spec, backend.metadata.as_ref());
        match self.vllm.prepare_assignment(spec, params.raw()) {
            VllmPreparedAssignment::ExternalEndpoint(endpoint) => {
                let mut runtime_spec = spec.clone();
                runtime_spec.base_url = endpoint.base_url;
                apply_runtime_overrides(
                    &mut runtime_spec,
                    placement.weight_override,
                    placement.priority_override,
                );
                runtime_backends.push(available_backend_info(runtime_spec.clone()));
                Some(ready_status_for_runtime_spec(
                    backend,
                    &runtime_spec,
                    node_uuid,
                    now_ms,
                ))
            }
            VllmPreparedAssignment::Placeholder { reason } => {
                Some(pending_status_for_backend(backend, spec, node_uuid, now_ms, reason))
            }
        }
    }

    /// Report a single node status without interrupting reconcile on transient failures.
    /// 上报单条节点状态；若出现瞬时失败，不中断当前收敛流程。
    async fn report_single_status(
        &self,
        client: &mut AiBackendControlPlaneServiceClient<Channel>,
        node_uuid: &str,
        status: AiBackendNodeStatusRecord,
    ) {
        if let Err(error) = client
            .report_ai_backend_node_statuses(ReportAiBackendNodeStatusesRequest {
                node_uuid: node_uuid.to_string(),
                statuses: vec![status],
            })
            .await
        {
            tracing::warn!(error = %error, node_uuid = %node_uuid, "AI backend status progress report failed");
        }
    }
}

impl Controller for BackendAssignmentController {
    fn name(&self) -> &'static str {
        "backend_assignment_controller"
    }

    fn start(&self) {
        BackendAssignmentController::start(self)
    }

    fn shutdown(&self) {
        BackendAssignmentController::shutdown(self)
    }
}

/// Compute whether llama.cpp reconcile is expected to perform a pull step.
/// 计算 llama.cpp 收敛是否预期会执行拉取阶段。
fn compute_llamacpp_should_pull(params: &std::collections::HashMap<String, String>) -> bool {
    !params
        .get("server_mode")
        .map(|value| value.trim().eq_ignore_ascii_case("raw"))
        .unwrap_or(false)
        && !params
            .get("skip_download")
            .map(|value| parse_metadata_truthy(value))
            .unwrap_or(false)
}

/// Parse a truthy metadata value used by local assignment helpers.
/// 解析 local assignment helper 使用的 metadata 真值字符串。
fn parse_metadata_truthy(value: &str) -> bool {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => true,
        _ => false,
    }
}

/// Build a ready status from a runtime spec.
/// 基于运行时 spec 构建 ready 状态。
fn ready_status_for_runtime_spec(
    backend: &crate::proto::sms::AiBackendRecord,
    runtime_spec: &crate::proto::sms::BackendSpec,
    node_uuid: &str,
    now_ms: i64,
) -> AiBackendNodeStatusRecord {
    AiBackendNodeStatusRecord {
        backend_id: backend.backend_id.clone(),
        node_uuid: node_uuid.to_string(),
        observed_generation: backend.generation,
        status: AiBackendNodeStatus::Ready as i32,
        status_reason: String::new(),
        runtime_backend_name: runtime_spec.name.clone(),
        endpoint: runtime_spec.base_url.clone(),
        available: true,
        operations: runtime_spec.operations.clone(),
        features: runtime_spec.features.clone(),
        transports: runtime_spec.transports.clone(),
        last_heartbeat_at_ms: now_ms,
    }
}

/// Build a pending status from canonical backend information.
/// 基于规范 backend 信息构建 pending 状态。
fn pending_status_for_backend(
    backend: &crate::proto::sms::AiBackendRecord,
    spec: &crate::proto::sms::BackendSpec,
    node_uuid: &str,
    now_ms: i64,
    message: String,
) -> AiBackendNodeStatusRecord {
    AiBackendNodeStatusRecord {
        backend_id: backend.backend_id.clone(),
        node_uuid: node_uuid.to_string(),
        observed_generation: backend.generation,
        status: AiBackendNodeStatus::Pending as i32,
        status_reason: message,
        runtime_backend_name: String::new(),
        endpoint: String::new(),
        available: false,
        operations: spec.operations.clone(),
        features: spec.features.clone(),
        transports: spec.transports.clone(),
        last_heartbeat_at_ms: now_ms,
    }
}

/// Build a reconciling status from canonical backend information.
/// 基于规范 backend 信息构建 reconciling 状态。
fn reconciling_status_for_backend(
    backend: &crate::proto::sms::AiBackendRecord,
    spec: &crate::proto::sms::BackendSpec,
    node_uuid: &str,
    now_ms: i64,
    message: String,
) -> AiBackendNodeStatusRecord {
    AiBackendNodeStatusRecord {
        backend_id: backend.backend_id.clone(),
        node_uuid: node_uuid.to_string(),
        observed_generation: backend.generation,
        status: AiBackendNodeStatus::Reconciling as i32,
        status_reason: message,
        runtime_backend_name: String::new(),
        endpoint: String::new(),
        available: false,
        operations: spec.operations.clone(),
        features: spec.features.clone(),
        transports: spec.transports.clone(),
        last_heartbeat_at_ms: now_ms,
    }
}

/// Build an error status from canonical backend information.
/// 基于规范 backend 信息构建 error 状态。
fn error_status_for_backend(
    backend: &crate::proto::sms::AiBackendRecord,
    spec: &crate::proto::sms::BackendSpec,
    node_uuid: &str,
    now_ms: i64,
    message: String,
) -> AiBackendNodeStatusRecord {
    AiBackendNodeStatusRecord {
        backend_id: backend.backend_id.clone(),
        node_uuid: node_uuid.to_string(),
        observed_generation: backend.generation,
        status: AiBackendNodeStatus::Error as i32,
        status_reason: message,
        runtime_backend_name: String::new(),
        endpoint: String::new(),
        available: false,
        operations: spec.operations.clone(),
        features: spec.features.clone(),
        transports: spec.transports.clone(),
        last_heartbeat_at_ms: now_ms,
    }
}

/// Convert metadata Struct into string params for local drivers.
/// 把 metadata Struct 转成 local driver 使用的字符串参数表。
fn metadata_to_string_map(
    value: Option<&prost_types::Struct>,
) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    let Some(value) = value else {
        return out;
    };
    for (key, value) in value.fields.iter() {
        if let Some(stringified) = stringify_prost_value(value) {
            out.insert(key.clone(), stringified);
        }
    }
    out
}

/// Convert a protobuf Value into a compact string when possible.
/// 在可能的情况下把 protobuf Value 转成紧凑字符串。
fn stringify_prost_value(value: &prost_types::Value) -> Option<String> {
    match value.kind.as_ref()? {
        ProstValueKind::NullValue(_) => None,
        ProstValueKind::BoolValue(inner) => Some(inner.to_string()),
        ProstValueKind::NumberValue(inner) => Some(inner.to_string()),
        ProstValueKind::StringValue(inner) => Some(inner.clone()),
        ProstValueKind::StructValue(_) | ProstValueKind::ListValue(_) => None,
    }
}

/// Build a stable local spec fingerprint from control-plane state.
/// 基于控制面状态构建稳定的 local spec 指纹。
fn local_assignment_spec_key(
    backend: &crate::proto::sms::AiBackendRecord,
    placement: &crate::proto::sms::AiBackendPlacementRecord,
    params: &std::collections::HashMap<String, String>,
) -> String {
    let mut sorted_params = params.iter().collect::<Vec<_>>();
    sorted_params.sort_by(|left, right| left.0.cmp(right.0));
    let rendered = sorted_params
        .into_iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&");
    format!(
        "backend_id={};generation={};placement={};weight={:?};priority={:?};params={}",
        backend.backend_id,
        backend.generation,
        placement.placement_id,
        placement.weight_override,
        placement.priority_override,
        rendered
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::sms::{
        AiBackendDesiredState, AiBackendManagementMode, AiBackendPlacementRecord, AiBackendRecord,
        BackendOrigin, BackendSpec,
    };
    use axum::{routing::get, Json, Router};
    use serde_json::{json, Value};
    use tokio::net::TcpListener;
    use tonic::transport::Endpoint;

    fn sample_assignment(
        hosting: AiBackendHosting,
        backend_kind: &str,
    ) -> ResolvedAiBackendAssignment {
        ResolvedAiBackendAssignment {
            backend: Some(AiBackendRecord {
                backend_id: "backend-1".to_string(),
                display_name: "Backend 1".to_string(),
                provider: "openai".to_string(),
                model: "gpt-4.1".to_string(),
                hosting: hosting as i32,
                backend_kind: backend_kind.to_string(),
                desired_state: AiBackendDesiredState::Enabled as i32,
                management_mode: AiBackendManagementMode::SmsRemote as i32,
                credential_ref: Some("cred-1".to_string()),
                spec: Some(BackendSpec {
                    name: "backend-backend-1".to_string(),
                    kind: "openai-compatible".to_string(),
                    operations: vec!["chat".to_string()],
                    features: vec!["stream".to_string()],
                    transports: vec!["http".to_string()],
                    weight: 100,
                    priority: 0,
                    base_url: "https://example.com".to_string(),
                    provider: "openai".to_string(),
                    model: "gpt-4.1".to_string(),
                    hosting: crate::proto::sms::BackendHosting::Remote as i32,
                    credential_ref: "cred-1".to_string(),
                    origin: BackendOrigin::Sms as i32,
                    deployment_id: String::new(),
                }),
                labels: Default::default(),
                metadata: None,
                generation: 2,
                created_at_ms: 1,
                updated_at_ms: 2,
            }),
            placement: Some(AiBackendPlacementRecord {
                placement_id: "placement-1".to_string(),
                backend_id: "backend-1".to_string(),
                node_uuid: "node-1".to_string(),
                desired_state: AiBackendDesiredState::Enabled as i32,
                weight_override: Some(50),
                priority_override: Some(7),
                generation: 1,
                created_at_ms: 1,
                updated_at_ms: 2,
            }),
        }
    }

    fn sample_assignment_with_metadata(
        hosting: AiBackendHosting,
        backend_kind: &str,
        metadata: prost_types::Struct,
    ) -> ResolvedAiBackendAssignment {
        let mut assignment = sample_assignment(hosting, backend_kind);
        assignment.backend.as_mut().unwrap().metadata = Some(metadata);
        assignment
    }

    fn clear_assignment_base_url(
        mut assignment: ResolvedAiBackendAssignment,
    ) -> ResolvedAiBackendAssignment {
        if let Some(backend) = assignment.backend.as_mut() {
            if let Some(spec) = backend.spec.as_mut() {
                spec.base_url.clear();
            }
        }
        assignment
    }

    fn test_controller() -> Arc<BackendAssignmentController> {
        BackendAssignmentController::new(
            Arc::new(crate::spearlet::config::SpearletConfig::default()),
            Endpoint::from_static("http://127.0.0.1:1").connect_lazy(),
            Duration::from_secs(1),
            BackendReportTrigger::default(),
        )
    }

    fn test_client() -> AiBackendControlPlaneServiceClient<Channel> {
        AiBackendControlPlaneServiceClient::new(
            Endpoint::from_static("http://127.0.0.1:1").connect_lazy(),
        )
    }

    async fn start_json_server(
        path: &'static str,
        payload: Value,
    ) -> (tokio::task::JoinHandle<()>, String) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let app = Router::new().route(
            path,
            get(move || {
                let payload = payload.clone();
                async move { Json(payload) }
            }),
        );
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve");
        });
        (handle, format!("http://{}", addr))
    }

    #[tokio::test]
    async fn materializes_remote_assignment_into_runtime_backend() {
        let (handle, base_url) = start_json_server(
            "/api/tags",
            json!({
                "models": [{"name": "gpt-4.1"}]
            }),
        )
        .await;
        let controller = test_controller();
        let mut client = test_client();
        let mut runtime_backends = Vec::new();
        let mut live_local_ids = std::collections::HashSet::new();
        let mut assignment = sample_assignment(AiBackendHosting::Remote, "ollama_chat");
        let backend = assignment.backend.as_mut().expect("backend");
        backend.provider = "ollama".to_string();
        backend.credential_ref = None;
        let spec = backend.spec.as_mut().expect("spec");
        spec.kind = "ollama_chat".to_string();
        spec.provider = "ollama".to_string();
        spec.base_url = base_url;
        spec.credential_ref.clear();
        let status = controller
            .materialize_assignment(
                &mut client,
                &assignment,
                &mut runtime_backends,
                &mut live_local_ids,
                "node-1",
            )
            .await
            .expect("status");

        assert_eq!(runtime_backends.len(), 1);
        assert_eq!(runtime_backends[0].spec.as_ref().unwrap().weight, 50);
        assert_eq!(runtime_backends[0].spec.as_ref().unwrap().priority, 7);
        assert_eq!(status.status, AiBackendNodeStatus::Ready as i32);
        assert!(status.available);
        handle.abort();
    }

    #[tokio::test]
    async fn keeps_unsupported_local_assignment_out_of_runtime_registry_for_now() {
        let controller = test_controller();
        let mut client = test_client();
        let mut runtime_backends = Vec::new();
        let mut live_local_ids = std::collections::HashSet::new();
        let status = controller
            .materialize_assignment(
                &mut client,
                &clear_assignment_base_url(sample_assignment(AiBackendHosting::Local, "vllm")),
                &mut runtime_backends,
                &mut live_local_ids,
                "node-1",
            )
            .await
            .expect("status");

        assert!(runtime_backends.is_empty());
        assert_eq!(status.status, AiBackendNodeStatus::Pending as i32);
        assert!(!status.available);
    }

    #[tokio::test]
    async fn returns_vllm_placeholder_status_for_vllm_assignments() {
        let controller = test_controller();
        let mut client = test_client();
        let mut runtime_backends = Vec::new();
        let mut live_local_ids = std::collections::HashSet::new();
        let status = controller
            .materialize_assignment(
                &mut client,
                &clear_assignment_base_url(sample_assignment(AiBackendHosting::Local, "vllm")),
                &mut runtime_backends,
                &mut live_local_ids,
                "node-1",
            )
            .await
            .expect("status");

        assert!(runtime_backends.is_empty());
        assert_eq!(status.status, AiBackendNodeStatus::Pending as i32);
        assert!(status.status_reason.contains("vLLM"));
    }

    #[tokio::test]
    async fn materializes_vllm_external_endpoint_assignments_into_runtime_backend() {
        let controller = test_controller();
        let mut client = test_client();
        let mut runtime_backends = Vec::new();
        let mut live_local_ids = std::collections::HashSet::new();
        let metadata = prost_types::Struct {
            fields: [(
                "mode".to_string(),
                prost_types::Value {
                    kind: Some(ProstValueKind::StringValue("external_endpoint".to_string())),
                },
            )]
            .into_iter()
            .collect(),
        };
        let mut assignment =
            sample_assignment_with_metadata(AiBackendHosting::Local, "vllm", metadata);
        assignment
            .backend
            .as_mut()
            .and_then(|backend| backend.spec.as_mut())
            .expect("backend spec")
            .base_url = "http://127.0.0.1:8000/v1".to_string();
        let status = controller
            .materialize_assignment(
                &mut client,
                &assignment,
                &mut runtime_backends,
                &mut live_local_ids,
                "node-1",
            )
            .await
            .expect("status");

        assert_eq!(runtime_backends.len(), 1);
        assert_eq!(
            runtime_backends[0]
                .spec
                .as_ref()
                .expect("runtime spec")
                .base_url,
            "http://127.0.0.1:8000/v1"
        );
        assert_eq!(status.status, AiBackendNodeStatus::Ready as i32);
        assert!(status.available);
    }

    #[test]
    fn metadata_to_string_map_keeps_flat_scalars() {
        let mut fields = std::collections::BTreeMap::new();
        fields.insert(
            "threads".to_string(),
            prost_types::Value {
                kind: Some(ProstValueKind::StringValue("8".to_string())),
            },
        );
        fields.insert(
            "skip_download".to_string(),
            prost_types::Value {
                kind: Some(ProstValueKind::BoolValue(true)),
            },
        );

        let metadata = prost_types::Struct {
            fields: fields.into_iter().collect(),
        };
        let params = metadata_to_string_map(Some(&metadata));

        assert_eq!(params.get("threads").map(String::as_str), Some("8"));
        assert_eq!(
            params.get("skip_download").map(String::as_str),
            Some("true")
        );
    }

    #[test]
    fn llamacpp_assignment_params_capture_should_pull() {
        let mut fields = std::collections::BTreeMap::new();
        fields.insert(
            "skip_download".to_string(),
            prost_types::Value {
                kind: Some(ProstValueKind::BoolValue(true)),
            },
        );
        let metadata = prost_types::Struct {
            fields: fields.into_iter().collect(),
        };

        let params = LlamaCppAssignmentParams::from_metadata(Some(&metadata));
        assert_eq!(
            params.raw().get("skip_download").map(String::as_str),
            Some("true")
        );
        assert!(!params.should_pull());
    }

    #[test]
    fn vllm_assignment_params_preserve_metadata_scalars() {
        let spec = BackendSpec {
            name: String::new(),
            kind: "vllm-openai".to_string(),
            operations: vec!["chat".to_string()],
            features: vec![],
            transports: vec!["http".to_string()],
            weight: 1,
            priority: 0,
            base_url: String::new(),
            provider: "vllm".to_string(),
            model: "foo".to_string(),
            hosting: crate::proto::sms::BackendHosting::NodeLocal as i32,
            credential_ref: String::new(),
            origin: BackendOrigin::Sms as i32,
            deployment_id: String::new(),
        };
        let mut fields = std::collections::BTreeMap::new();
        fields.insert(
            "mode".to_string(),
            prost_types::Value {
                kind: Some(ProstValueKind::StringValue("external_endpoint".to_string())),
            },
        );
        let metadata = prost_types::Struct {
            fields: fields.into_iter().collect(),
        };

        let params = VllmAssignmentParams::from_spec_and_metadata(&spec, Some(&metadata));
        assert_eq!(
            params.raw().get("mode").map(String::as_str),
            Some("external_endpoint")
        );
    }

    #[test]
    fn resolves_local_assignment_runtime_family_from_backend_kind_and_spec_fields() {
        let backend = AiBackendRecord {
            backend_id: "backend-1".to_string(),
            display_name: "Backend 1".to_string(),
            provider: "vllm".to_string(),
            model: "foo".to_string(),
            hosting: AiBackendHosting::Local as i32,
            backend_kind: "custom".to_string(),
            desired_state: AiBackendDesiredState::Enabled as i32,
            management_mode: AiBackendManagementMode::SmsLocal as i32,
            credential_ref: None,
            spec: None,
            labels: Default::default(),
            metadata: None,
            generation: 1,
            created_at_ms: 1,
            updated_at_ms: 1,
        };
        let spec = BackendSpec {
            name: String::new(),
            kind: "vllm-openai".to_string(),
            operations: vec!["chat".to_string()],
            features: vec![],
            transports: vec!["http".to_string()],
            weight: 1,
            priority: 0,
            base_url: String::new(),
            provider: "vllm".to_string(),
            model: "foo".to_string(),
            hosting: crate::proto::sms::BackendHosting::NodeLocal as i32,
            credential_ref: String::new(),
            origin: BackendOrigin::Sms as i32,
            deployment_id: String::new(),
        };

        let mut llamacpp_backend = backend.clone();
        llamacpp_backend.backend_kind = "llama.cpp".to_string();
        assert_eq!(
            LocalAssignmentRuntimeFamily::from_assignment(&llamacpp_backend, &spec),
            LocalAssignmentRuntimeFamily::LlamaCpp
        );
        assert_eq!(
            LocalAssignmentRuntimeFamily::from_assignment(&backend, &spec),
            LocalAssignmentRuntimeFamily::Vllm
        );
    }

    #[test]
    fn compute_llamacpp_should_pull_respects_server_mode_and_skip_download() {
        let mut params = std::collections::HashMap::new();
        assert!(compute_llamacpp_should_pull(&params));

        params.insert("server_mode".to_string(), "raw".to_string());
        assert!(!compute_llamacpp_should_pull(&params));

        params.remove("server_mode");
        params.insert("skip_download".to_string(), "1".to_string());
        assert!(!compute_llamacpp_should_pull(&params));
    }

    #[test]
    fn reconciling_status_uses_reconciling_enum_and_reason() {
        let assignment = sample_assignment(AiBackendHosting::Local, "llamacpp");
        let backend = assignment.backend.as_ref().unwrap();
        let spec = backend.spec.as_ref().unwrap();

        let status =
            reconciling_status_for_backend(backend, spec, "node-1", 123, "starting".to_string());

        assert_eq!(status.status, AiBackendNodeStatus::Reconciling as i32);
        assert_eq!(status.status_reason, "starting");
        assert!(!status.available);
    }
}
