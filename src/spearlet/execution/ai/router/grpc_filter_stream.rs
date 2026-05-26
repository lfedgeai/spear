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

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use tokio::sync::{mpsc, Semaphore};
use tonic::transport::{Channel, Endpoint};

use crate::proto::spearlet::{
    router_filter_service_client::RouterFilterServiceClient, Candidate, CandidateRuntimeHints,
    DecisionAction, FilterRequest, FilterResponse, Operation as ProtoOperation, RequestSignals,
    Requirements, RoutingHints,
};
use crate::spearlet::config::RouterGrpcFilterStreamConfig;
use crate::spearlet::execution::ai::ir::{CanonicalError, CanonicalRequestEnvelope, Operation};
use crate::spearlet::execution::ai::router::registry::{BackendInstance, Hosting};

static GLOBAL_HUB: OnceLock<Arc<RouterFilterStreamHub>> = OnceLock::new();

fn to_proto_operation(op: &Operation) -> i32 {
    match op {
        Operation::ChatCompletions => ProtoOperation::ChatCompletions as i32,
        Operation::Embeddings => ProtoOperation::Embeddings as i32,
        Operation::ImageGeneration => ProtoOperation::ImageGeneration as i32,
        Operation::SpeechToText => ProtoOperation::SpeechToText as i32,
        Operation::TextToSpeech => ProtoOperation::TextToSpeech as i32,
        Operation::RealtimeVoice => ProtoOperation::RealtimeVoice as i32,
    }
}

fn to_operation_name(op: &Operation) -> &'static str {
    match op {
        Operation::ChatCompletions => "chat_completions",
        Operation::Embeddings => "embeddings",
        Operation::ImageGeneration => "image_generation",
        Operation::SpeechToText => "speech_to_text",
        Operation::TextToSpeech => "text_to_speech",
        Operation::RealtimeVoice => "realtime_voice",
    }
}

fn requested_model(req: &CanonicalRequestEnvelope) -> Option<&str> {
    match &req.payload {
        crate::spearlet::execution::ai::ir::Payload::ChatCompletions(p) => Some(p.model.as_str()),
        crate::spearlet::execution::ai::ir::Payload::Embeddings(p) => p.model.as_deref(),
        crate::spearlet::execution::ai::ir::Payload::ImageGeneration(p) => p.model.as_deref(),
        crate::spearlet::execution::ai::ir::Payload::SpeechToText(p) => p.model.as_deref(),
        crate::spearlet::execution::ai::ir::Payload::TextToSpeech(p) => p.model.as_deref(),
        crate::spearlet::execution::ai::ir::Payload::RealtimeVoice(p) => p.model.as_deref(),
    }
}

fn build_signals(req: &CanonicalRequestEnvelope) -> RequestSignals {
    match &req.payload {
        crate::spearlet::execution::ai::ir::Payload::ChatCompletions(p) => {
            let mut approx: u32 = 0;
            for m in p.messages.iter() {
                if let Some(s) = m.content.as_str() {
                    approx = approx.saturating_add(s.len().min(u32::MAX as usize) as u32);
                }
            }
            let uses_tools = !p.tools.is_empty();
            let uses_json_schema = p
                .params
                .get("response_format")
                .and_then(|v| v.get("json_schema"))
                .is_some();
            RequestSignals {
                model: p.model.clone(),
                message_count: p.messages.len().min(u32::MAX as usize) as u32,
                approx_text_bytes: approx,
                uses_tools,
                uses_json_schema,
                ..Default::default()
            }
        }
        crate::spearlet::execution::ai::ir::Payload::SpeechToText(p) => RequestSignals {
            model: p.model.clone().unwrap_or_default(),
            ..Default::default()
        },
        _ => RequestSignals {
            model: requested_model(req).unwrap_or_default().to_string(),
            ..Default::default()
        },
    }
}

#[derive(Debug, Clone, Default)]
pub struct FilterTrace {
    pub decision_id: Option<String>,
    pub dropped: Vec<String>,
    pub weight_overrides: Vec<(String, u32)>,
    pub priority_overrides: Vec<(String, i32)>,
    pub reason_codes_by_candidate: HashMap<String, Vec<String>>,
    pub final_action: Option<FinalActionTrace>,
}

#[derive(Debug, Clone, Default)]
pub struct FinalActionTrace {
    pub reject_request: bool,
    pub reject_code: Option<String>,
    pub force_backend: Option<String>,
}

