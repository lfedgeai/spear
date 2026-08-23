//! Router gRPC filter integration (client hub).
//! Router gRPC 过滤集成（客户端 Hub）。
//!
//! Design / 设计：
//! - Router Filter is a server (default in SMS); Spearlet connects to it as a client.
//! - Router::route is sync, so we keep a background async worker to do gRPC calls.
//! - The sync path blocks waiting for response with a strict timeout budget.
//! - fail-open/fail-closed is decided by Router based on config.fail_open.
//! - Router Filter 作为服务端（默认在 SMS）；Spearlet 作为客户端连接。
//! - Router::route 是同步函数，因此使用后台异步 worker 执行 gRPC 调用。
//! - 同步路径在严格预算内阻塞等待响应。
//! - fail-open/fail-closed 由 Router 按 config.fail_open 决定。

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use crate::proto::spearlet::FilterResponse;
use tokio::sync::{mpsc, Semaphore};

use crate::spearlet::config::RouterGrpcFilterStreamConfig;
use crate::spearlet::execution::ai::ir::{CanonicalError, CanonicalRequestEnvelope};
use crate::spearlet::execution::ai::router::filter_inflight::InflightRegistry;
use crate::spearlet::execution::ai::router::filter_protocol::{
    build_filter_request, trace_from_response,
};
use crate::spearlet::execution::ai::router::filter_worker::{start_background_worker, FilterJob};
use crate::spearlet::execution::ai::router::registry::BackendInstance;

pub use crate::spearlet::execution::ai::router::filter_protocol::{
    requested_model, FilterTrace, FinalActionTrace,
};

static GLOBAL_HUB: OnceLock<Arc<RouterFilterStreamHub>> = OnceLock::new();

/// RouterFilterStreamHub is a process-local broker for routing-time filter calls.
/// RouterFilterStreamHub 是进程内的路由过滤调用 broker。
pub struct RouterFilterStreamHub {
    pub config: RouterGrpcFilterStreamConfig,
    inflight: Arc<InflightRegistry>,
    total_inflight: Arc<Semaphore>,
    job_tx: mpsc::UnboundedSender<FilterJob>,
    job_rx: parking_lot::Mutex<Option<mpsc::UnboundedReceiver<FilterJob>>>,
}

impl RouterFilterStreamHub {
    /// Initialize global hub (idempotent).
    /// 初始化全局 hub（幂等）。
    pub fn init_global(config: RouterGrpcFilterStreamConfig) -> Arc<Self> {
        GLOBAL_HUB
            .get_or_init(|| {
                let hub = Arc::new(Self::new(config));
                hub.start_background();
                hub
            })
            .clone()
    }

    /// Get global hub if already initialized.
    /// 获取已初始化的全局 hub。
    pub fn global() -> Option<Arc<Self>> {
        GLOBAL_HUB.get().cloned()
    }

    pub fn new(config: RouterGrpcFilterStreamConfig) -> Self {
        let max_total = config.max_inflight_total.max(1);
        let (job_tx, job_rx) = mpsc::unbounded_channel::<FilterJob>();
        Self {
            config,
            inflight: Arc::new(InflightRegistry::new()),
            total_inflight: Arc::new(Semaphore::new(max_total)),
            job_tx,
            job_rx: parking_lot::Mutex::new(Some(job_rx)),
        }
    }

    pub fn start_background(self: &Arc<Self>) {
        let rx = self.job_rx.lock().take();
        let Some(rx) = rx else {
            return;
        };
        start_background_worker(self.config.clone(), self.inflight.clone(), rx);
    }

    pub fn try_filter_candidates_blocking(
        &self,
        req: &CanonicalRequestEnvelope,
        candidates: &mut Vec<&BackendInstance>,
        decision_timeout_ms: u64,
    ) -> Result<(FilterResponse, FilterTrace), CanonicalError> {
        if !self.config.enabled {
            return Err(CanonicalError {
                code: "router_filter_unavailable".to_string(),
                message: "router filter disabled".to_string(),
                retryable: true,
                operation: Some(req.operation.clone()),
            });
        }

        let total_permit = self
            .total_inflight
            .try_acquire()
            .map_err(|_| CanonicalError {
                code: "router_filter_busy".to_string(),
                message: "router filter inflight limit reached".to_string(),
                retryable: true,
                operation: Some(req.operation.clone()),
            })?;

        let correlation_id = uuid::Uuid::new_v4().to_string();
        let wait_handle = self.inflight.register(correlation_id.clone());

        let filter_req = build_filter_request(
            req,
            candidates.as_slice(),
            &self.config,
            correlation_id.clone(),
            decision_timeout_ms,
        );

        if self.job_tx.send(FilterJob { req: filter_req }).is_err() {
            self.inflight.cancel(&correlation_id);
            drop(total_permit);
            return Err(CanonicalError {
                code: "router_filter_unavailable".to_string(),
                message: "router filter worker unavailable".to_string(),
                retryable: true,
                operation: Some(req.operation.clone()),
            });
        }

        let timeout = Duration::from_millis(decision_timeout_ms.max(1));
        let resp = wait_handle.wait(timeout, &req.operation)?;
        self.inflight.cancel(&correlation_id);
        drop(total_permit);

        let trace = trace_from_response(&resp, self.config.max_debug_kv);
        Ok((resp, trace))
    }
}
