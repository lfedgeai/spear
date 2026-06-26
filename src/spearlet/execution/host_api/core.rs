use crate::spearlet::execution::ai::router::Router;
use crate::spearlet::execution::ai::AiEngine;
use crate::spearlet::execution::ai::engine_holder::{global as global_engine_holder, EngineHolder};
use crate::spearlet::execution::host_api::iface::{HttpCallResult, SpearHostApi};
use crate::spearlet::execution::hostcall::fd_table::FdTable;
use crate::spearlet::execution::ExecutionError;
use crate::spearlet::mcp::registry_sync::{global_mcp_registry_sync, McpRegistrySyncService};
use crate::spearlet::mcp::task_subset::McpTaskPolicy;
use dashmap::DashMap;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};

thread_local! {
    static CURRENT_WASM_EXECUTION_ID: RefCell<Option<String>> = const { RefCell::new(None) };
}

pub fn set_current_wasm_execution_id(execution_id: Option<String>) {
    CURRENT_WASM_EXECUTION_ID.with(|v| {
        *v.borrow_mut() = execution_id;
    });
}

pub(crate) fn current_wasm_execution_id() -> Option<String> {
    CURRENT_WASM_EXECUTION_ID.with(|v| v.borrow().clone())
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WasmLogEntry {
    pub seq: u64,
    pub ts_ms: u64,
    pub level: String,
    pub execution_id: Option<String>,
    pub task_id: Option<String>,
    pub instance_id: Option<String>,
    pub message: String,
}

#[derive(Debug)]
struct WasmLogRing {
    cap: usize,
    inner: Mutex<WasmLogRingInner>,
}

#[derive(Debug)]
struct WasmLogRingInner {
    next_seq: u64,
    buf: VecDeque<WasmLogEntry>,
}

impl WasmLogRing {
    fn new(cap: usize) -> Self {
        Self {
            cap: cap.max(1),
            inner: Mutex::new(WasmLogRingInner {
                next_seq: 1,
                buf: VecDeque::new(),
            }),
        }
    }

    fn push(
        &self,
        ts_ms: u64,
        level: String,
        execution_id: Option<String>,
        task_id: Option<String>,
        instance_id: Option<String>,
        message: String,
    ) -> u64 {
        let msg_preview = if message.chars().count() > 200 {
            let mut s = message.chars().take(200).collect::<String>();
            s.push_str("...");
            s
        } else {
            message.clone()
        };

        let mut st = self.inner.lock();
        let seq = st.next_seq;
        st.next_seq = st.next_seq.wrapping_add(1);
        st.buf.push_back(WasmLogEntry {
            seq,
            ts_ms,
            level: level.clone(),
            execution_id: execution_id.clone(),
            task_id: task_id.clone(),
            instance_id: instance_id.clone(),
            message,
        });
        while st.buf.len() > self.cap {
            st.buf.pop_front();
        }
        debug!(
            seq = seq,
            ts_ms = ts_ms,
            level = %level,
            execution_id = execution_id.as_deref().unwrap_or(""),
            task_id = task_id.as_deref().unwrap_or(""),
            instance_id = instance_id.as_deref().unwrap_or(""),
            msg = %msg_preview,
            buf_len = st.buf.len(),
            cap = self.cap,
            next_seq = st.next_seq,
            "wasm_log_ring.push"
        );
        seq
    }

    fn read(&self, since_seq: Option<u64>, limit: usize) -> Vec<WasmLogEntry> {
        let st = self.inner.lock();
        let mut out = Vec::new();
        let limit = limit.max(1).min(self.cap);
        for e in st.buf.iter() {
            if since_seq.is_some_and(|s| e.seq <= s) {
                continue;
            }
            out.push(e.clone());
            if out.len() >= limit {
                break;
            }
        }
        let preview = out
            .iter()
            .take(10)
            .map(|e| {
                let msg = if e.message.chars().count() > 200 {
                    let mut s = e.message.chars().take(200).collect::<String>();
                    s.push_str("...");
                    s
                } else {
                    e.message.clone()
                };
                format!(
                    "seq={} exec={} level={} msg={}",
                    e.seq,
                    e.execution_id.as_deref().unwrap_or(""),
                    e.level,
                    msg
                )
            })
            .collect::<Vec<_>>();
        debug!(
            since_seq = since_seq.unwrap_or(0),
            limit = limit,
            out_len = out.len(),
            first_seq = out.first().map(|e| e.seq).unwrap_or(0),
            last_seq = out.last().map(|e| e.seq).unwrap_or(0),
            buf_len = st.buf.len(),
            cap = self.cap,
            next_seq = st.next_seq,
            preview = ?preview,
            "wasm_log_ring.read"
        );
        out
    }
}

static WASM_LOG_RINGS: OnceLock<DashMap<String, Arc<WasmLogRing>>> = OnceLock::new();

fn wasm_log_rings() -> &'static DashMap<String, Arc<WasmLogRing>> {
    WASM_LOG_RINGS.get_or_init(DashMap::new)
}