fn trace_from_response(resp: &FilterResponse, max_debug_kv: usize) -> FilterTrace {
    let mut trace = FilterTrace::default();
    if !resp.decision_id.trim().is_empty() {
        trace.decision_id = Some(resp.decision_id.clone());
    }
    let mut reason_map: HashMap<String, Vec<String>> = HashMap::new();
    for d in resp.decisions.iter() {
        if d.action == DecisionAction::Drop as i32 {
            trace.dropped.push(d.name.clone());
        }
        if let Some(w) = d.weight_override {
            trace.weight_overrides.push((d.name.clone(), w));
        }
        if let Some(p) = d.priority_override {
            trace.priority_overrides.push((d.name.clone(), p));
        }
        if !d.reason_codes.is_empty() {
            reason_map.insert(d.name.clone(), d.reason_codes.clone());
        }
    }
    trace.reason_codes_by_candidate = reason_map;
    if let Some(fa) = resp.final_action.as_ref() {
        let mut fat = FinalActionTrace::default();
        fat.reject_request = fa.reject_request;
        if !fa.reject_code.trim().is_empty() {
            fat.reject_code = Some(fa.reject_code.clone());
        }
        if !fa.force_backend.trim().is_empty() {
            fat.force_backend = Some(fa.force_backend.clone());
        }
        trace.final_action = Some(fat);
    }
    let _ = max_debug_kv;
    trace
}

#[derive(Debug)]
struct FilterJob {
    req: FilterRequest,
}