fn wasm_logs_key_for_execution(execution_id: &str) -> String {
    format!("exec:{}", execution_id)
}

pub fn get_wasm_logs_by_execution(
    execution_id: &str,
    since_seq: Option<u64>,
    limit: usize,
) -> Vec<WasmLogEntry> {
    let out = wasm_log_rings()
        .get(&wasm_logs_key_for_execution(execution_id))
        .map(|r| r.read(since_seq, limit))
        .unwrap_or_default();
    debug!(
        execution_id = %execution_id,
        since_seq = since_seq.unwrap_or(0),
        limit = limit,
        count = out.len(),
        logs = ?out,
        "get_wasm_logs_by_execution"
    );
    out
}

pub fn clear_wasm_logs_by_execution(execution_id: &str) {
    wasm_log_rings().remove(&wasm_logs_key_for_execution(execution_id));
}

#[derive(Clone, Debug)]
pub struct DefaultHostApi {
    pub(super) runtime_config: super::super::runtime::RuntimeConfig,
    pub(super) fd_table: Arc<FdTable>,
    pub(super) ai_engine_holder: Arc<EngineHolder>,
    pub(super) mcp_registry_sync: Option<Arc<McpRegistrySyncService>>,
    pub(super) task_id: Option<String>,
    pub(super) mcp_task_policy: Option<Arc<McpTaskPolicy>>,
    pub(super) instance_id: Option<String>,
    pub(super) execution_id: Option<String>,
    pub(super) exec_termination: Arc<super::termination::WasmTerminationRegistry>,
    pub(super) instance_termination: Arc<super::termination::WasmTerminationRegistry>,
}

impl DefaultHostApi {
    pub fn new(runtime_config: super::super::runtime::RuntimeConfig) -> Self {
        let ai_engine_holder = global_engine_holder().unwrap_or_else(|| {
            let (registry, policy) = crate::spearlet::execution::ai::router::builder::build_registry_from_runtime_config(&runtime_config);
            let grpc_filter_stream = runtime_config
                .spearlet_config
                .as_ref()
                .and_then(|cfg| {
                    cfg.ai.router_grpc_filter_stream.clone().map(|mut f| {
                        if f.enabled && f.addr.trim().is_empty() {
                            f.addr = cfg.sms_grpc_addr.clone();
                        }
                        f
                    })
                })
                .map(
                    crate::spearlet::execution::ai::router::grpc_filter_stream::RouterFilterStreamHub::init_global,
                )
                .or_else(crate::spearlet::execution::ai::router::grpc_filter_stream::RouterFilterStreamHub::global)
                .filter(|h| h.config.enabled);
            let router = Router::new_with_filter(registry, policy, grpc_filter_stream);
            let engine = Arc::new(AiEngine::new(router));
            Arc::new(EngineHolder::new(engine, 0))
        });

        let mcp_registry_sync = runtime_config
            .spearlet_config
            .clone()
            .map(|cfg| global_mcp_registry_sync(Arc::new(cfg)));
        Self {
            runtime_config,
            fd_table: Arc::new(FdTable::new(1000)),
            ai_engine_holder,
            mcp_registry_sync,
            task_id: None,
            mcp_task_policy: None,
            instance_id: None,
            execution_id: None,
            exec_termination: super::termination::exec_registry(),
            instance_termination: super::termination::instance_registry(),
        }
    }

    pub fn set_execution_id(&mut self, execution_id: Option<String>) {
        self.execution_id = execution_id;
    }

    pub fn with_task_policy(mut self, task_id: String, policy: Arc<McpTaskPolicy>) -> Self {
        self.task_id = Some(task_id);
        self.mcp_task_policy = Some(policy);
        self
    }

    pub fn with_instance_id(mut self, instance_id: String) -> Self {
        self.instance_id = Some(instance_id);
        self
    }

    pub fn check_wasm_termination(&self) -> Option<super::termination::TerminationSnapshot> {
        let exec_id = self.execution_id.clone().or_else(current_wasm_execution_id);
        if let Some(execution_id) = exec_id.as_deref() {
            if let Some(s) = self.exec_termination.check(execution_id) {
                return Some(s);
            }
        }
        if let Some(instance_id) = self.instance_id.as_deref() {
            if let Some(s) = self.instance_termination.check(instance_id) {
                return Some(s);
            }
        }
        None
    }

    pub fn wasm_log_write(&self, level: &str, message: &str) {
        let Some(instance_id) = self.instance_id.as_ref() else {
            self.log(level, message);
            return;
        };

        let ts_ms = self.time_now_ms();
        let task_id = self.task_id.clone();
        let execution_id_for_entry = self.execution_id.clone().or_else(current_wasm_execution_id);
        let instance_id_for_entry = Some(instance_id.clone());
        let ring_exec = execution_id_for_entry.as_ref().map(|execution_id| {
            wasm_log_rings()
                .entry(wasm_logs_key_for_execution(execution_id))
                .or_insert_with(|| Arc::new(WasmLogRing::new(2048)))
                .clone()
        });
        if let Some(r) = ring_exec {
            let _ = r.push(
                ts_ms,
                level.to_string(),
                execution_id_for_entry.clone(),
                task_id.clone(),
                instance_id_for_entry.clone(),
                message.to_string(),
            );
        }

        match level {
            "trace" => tracing::trace!(
                task_id = task_id,
                execution_id = execution_id_for_entry,
                instance_id = instance_id_for_entry,
                "{message}"
            ),
            "debug" => tracing::debug!(
                task_id = task_id,
                execution_id = execution_id_for_entry,
                instance_id = instance_id_for_entry,
                "{message}"
            ),
            "info" => tracing::info!(
                task_id = task_id,
                execution_id = execution_id_for_entry,
                instance_id = instance_id_for_entry,
                "{message}"
            ),
            "warn" => tracing::warn!(
                task_id = task_id,
                execution_id = execution_id_for_entry,
                instance_id = instance_id_for_entry,
                "{message}"
            ),
            "error" => tracing::error!(
                task_id = task_id,
                execution_id = execution_id_for_entry,
                instance_id = instance_id_for_entry,
                "{message}"
            ),
            _ => tracing::info!(
                task_id = task_id,
                execution_id = execution_id_for_entry,
                instance_id = instance_id_for_entry,
                "{message}"
            ),
        }
    }
}

impl Drop for DefaultHostApi {
    fn drop(&mut self) {
        if let Some(svc) = self.mcp_registry_sync.as_ref() {
            if Arc::strong_count(svc) == 1 {
                svc.shutdown();
            }
        }
        if Arc::strong_count(&self.fd_table) == 1 {
            self.fd_table.close_all();
        }
    }
}

impl SpearHostApi for DefaultHostApi {
    fn log(&self, level: &str, message: &str) {
        match level {
            "trace" => debug!("{message}"),
            "debug" => debug!("{message}"),
            "info" => info!("{message}"),
            "warn" => warn!("{message}"),
            "error" => tracing::error!("{message}"),
            _ => info!("{message}"),
        }
    }

    fn time_now_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    fn random_bytes(&self, len: usize) -> Vec<u8> {
        use rand::RngCore;
        let mut out = vec![0u8; len];
        rand::thread_rng().fill_bytes(&mut out);
        out
    }

    fn get_env(&self, key: &str) -> Option<String> {
        self.runtime_config.global_environment.get(key).cloned()
    }

    fn http_call(
        &self,
        _method: &str,
        _url: &str,
        _headers: HashMap<String, String>,
        _body: Vec<u8>,
    ) -> Result<HttpCallResult, ExecutionError> {
        Err(ExecutionError::NotSupported {
            operation: "http_call".to_string(),
        })
    }

    fn put_result(
        &self,
        task_id: &str,
        data: Vec<u8>,
        metadata: HashMap<String, String>,
    ) -> Result<String, ExecutionError> {
        let _ = (task_id, data, metadata);
        Err(ExecutionError::NotSupported {
            operation: "put_result".to_string(),
        })
    }

    fn get_object(&self, id: &str) -> Result<Vec<u8>, ExecutionError> {
        let _ = id;
        Err(ExecutionError::NotSupported {
            operation: "get_object".to_string(),
        })
    }

    fn put_object(&self, name: &str, bytes: Vec<u8>) -> Result<String, ExecutionError> {
        let _ = (name, bytes);
        Err(ExecutionError::NotSupported {
            operation: "put_object".to_string(),
        })
    }
}