/// RouterFilterStreamHub is a process-local broker for routing-time filter calls.
/// RouterFilterStreamHub 是进程内的路由过滤调用 broker。
pub struct RouterFilterStreamHub {
    pub config: RouterGrpcFilterStreamConfig,
    inflight: parking_lot::Mutex<
        HashMap<String, std::sync::mpsc::Sender<Result<FilterResponse, CanonicalError>>>,
    >,
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
            inflight: parking_lot::Mutex::new(HashMap::new()),
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
        let hub = self.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                hub.filter_worker(rx).await;
            });
        } else {
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("build router filter background runtime");
                rt.block_on(async move {
                    hub.filter_worker(rx).await;
                });
            });
        }
    }

    async fn build_client(
        &self,
        connect_timeout: Duration,
    ) -> Result<RouterFilterServiceClient<Channel>, CanonicalError> {
        let addr = self.config.addr.trim();
        if addr.is_empty() {
            return Err(CanonicalError {
                code: "router_filter_unavailable".to_string(),
                message: "router filter addr is empty".to_string(),
                retryable: true,
                operation: None,
            });
        }
        let ep = Endpoint::from_shared(format!("http://{}", addr))
            .map_err(|e| CanonicalError {
                code: "router_filter_unavailable".to_string(),
                message: format!("invalid router filter addr: {}", e),
                retryable: true,
                operation: None,
            })?
            .tcp_nodelay(true)
            .connect_timeout(connect_timeout);

        let channel = ep.connect().await.map_err(|e| CanonicalError {
            code: "router_filter_unavailable".to_string(),
            message: format!("router filter connect error: {}", e),
            retryable: true,
            operation: None,
        })?;
        Ok(RouterFilterServiceClient::new(channel))
    }

    async fn filter_worker(self: Arc<Self>, mut rx: mpsc::UnboundedReceiver<FilterJob>) {
        let mut client: Option<RouterFilterServiceClient<Channel>> = None;
        while let Some(job) = rx.recv().await {
            if !self.config.enabled {
                self.finish_with_error(
                    job.req.correlation_id.as_str(),
                    CanonicalError {
                        code: "router_filter_unavailable".to_string(),
                        message: "router filter disabled".to_string(),
                        retryable: true,
                        operation: None,
                    },
                );
                continue;
            }

            if client.is_none() {
                let connect_timeout =
                    Duration::from_millis(job.req.decision_timeout_ms.max(1) as u64)
                        .min(Duration::from_millis(200));
                client = self.build_client(connect_timeout).await.ok();
            }
            let Some(mut c) = client.clone() else {
                self.finish_with_error(
                    job.req.correlation_id.as_str(),
                    CanonicalError {
                        code: "router_filter_unavailable".to_string(),
                        message: "router filter client unavailable".to_string(),
                        retryable: true,
                        operation: None,
                    },
                );
                continue;
            };

            let timeout = Duration::from_millis(job.req.decision_timeout_ms.max(1) as u64);
            let cid = job.req.correlation_id.clone();
            let r = tokio::time::timeout(timeout, c.filter(job.req)).await;
            match r {
                Err(_) => {
                    self.finish_with_error(
                        cid.as_str(),
                        CanonicalError {
                            code: "router_filter_timeout".to_string(),
                            message: "router filter decision timed out".to_string(),
                            retryable: true,
                            operation: None,
                        },
                    );
                }
                Ok(Err(e)) => {
                    client = None;
                    self.finish_with_error(
                        cid.as_str(),
                        CanonicalError {
                            code: "router_filter_unavailable".to_string(),
                            message: format!("router filter rpc error: {}", e.message()),
                            retryable: true,
                            operation: None,
                        },
                    );
                }
                Ok(Ok(resp)) => {
                    self.finish_with_response(resp.into_inner());
                }
            }
        }
    }

    fn finish_with_response(&self, resp: FilterResponse) {
        let cid = resp.correlation_id.clone();
        let tx = self.inflight.lock().remove(&cid);
        if let Some(tx) = tx {
            let _ = tx.send(Ok(resp));
        }
    }

    fn finish_with_error(&self, correlation_id: &str, err: CanonicalError) {
        let tx = self.inflight.lock().remove(correlation_id);
        if let Some(tx) = tx {
            let _ = tx.send(Err(err));
        }
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

        let (wait_tx, wait_rx) =
            std::sync::mpsc::channel::<Result<FilterResponse, CanonicalError>>();
        self.inflight.lock().insert(correlation_id.clone(), wait_tx);

        let max_candidates = self.config.max_candidates_sent.max(1);
        let mut proto_candidates: Vec<Candidate> = Vec::new();
        for c in candidates.iter().take(max_candidates) {
            let ops = c
                .capabilities
                .ops
                .iter()
                .map(to_operation_name)
                .map(|s| s.to_string())
                .collect();
            proto_candidates.push(Candidate {
                name: c.name.clone(),
                kind: c.kind.clone(),
                base_url: c.base_url.clone(),
                model: c.model.clone().unwrap_or_default(),
                weight: c.weight,
                priority: c.priority,
                ops,
                features: c.capabilities.features.clone(),
                transports: c.capabilities.transports.clone(),
                is_local: c.hosting == Hosting::Local,
                runtime: Some(CandidateRuntimeHints::default()),
            });
        }

        let routing = RoutingHints {
            backend: req.routing.backend.clone().unwrap_or_default(),
            allowlist: req.routing.allowlist.clone(),
            denylist: req.routing.denylist.clone(),
            requested_model: requested_model(req).unwrap_or_default().to_string(),
        };

        let requirements = Requirements {
            required_features: req.requirements.required_features.clone(),
            required_transports: req.requirements.required_transports.clone(),
        };

        let op = to_proto_operation(&req.operation);

        let (content_type, payload) = if self.config.content_fetch_enabled {
            match serde_json::to_vec(&req.payload) {
                Ok(mut b) => {
                    let max = self.config.content_fetch_max_bytes.max(1);
                    if b.len() > max {
                        b.clear();
                        ("".to_string(), Vec::new())
                    } else {
                        ("application/json".to_string(), b)
                    }
                }
                Err(_) => ("".to_string(), Vec::new()),
            }
        } else {
            ("".to_string(), Vec::new())
        };

        let filter_req = FilterRequest {
            correlation_id: correlation_id.clone(),
            request_id: req.request_id.clone(),
            operation: op,
            decision_timeout_ms: decision_timeout_ms.min(u32::MAX as u64) as u32,
            meta: req.meta.clone(),
            routing: Some(routing),
            requirements: Some(requirements),
            signals: Some(build_signals(req)),
            candidates: proto_candidates,
            request_content_type: content_type,
            request_payload: payload,
        };

        if self.job_tx.send(FilterJob { req: filter_req }).is_err() {
            self.inflight.lock().remove(&correlation_id);
            drop(total_permit);
            return Err(CanonicalError {
                code: "router_filter_unavailable".to_string(),
                message: "router filter worker unavailable".to_string(),
                retryable: true,
                operation: Some(req.operation.clone()),
            });
        }

        let timeout = Duration::from_millis(decision_timeout_ms.max(1));
        let resp = wait_rx.recv_timeout(timeout).map_err(|_| {
            self.inflight.lock().remove(&correlation_id);
            CanonicalError {
                code: "router_filter_timeout".to_string(),
                message: "router filter decision timed out".to_string(),
                retryable: true,
                operation: Some(req.operation.clone()),
            }
        })??;
        self.inflight.lock().remove(&correlation_id);
        drop(total_permit);

        let trace = trace_from_response(&resp, self.config.max_debug_kv);
        Ok((resp, trace))
    }
}
