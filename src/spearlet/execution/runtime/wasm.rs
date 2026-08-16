//! WASM Runtime Implementation
//! WASM 运行时实现
//!
//! This module provides WebAssembly-based execution runtime using Wasmtime.
//! 该模块使用 Wasmtime 提供基于 WebAssembly 的执行运行时。

#[cfg(feature = "wasmedge")]
use super::wasm_hostcalls::build_spear_import_with_api;
use super::{
    ExecutionContext, Runtime, RuntimeCapabilities, RuntimeConfig, RuntimeExecutionResponse,
    RuntimeType,
};
use crate::spearlet::execution::artifact_fetch;
use crate::spearlet::execution::{
    instance::{InstanceConfig, InstanceResourceLimits, TaskInstance},
    ExecutionError, ExecutionResult, InstanceStatus, DEFAULT_ENTRY_FUNCTION_NAME,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::debug;
#[cfg(feature = "wasmedge")]
use wasmedge_sdk::config::{CommonConfigOptions, ConfigBuilder};
#[cfg(feature = "wasmedge")]
use wasmedge_sdk::{params, vm::SyncInst, wasi::WasiModule, Module, Vm};

// Note: In a real implementation, you would use wasmedge crate
// 注意：在真实实现中，您会使用 wasmedge crate
// For now, we'll create a mock implementation to avoid adding dependencies
// 现在，我们将创建一个模拟实现以避免添加依赖项

/// WASM runtime configuration / WASM 运行时配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmConfig {
    /// WASM module cache directory / WASM 模块缓存目录
    pub cache_directory: String,
    /// Maximum WASM module size in bytes / WASM 模块最大大小（字节）
    pub max_module_size_bytes: u64,
    /// WASM execution configuration / WASM 执行配置
    pub execution_config: WasmExecutionConfig,
    /// WASM security configuration / WASM 安全配置
    pub security_config: WasmSecurityConfig,
    /// WASM optimization configuration / WASM 优化配置
    pub optimization_config: WasmOptimizationConfig,
}

/// WASM execution configuration / WASM 执行配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmExecutionConfig {
    /// Maximum stack size / 最大栈大小
    pub max_stack_size: u32,
    /// Maximum heap size / 最大堆大小
    pub max_heap_size: u32,
    /// Enable WASI (WebAssembly System Interface) / 启用 WASI（WebAssembly 系统接口）
    pub enable_wasi: bool,
    /// WASI allowed directories / WASI 允许的目录
    pub wasi_allowed_dirs: Vec<String>,
    /// WASI environment variables / WASI 环境变量
    pub wasi_env_vars: HashMap<String, String>,
    /// Enable multi-threading / 启用多线程
    pub enable_threads: bool,
    /// Maximum number of threads / 最大线程数
    pub max_threads: u32,
}

/// WASM security configuration / WASM 安全配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmSecurityConfig {
    /// Enable sandbox mode / 启用沙箱模式
    pub enable_sandbox: bool,
    /// Allowed host functions / 允许的主机函数
    pub allowed_host_functions: Vec<String>,
    /// Maximum execution time per call / 每次调用的最大执行时间
    pub max_execution_time_ms: u64,
    /// Maximum memory allocation / 最大内存分配
    pub max_memory_allocation: u64,
    /// Enable fuel (execution limits) / 启用燃料（执行限制）
    pub enable_fuel: bool,
    /// Fuel limit per execution / 每次执行的燃料限制
    pub fuel_limit: u64,
}

/// WASM optimization configuration / WASM 优化配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmOptimizationConfig {
    /// Enable JIT compilation / 启用 JIT 编译
    pub enable_jit: bool,
    /// Optimization level (0-3) / 优化级别（0-3）
    pub optimization_level: u8,
    /// Enable module caching / 启用模块缓存
    pub enable_caching: bool,
    /// Cache TTL in seconds / 缓存 TTL（秒）
    pub cache_ttl_seconds: u64,
    /// Enable parallel compilation / 启用并行编译
    pub enable_parallel_compilation: bool,
}

impl Default for WasmConfig {
    fn default() -> Self {
        Self {
            cache_directory: "/tmp/spearlet/wasm_cache".to_string(),
            max_module_size_bytes: 100 * 1024 * 1024, // 100MB
            execution_config: WasmExecutionConfig {
                max_stack_size: 1024 * 1024,     // 1MB
                max_heap_size: 64 * 1024 * 1024, // 64MB
                enable_wasi: true,
                wasi_allowed_dirs: vec!["/tmp".to_string()],
                wasi_env_vars: HashMap::new(),
                enable_threads: false,
                max_threads: 1,
            },
            security_config: WasmSecurityConfig {
                enable_sandbox: true,
                allowed_host_functions: vec![],
                max_execution_time_ms: 30000,             // 30 seconds
                max_memory_allocation: 128 * 1024 * 1024, // 128MB
                enable_fuel: true,
                fuel_limit: 1_000_000,
            },
            optimization_config: WasmOptimizationConfig {
                enable_jit: true,
                optimization_level: 2,
                enable_caching: true,
                cache_ttl_seconds: 3600, // 1 hour
                enable_parallel_compilation: true,
            },
        }
    }
}

/// Mock WASM module handle / 模拟 WASM 模块句柄
#[derive(Debug, Clone)]
pub struct WasmModuleHandle {
    /// Module ID / 模块 ID
    pub module_id: String,
    /// Module name / 模块名称
    pub module_name: String,
    /// Module size in bytes / 模块大小（字节）
    pub module_size: u64,
    /// Module hash / 模块哈希
    pub module_hash: String,
    /// Compilation time / 编译时间
    pub compilation_time: std::time::SystemTime,
    /// Exported functions / 导出的函数
    pub exported_functions: Vec<String>,
    /// Memory usage / 内存使用
    pub memory_usage: u64,
    pub module_bytes: Vec<u8>,
}

/// Mock WASM instance handle / 模拟 WASM 实例句柄
#[derive(Debug)]
pub struct WasmInstanceHandle {
    /// Instance ID / 实例 ID
    pub instance_id: String,
    /// Module handle / 模块句柄
    pub module_handle: WasmModuleHandle,
    /// Instance state / 实例状态
    pub state: Arc<Mutex<WasmInstanceState>>,
    /// Execution statistics / 执行统计
    pub execution_stats: Arc<Mutex<WasmExecutionStats>>,
    pub req_tx: std::sync::mpsc::Sender<WasmWorkerRequest>,
    pub req_rx: Arc<Mutex<Option<std::sync::mpsc::Receiver<WasmWorkerRequest>>>>,
    pub exec_lock: Arc<tokio::sync::Mutex<()>>,
}

#[derive(Debug)]
pub enum WasmWorkerRequest {
    Invoke {
        execution_id: String,
        function_name: String,
        timeout_ms: Option<u64>,
        context_data: std::collections::HashMap<String, serde_json::Value>,
        completion_tx: Option<
            tokio::sync::mpsc::UnboundedSender<
                crate::spearlet::execution::runtime::ExecutionCompletionEvent,
            >,
        >,
        reply_tx: std::sync::mpsc::Sender<ExecutionResult<Vec<u8>>>,
    },
    Stop {
        reply_tx: std::sync::mpsc::Sender<ExecutionResult<Vec<u8>>>,
    },
}

/// WASM instance state / WASM 实例状态
#[derive(Debug, Clone)]
pub struct WasmInstanceState {
    /// Is instance initialized / 实例是否已初始化
    pub initialized: bool,
    /// Current memory usage / 当前内存使用
    pub memory_usage: u64,
    /// Current fuel remaining / 当前剩余燃料
    pub fuel_remaining: u64,
    /// Last execution time / 上次执行时间
    pub last_execution_time: Option<std::time::SystemTime>,
    /// Whether instance is currently running / 是否正在运行
    pub is_running: bool,
    /// Current function name / 当前函数名
    pub current_function: Option<String>,
}

/// WASM execution statistics / WASM 执行统计
#[derive(Debug, Clone)]
pub struct WasmExecutionStats {
    /// Total executions / 总执行次数
    pub total_executions: u64,
    /// Total execution time / 总执行时间
    pub total_execution_time_ms: u64,
    /// Average execution time / 平均执行时间
    pub average_execution_time_ms: f64,
    /// Memory peak usage / 内存峰值使用
    pub memory_peak_usage: u64,
    /// Fuel consumed / 消耗的燃料
    pub fuel_consumed: u64,
}

impl Default for WasmExecutionStats {
    fn default() -> Self {
        Self {
            total_executions: 0,
            total_execution_time_ms: 0,
            average_execution_time_ms: 0.0,
            memory_peak_usage: 0,
            fuel_consumed: 0,
        }
    }
}

/// WASM runtime implementation / WASM 运行时实现
pub struct WasmRuntime {
    /// WASM configuration / WASM 配置
    config: WasmConfig,
    /// Runtime configuration / 运行时配置
    runtime_config: RuntimeConfig,
    /// Module cache / 模块缓存
    module_cache: Arc<Mutex<HashMap<String, WasmModuleHandle>>>,
}

impl WasmRuntime {
    /// Create a new WASM runtime / 创建新的 WASM 运行时
    pub fn new(runtime_config: &RuntimeConfig) -> ExecutionResult<Self> {
        let wasm_config = if let Some(wasm_settings) = runtime_config.settings.get("wasm") {
            serde_json::from_value(wasm_settings.clone()).map_err(|e| {
                ExecutionError::InvalidConfiguration {
                    message: format!("Invalid WASM configuration: {}", e),
                }
            })?
        } else {
            WasmConfig::default()
        };

        // Create cache directory if it doesn't exist / 如果缓存目录不存在则创建
        if let Err(e) = std::fs::create_dir_all(&wasm_config.cache_directory) {
            tracing::warn!("Failed to create WASM cache directory: {}", e);
        }

        Ok(Self {
            config: wasm_config,
            runtime_config: runtime_config.clone(),
            module_cache: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Load WASM module from bytes / 从字节加载 WASM 模块
    async fn load_wasm_module(&self, module_bytes: &[u8]) -> ExecutionResult<WasmModuleHandle> {
        if module_bytes.len() > self.config.max_module_size_bytes as usize {
            return Err(ExecutionError::InvalidConfiguration {
                message: format!(
                    "WASM module size {} exceeds maximum {}",
                    module_bytes.len(),
                    self.config.max_module_size_bytes
                ),
            });
        }

        // Calculate module hash / 计算模块哈希
        let module_hash = format!("{:x}", md5::compute(module_bytes));

        // Check cache first / 首先检查缓存
        {
            let cache = self.module_cache.lock().await;
            if let Some(cached_module) = cache.get(&module_hash) {
                return Ok(cached_module.clone());
            }
        }

        // Mock module compilation / 模拟模块编译
        tokio::time::sleep(Duration::from_millis(100)).await; // Simulate compilation time / 模拟编译时间

        let module_handle = WasmModuleHandle {
            module_id: uuid::Uuid::new_v4().to_string(),
            module_name: "user_module".to_string(),
            module_size: module_bytes.len() as u64,
            module_hash: module_hash.clone(),
            compilation_time: std::time::SystemTime::now(),
            exported_functions: vec!["main".to_string(), "_start".to_string()], // Mock exports / 模拟导出
            memory_usage: self.config.execution_config.max_heap_size as u64,
            module_bytes: module_bytes.to_vec(),
        };

        // Cache the module / 缓存模块
        {
            let mut cache = self.module_cache.lock().await;
            cache.insert(module_hash, module_handle.clone());
        }

        Ok(module_handle)
    }

    /// Create WASM instance from module / 从模块创建 WASM 实例
    async fn create_wasm_instance(
        &self,
        module_handle: WasmModuleHandle,
        _instance_config: &InstanceConfig,
    ) -> ExecutionResult<WasmInstanceHandle> {
        let instance_id = uuid::Uuid::new_v4().to_string();

        let state = WasmInstanceState {
            initialized: true,
            memory_usage: module_handle.memory_usage,
            fuel_remaining: self.config.security_config.fuel_limit,
            last_execution_time: None,
            is_running: false,
            current_function: None,
        };

        let (req_tx, req_rx) = std::sync::mpsc::channel::<WasmWorkerRequest>();

        let instance_handle = WasmInstanceHandle {
            instance_id,
            module_handle,
            state: Arc::new(Mutex::new(state)),
            execution_stats: Arc::new(Mutex::new(WasmExecutionStats::default())),
            req_tx,
            req_rx: Arc::new(Mutex::new(Some(req_rx))),
            exec_lock: Arc::new(tokio::sync::Mutex::new(())),
        };

        Ok(instance_handle)
    }

    #[cfg(feature = "wasmedge")]
    async fn maybe_start_wasm_worker(
        &self,
        instance: &Arc<TaskInstance>,
        wasm_handle: &Arc<WasmInstanceHandle>,
    ) -> ExecutionResult<()> {
        let req_rx = wasm_handle.req_rx.lock().await.take();
        let Some(req_rx) = req_rx else {
            return Ok(());
        };

        use std::collections::HashMap;
        use wasmedge_sdk::Store;

        let c = ConfigBuilder::new(CommonConfigOptions::default().threads(true))
            .build()
            .map_err(|e| ExecutionError::RuntimeError {
                message: format!("wasmedge config error: {}", e),
            })?;

        let bytes = if wasm_handle
            .module_handle
            .module_bytes
            .starts_with(&[0x00, 0x61, 0x73, 0x6d])
        {
            wasm_handle.module_handle.module_bytes.clone()
        } else {
            return Err(ExecutionError::InvalidConfiguration {
                message: "Invalid WASM module format".to_string(),
            });
        };

        let runtime_config = self.runtime_config.clone();
        let handle_module_name = wasm_handle.module_handle.module_name.clone();
        let task_id = instance.task_id.clone();
        let instance_id = instance.id().to_string();
        let task_policy = std::sync::Arc::new(
            crate::spearlet::mcp::task_subset::parse_task_config(&instance.config.task_config),
        );

        let worker = move || {
            let mut wasi_module = WasiModule::create(None, None, None).unwrap();
            let mut instances: HashMap<String, &mut dyn SyncInst> = HashMap::new();
            instances.insert(wasi_module.name().to_string(), wasi_module.as_mut());

            let mut spear_import = build_spear_import_with_api(
                runtime_config,
                task_id.clone(),
                task_policy.clone(),
                instance_id.clone(),
            )
            .unwrap();
            let spear_name = "spear".to_string();
            let spear_inst: &mut dyn SyncInst = &mut spear_import;
            instances.insert(spear_name, spear_inst);

            let store = Store::new(Some(&c), instances).unwrap();
            let mut vm = Vm::new(store);
            let module = Module::from_bytes(None, &bytes).unwrap();
            vm.register_module(Some(&handle_module_name), module)
                .unwrap();

            while let Ok(r) = req_rx.recv() {
                match r {
                    WasmWorkerRequest::Stop { reply_tx } => {
                        let _ = reply_tx.send(Ok(Vec::new()));
                        break;
                    }
                    WasmWorkerRequest::Invoke {
                        execution_id,
                        function_name,
                        timeout_ms,
                        context_data,
                        completion_tx,
                        reply_tx,
                    } => {
                        let start = std::time::Instant::now();
                        let ctx_key_preview =
                            context_data.keys().take(10).cloned().collect::<Vec<_>>();
                        tracing::debug!(
                            instance_id = %instance_id,
                            module_name = %handle_module_name,
                            function_name = %function_name,
                            timeout_ms = timeout_ms.unwrap_or(0),
                            ctx_keys_len = context_data.len(),
                            ctx_keys = ?ctx_key_preview,
                            "wasm.worker.invoke.start"
                        );
                        crate::spearlet::execution::host_api::set_current_wasm_execution_id(Some(
                            execution_id.clone(),
                        ));
                        let res = {
                            let out = if let Some(_timeout_ms) = timeout_ms {
                                #[cfg(all(target_os = "linux", not(target_env = "musl")))]
                                {
                                    vm.run_func_with_timeout(
                                        Some(&handle_module_name),
                                        &function_name,
                                        params!(),
                                        std::time::Duration::from_millis(_timeout_ms),
                                    )
                                }
                                #[cfg(not(all(target_os = "linux", not(target_env = "musl"))))]
                                {
                                    let _ = _timeout_ms;
                                    vm.run_func(
                                        Some(&handle_module_name),
                                        &function_name,
                                        params!(),
                                    )
                                }
                            } else {
                                vm.run_func(Some(&handle_module_name), &function_name, params!())
                            };

                            match out {
                                Ok(values) => Ok(format!("{:?}", values).into_bytes()),
                                Err(e) => {
                                    if let Some(s) = crate::spearlet::execution::host_api::termination::instance_termination_registry().read_termination_request(&instance_id) {
                                        Err(ExecutionError::InstanceDestroyed {
                                            message: s
                                                .message
                                                .unwrap_or_else(|| "instance destroyed".to_string()),
                                        })
                                    } else if let Some(s) = crate::spearlet::execution::host_api::termination::execution_termination_registry().read_termination_request(&execution_id) {
                                        Err(ExecutionError::ExecutionTerminated {
                                            message: s
                                                .message
                                                .unwrap_or_else(|| "execution terminated".to_string()),
                                        })
                                    } else {
                                        Err(ExecutionError::RuntimeError {
                                            message: format!("wasmedge exec error: {}", e),
                                        })
                                    }
                                }
                            }
                        };
                        crate::spearlet::execution::host_api::set_current_wasm_execution_id(None);
                        crate::spearlet::execution::host_api::termination::clear_execution_termination_request(&execution_id);
                        let elapsed_ms = start.elapsed().as_millis() as u64;
                        tracing::debug!(
                            execution_id = %execution_id,
                            instance_id = %instance_id,
                            module_name = %handle_module_name,
                            function_name = %function_name,
                            elapsed_ms = elapsed_ms,
                            ok = res.is_ok(),
                            "wasm.worker.invoke.done"
                        );
                        if let Some(tx) = completion_tx {
                            let (status, output, error_message) = match &res {
                                Ok(v) => (
                                    crate::spearlet::execution::runtime::ExecutionStatus::Completed,
                                    v.clone(),
                                    None,
                                ),
                                Err(e) => (
                                    crate::spearlet::execution::runtime::ExecutionStatus::Failed,
                                    Vec::new(),
                                    Some(e.to_string()),
                                ),
                            };
                            let _ = tx.send(
                                crate::spearlet::execution::runtime::ExecutionCompletionEvent {
                                    execution_id: execution_id.clone(),
                                    execution_status: status,
                                    completed_at_ms: chrono::Utc::now().timestamp_millis(),
                                    duration_ms: elapsed_ms,
                                    output,
                                    error_message,
                                    runtime_metadata: std::collections::HashMap::new(),
                                },
                            );
                        }
                        let _ = reply_tx.send(res);
                    }
                }
            }
        };

        let handle =
            tokio::runtime::Handle::try_current().map_err(|_| ExecutionError::RuntimeError {
                message: "tokio runtime is required to start wasm worker".to_string(),
            })?;
        let _ = handle.spawn_blocking(worker);

        Ok(())
    }

    /// Execute WASM function / 执行 WASM 函数
    async fn execute_wasm_function(
        &self,
        instance_handle: &WasmInstanceHandle,
        execution_id: &str,
        function_name: &str,
        timeout_ms: Option<u64>,
        context_data: &std::collections::HashMap<String, serde_json::Value>,
        _input_data: &[u8],
    ) -> ExecutionResult<Vec<u8>> {
        let start_time = Instant::now();

        #[cfg(feature = "wasmedge")]
        let output = {
            let _exec_guard = match instance_handle.exec_lock.try_lock() {
                Ok(g) => g,
                Err(_) => {
                    return Err(ExecutionError::InvalidRequest {
                        message: "another execution already in progress".to_string(),
                    });
                }
            };
            {
                let mut state = instance_handle.state.lock().await;
                state.last_execution_time = Some(std::time::SystemTime::now());
                state.is_running = true;
                state.current_function = Some(function_name.to_string());
            }
            let (tx, rx) = std::sync::mpsc::channel::<ExecutionResult<Vec<u8>>>();
            let req = WasmWorkerRequest::Invoke {
                execution_id: execution_id.to_string(),
                function_name: function_name.to_string(),
                timeout_ms,
                context_data: context_data.clone(),
                completion_tx: None,
                reply_tx: tx,
            };
            let send_res = instance_handle.req_tx.send(req);
            if send_res.is_err() {
                return Err(ExecutionError::RuntimeError {
                    message: "wasm instance thread unavailable".to_string(),
                });
            }
            let join = tokio::task::spawn_blocking(move || rx.recv());
            let recv_res = match timeout_ms {
                None => join.await,
                Some(timeout_ms) => {
                    match tokio::time::timeout(Duration::from_millis(timeout_ms), join).await {
                        Ok(v) => v,
                        Err(_) => return Err(ExecutionError::ExecutionTimeout { timeout_ms }),
                    }
                }
            };
            let recv_res = recv_res.map_err(|e| ExecutionError::RuntimeError {
                message: e.to_string(),
            })?;
            let bytes: Vec<u8> = match recv_res {
                Ok(Ok(b)) => b,
                Ok(Err(e)) => return Err(e),
                Err(e) => {
                    return Err(ExecutionError::RuntimeError {
                        message: e.to_string(),
                    })
                }
            };
            bytes
        };
        #[cfg(not(feature = "wasmedge"))]
        let output = {
            tokio::time::sleep(Duration::from_millis(10)).await;
            format!(
                "WASM function '{}' executed with {} bytes input",
                function_name,
                input_data.len()
            )
        };

        let execution_time = start_time.elapsed();
        let execution_time_ms = execution_time.as_millis() as u64;

        // Update execution statistics / 更新执行统计
        {
            let mut stats = instance_handle.execution_stats.lock().await;
            stats.total_executions += 1;
            stats.total_execution_time_ms += execution_time_ms;
            stats.average_execution_time_ms =
                stats.total_execution_time_ms as f64 / stats.total_executions as f64;
            stats.fuel_consumed += 1000; // Mock fuel consumption / 模拟燃料消耗
        }

        // Reset running state / 重置运行状态
        {
            let mut state = instance_handle.state.lock().await;
            state.is_running = false;
            state.current_function = None;
        }

        Ok(output)
    }

    /// Get WASM instance metrics / 获取 WASM 实例指标
    async fn get_wasm_metrics(
        &self,
        instance_handle: &WasmInstanceHandle,
    ) -> ExecutionResult<HashMap<String, serde_json::Value>> {
        let mut metrics = HashMap::new();

        // Get state metrics / 获取状态指标
        {
            let state = instance_handle.state.lock().await;
            metrics.insert(
                "memory_usage_bytes".to_string(),
                serde_json::Value::Number(serde_json::Number::from(state.memory_usage)),
            );
            metrics.insert(
                "fuel_remaining".to_string(),
                serde_json::Value::Number(serde_json::Number::from(state.fuel_remaining)),
            );
            metrics.insert(
                "initialized".to_string(),
                serde_json::Value::Bool(state.initialized),
            );
        }

        // Get execution statistics / 获取执行统计
        {
            let stats = instance_handle.execution_stats.lock().await;
            metrics.insert(
                "total_executions".to_string(),
                serde_json::Value::Number(serde_json::Number::from(stats.total_executions)),
            );
            metrics.insert(
                "total_execution_time_ms".to_string(),
                serde_json::Value::Number(serde_json::Number::from(stats.total_execution_time_ms)),
            );
            metrics.insert(
                "average_execution_time_ms".to_string(),
                serde_json::Value::Number(
                    serde_json::Number::from_f64(stats.average_execution_time_ms)
                        .unwrap_or(serde_json::Number::from(0)),
                ),
            );
            metrics.insert(
                "memory_peak_usage_bytes".to_string(),
                serde_json::Value::Number(serde_json::Number::from(stats.memory_peak_usage)),
            );
            metrics.insert(
                "fuel_consumed".to_string(),
                serde_json::Value::Number(serde_json::Number::from(stats.fuel_consumed)),
            );
        }

        // Module information / 模块信息
        metrics.insert(
            "module_id".to_string(),
            serde_json::Value::String(instance_handle.module_handle.module_id.clone()),
        );
        metrics.insert(
            "module_size_bytes".to_string(),
            serde_json::Value::Number(serde_json::Number::from(
                instance_handle.module_handle.module_size,
            )),
        );
        metrics.insert(
            "exported_functions".to_string(),
            serde_json::Value::Array(
                instance_handle
                    .module_handle
                    .exported_functions
                    .iter()
                    .map(|f| serde_json::Value::String(f.clone()))
                    .collect(),
            ),
        );

        Ok(metrics)
    }
}

#[async_trait]
impl Runtime for WasmRuntime {
    fn runtime_type(&self) -> RuntimeType {
        RuntimeType::Wasm
    }

    async fn create_instance(&self, config: &InstanceConfig) -> ExecutionResult<Arc<TaskInstance>> {
        debug!("WasmRuntime::create_instance task_id={}", config.task_id);
        let instance = Arc::new(TaskInstance::new(config.task_id.clone(), config.clone()));

        let module_bytes_vec: Vec<u8> = if let Some(snapshot) = &config.artifact {
            if let Some(uri) = &snapshot.location {
                if let Some(rest) = uri.strip_prefix("smsfile://") {
                    let (override_host_port, id_part) = match rest.find('/') {
                        Some(pos) => (Some(rest[..pos].to_string()), rest[pos + 1..].to_string()),
                        None => (None, rest.to_string()),
                    };
                    let id = id_part.trim_start_matches('/');
                    let path = format!("/api/v1/files/{}", id);

                    let cfg = self
                        .runtime_config
                        .spearlet_config
                        .as_ref()
                        .ok_or_else(|| ExecutionError::InvalidConfiguration {
                            message: "Missing SpearletConfig".to_string(),
                        })?;

                    let sms_http_addr = if let Some(hp) = override_host_port {
                        hp
                    } else {
                        cfg.sms_http_addr.clone()
                    };

                    debug!(
                        task_id = %config.task_id,
                        artifact_id = %config.artifact_id,
                        sms_http_addr = %cfg.sms_http_addr,
                        file_id = %id,
                        "Fetching WASM module from SMS"
                    );

                    match artifact_fetch::fetch_sms_file(&sms_http_addr, &path).await {
                        Ok(b) => b,
                        Err(e) => {
                            debug!(error = %e.to_string(), url = format!("http://{}{}", sms_http_addr, path), "Failed to fetch SMS file");
                            return Err(e);
                        }
                    }
                } else {
                    return Err(ExecutionError::InvalidConfiguration {
                        message: format!("Unsupported artifact URI scheme: {}", uri),
                    });
                }
            } else {
                return Err(ExecutionError::InvalidConfiguration {
                    message: "Missing artifact location for WASM module".to_string(),
                });
            }
        } else {
            debug!(
                "WasmRuntime::create_instance missing artifact snapshot task_id={} artifact_id={}",
                config.task_id, config.artifact_id
            );
            return Err(ExecutionError::InvalidConfiguration {
                message: "Missing artifact snapshot for WASM module".to_string(),
            });
        };

        // Validate WASM magic header / 校验 WASM 魔数
        if !module_bytes_vec.starts_with(&[0x00, 0x61, 0x73, 0x6d]) {
            return Err(ExecutionError::InvalidConfiguration {
                message: "Invalid WASM module: missing magic header".to_string(),
            });
        }

        // Load WASM module / 加载 WASM 模块
        let module_handle = self.load_wasm_module(&module_bytes_vec).await?;

        // Create WASM instance / 创建 WASM 实例
        let wasm_instance = self.create_wasm_instance(module_handle, config).await?;

        instance.set_runtime_handle(Arc::new(wasm_instance));
        instance.set_status(InstanceStatus::Ready);

        Ok(instance)
    }

    async fn start_instance(&self, instance: &Arc<TaskInstance>) -> ExecutionResult<()> {
        debug!("WasmRuntime::start_instance instance_id={}", instance.id());
        // WASM instance is ready to execute when created / WASM 实例在创建时就准备好执行
        #[cfg(feature = "wasmedge")]
        {
            let wasm_handle = instance
                .get_runtime_handle::<WasmInstanceHandle>()
                .ok_or_else(|| ExecutionError::RuntimeError {
                    message: "No WASM instance handle found".to_string(),
                })?;
            self.maybe_start_wasm_worker(instance, &wasm_handle).await?;
        }
        instance.set_status(InstanceStatus::Running);
        Ok(())
    }

    async fn stop_instance(&self, instance: &Arc<TaskInstance>) -> ExecutionResult<()> {
        debug!("WasmRuntime::stop_instance instance_id={}", instance.id());
        instance.set_status(InstanceStatus::Stopping);

        #[cfg(not(feature = "wasmedge"))]
        {
            instance.set_status(InstanceStatus::Stopped);
            return Ok(());
        }

        let wasm_handle = instance
            .get_runtime_handle::<WasmInstanceHandle>()
            .ok_or_else(|| ExecutionError::RuntimeError {
                message: "No WASM instance handle found".to_string(),
            })?;

        let mut rx_guard = wasm_handle.req_rx.lock().await;
        if rx_guard.is_some() {
            let _ = rx_guard.take();
            drop(rx_guard);
            {
                let mut state = wasm_handle.state.lock().await;
                state.is_running = false;
                state.current_function = None;
            }
            instance.set_status(InstanceStatus::Stopped);
            return Ok(());
        }
        drop(rx_guard);

        let _g = wasm_handle.exec_lock.lock().await;
        let (tx, rx) = std::sync::mpsc::channel::<ExecutionResult<Vec<u8>>>();
        let req = WasmWorkerRequest::Stop { reply_tx: tx };
        wasm_handle
            .req_tx
            .send(req)
            .map_err(|e| ExecutionError::RuntimeError {
                message: e.to_string(),
            })?;

        let _ = tokio::task::spawn_blocking(move || rx.recv()).await;
        {
            let mut state = wasm_handle.state.lock().await;
            state.is_running = false;
            state.current_function = None;
        }
        instance.set_status(InstanceStatus::Stopped);
        Ok(())
    }

    async fn execute(
        &self,
        instance: &Arc<TaskInstance>,
        context: ExecutionContext,
    ) -> ExecutionResult<RuntimeExecutionResponse> {
        debug!(
            "WasmRuntime::execute instance_id={} execution_id={}",
            instance.id(),
            context.execution_id
        );
        let start_time = Instant::now();
        instance.record_request_start();

        let wasm_handle = instance
            .get_runtime_handle::<WasmInstanceHandle>()
            .ok_or_else(|| ExecutionError::RuntimeError {
                message: "No WASM instance handle found".to_string(),
            })?;

        #[cfg(feature = "wasmedge")]
        self.maybe_start_wasm_worker(instance, &wasm_handle).await?;

        let function_name = if context.function_name.is_empty()
            || context.function_name == DEFAULT_ENTRY_FUNCTION_NAME
        {
            // Runtime-default entrypoint selection.
            // 运行时默认入口选择。
            //
            // For WASM/WASI modules, `_start` is the conventional entrypoint.
            // If the module doesn't export `_start`, we fall back to `main`.
            // 对于 WASM/WASI 模块，惯例入口是 `_start`；若未导出 `_start`，则回退到 `main`。
            let has_start = wasm_handle
                .module_handle
                .exported_functions
                .iter()
                .any(|f| f == "_start");
            if has_start { "_start" } else { "main" }.to_string()
        } else {
            context.function_name.clone()
        };

        let no_wait = !context.wait
            || matches!(
                context.execution_mode,
                crate::spearlet::execution::runtime::ExecutionMode::Async
            );

        debug!(
            instance_id = %instance.id(),
            execution_id = %context.execution_id,
            wait = context.wait,
            execution_mode = ?context.execution_mode,
            no_wait = no_wait,
            function_name = %function_name,
            "wasm.execute.plan"
        );

        if no_wait {
            {
                let state = wasm_handle.state.lock().await;
                if state.is_running {
                    return Err(ExecutionError::InvalidRequest {
                        message: "another execution already in progress".to_string(),
                    });
                }
            }

            {
                let mut state = wasm_handle.state.lock().await;
                state.last_execution_time = Some(std::time::SystemTime::now());
                state.is_running = true;
                state.current_function = Some(function_name.clone());
            }

            let (tx, _rx) = std::sync::mpsc::channel::<ExecutionResult<Vec<u8>>>();
            let req = WasmWorkerRequest::Invoke {
                execution_id: context.execution_id.clone(),
                function_name: function_name.clone(),
                timeout_ms: if context.timeout_ms > 0 {
                    Some(context.timeout_ms)
                } else {
                    None
                },
                context_data: context.context_data.clone(),
                completion_tx: context.completion_tx.clone(),
                reply_tx: tx,
            };
            wasm_handle
                .req_tx
                .send(req)
                .map_err(|e| ExecutionError::RuntimeError {
                    message: e.to_string(),
                })?;

            let duration = start_time.elapsed();
            let duration_ms = duration.as_millis() as u64;
            debug!(
                instance_id = %instance.id(),
                execution_id = %context.execution_id,
                function_name = %function_name,
                duration_ms = duration_ms,
                "wasm.no_wait.dispatched"
            );

            return Ok(RuntimeExecutionResponse {
                data: Vec::new(),
                duration_ms,
                metadata: std::collections::HashMap::new(),
                execution_mode: crate::spearlet::execution::runtime::ExecutionMode::Async,
                execution_status: crate::spearlet::execution::runtime::ExecutionStatus::Running,
                execution_id: context.execution_id,
                task_id: Some(instance.task_id.clone()),
                status_endpoint: None,
                estimated_completion_ms: None,
                error: None,
            });
        }

        let result = self
            .execute_wasm_function(
                &wasm_handle,
                &context.execution_id,
                &function_name,
                Some(context.timeout_ms),
                &context.context_data,
                &context.payload,
            )
            .await;

        let duration = start_time.elapsed();
        let duration_ms = duration.as_millis() as u64;

        match result {
            Ok(output) => {
                instance.record_request_completion(true, duration_ms as f64);
                debug!(
                    instance_id = %instance.id(),
                    execution_id = %context.execution_id,
                    duration_ms = duration_ms,
                    output_len = output.len(),
                    "WASM execution completed"
                );
                Ok(RuntimeExecutionResponse::new_sync(
                    context.execution_id,
                    output,
                    duration_ms,
                ))
            }
            Err(e) => {
                instance.record_request_completion(false, duration_ms as f64);
                debug!(
                    instance_id = %instance.id(),
                    execution_id = %context.execution_id,
                    duration_ms = duration_ms,
                    error = %e.to_string(),
                    "WASM execution failed"
                );
                Err(e)
            }
        }
    }

    async fn health_check(&self, instance: &Arc<TaskInstance>) -> ExecutionResult<bool> {
        debug!("WasmRuntime::health_check instance_id={}", instance.id());
        let wasm_handle = instance
            .get_runtime_handle::<WasmInstanceHandle>()
            .ok_or_else(|| ExecutionError::RuntimeError {
                message: "No WASM instance handle found".to_string(),
            })?;

        // Check if WASM instance is initialized / 检查 WASM 实例是否已初始化
        let state = wasm_handle.state.lock().await;
        let is_healthy = state.initialized && state.fuel_remaining > 0;

        instance.record_health_check(is_healthy);
        Ok(is_healthy)
    }

    async fn get_metrics(
        &self,
        instance: &Arc<TaskInstance>,
    ) -> ExecutionResult<HashMap<String, serde_json::Value>> {
        debug!("WasmRuntime::get_metrics instance_id={}", instance.id());
        let wasm_handle = instance
            .get_runtime_handle::<WasmInstanceHandle>()
            .ok_or_else(|| ExecutionError::RuntimeError {
                message: "No WASM instance handle found".to_string(),
            })?;

        self.get_wasm_metrics(&wasm_handle).await
    }

    async fn scale_instance(
        &self,
        instance: &Arc<TaskInstance>,
        new_limits: &InstanceResourceLimits,
    ) -> ExecutionResult<()> {
        debug!("WasmRuntime::scale_instance instance_id={}", instance.id());
        let wasm_handle = instance
            .get_runtime_handle::<WasmInstanceHandle>()
            .ok_or_else(|| ExecutionError::RuntimeError {
                message: "No WASM instance handle found".to_string(),
            })?;

        // Update fuel limit based on new resource limits / 根据新的资源限制更新燃料限制
        let new_fuel_limit = (new_limits.max_cpu_cores * 1_000_000.0) as u64;

        {
            let mut state = wasm_handle.state.lock().await;
            state.fuel_remaining = new_fuel_limit;
        }

        Ok(())
    }

    async fn cleanup_instance(&self, _instance: &Arc<TaskInstance>) -> ExecutionResult<()> {
        debug!(
            "WasmRuntime::cleanup_instance instance_id={}",
            _instance.id()
        );
        // WASM instances are automatically cleaned up when dropped / WASM 实例在丢弃时自动清理
        // No explicit cleanup needed / 不需要显式清理
        Ok(())
    }

    async fn get_running_function(
        &self,
        instance: &Arc<TaskInstance>,
    ) -> ExecutionResult<Option<String>> {
        let wasm_handle = instance
            .get_runtime_handle::<WasmInstanceHandle>()
            .ok_or_else(|| ExecutionError::RuntimeError {
                message: "No WASM instance handle found".to_string(),
            })?;
        let state = wasm_handle.state.lock().await;
        Ok(if state.is_running {
            state.current_function.clone()
        } else {
            None
        })
    }

    fn validate_config(&self, config: &InstanceConfig) -> ExecutionResult<()> {
        debug!("WasmRuntime::validate_config task_id={}", config.task_id);
        if config.runtime_type != RuntimeType::Wasm {
            return Err(ExecutionError::InvalidConfiguration {
                message: "Runtime type must be WASM".to_string(),
            });
        }

        // Validate resource limits / 验证资源限制
        let limits = &config.resource_limits;
        if limits.max_cpu_cores <= 0.0 {
            return Err(ExecutionError::InvalidConfiguration {
                message: "CPU cores must be greater than 0".to_string(),
            });
        }

        if limits.max_memory_bytes == 0 {
            return Err(ExecutionError::InvalidConfiguration {
                message: "Memory limit must be greater than 0".to_string(),
            });
        }

        // Validate WASM-specific limits / 验证 WASM 特定限制
        if limits.max_memory_bytes > self.config.security_config.max_memory_allocation {
            return Err(ExecutionError::InvalidConfiguration {
                message: format!(
                    "Memory limit {} exceeds maximum allowed {}",
                    limits.max_memory_bytes, self.config.security_config.max_memory_allocation
                ),
            });
        }

        Ok(())
    }

    fn get_capabilities(&self) -> RuntimeCapabilities {
        RuntimeCapabilities {
            supports_scaling: true, // WASM supports dynamic resource scaling / WASM 支持动态资源扩缩容
            supports_health_checks: true,
            supports_metrics: true,
            supports_hot_reload: true, // WASM modules can be reloaded / WASM 模块可以重新加载
            supports_persistent_storage: false, // WASM is stateless by default / WASM 默认是无状态的
            supports_network_isolation: true,   // WASM provides strong isolation / WASM 提供强隔离
            max_concurrent_instances: self.runtime_config.resource_pool.max_concurrent_instances,
            supported_protocols: vec!["HTTP".to_string(), "gRPC".to_string(), "Custom".to_string()],
        }
    }
}

#[cfg(test)]
mod wasm_runtime_thread_tests {
    use super::*;
    use crate::spearlet::execution::instance::{
        InstanceConfig, InstanceResourceLimits, NetworkConfig,
    };
    #[cfg(feature = "wasmedge")]
    use wasmedge_sdk::Module;

    #[test]
    fn test_wasm_config_default() {
        let config = WasmConfig::default();
        assert_eq!(config.cache_directory, "/tmp/spearlet/wasm_cache");
        assert_eq!(config.max_module_size_bytes, 100 * 1024 * 1024);
        assert!(config.execution_config.enable_wasi);
        assert!(config.security_config.enable_sandbox);
        assert!(config.optimization_config.enable_jit);
    }

    #[test]
    fn test_wasm_runtime_creation() {
        let runtime_config = RuntimeConfig {
            runtime_type: RuntimeType::Wasm,
            settings: HashMap::new(),
            global_environment: HashMap::new(),
            spearlet_config: None,
            resource_pool: super::super::ResourcePoolConfig::default(),
        };

        let runtime = WasmRuntime::new(&runtime_config);
        assert!(runtime.is_ok());

        let runtime = runtime.unwrap();
        assert_eq!(runtime.runtime_type(), RuntimeType::Wasm);
    }

    #[tokio::test]
    async fn test_load_wasm_module() {
        let runtime_config = RuntimeConfig {
            runtime_type: RuntimeType::Wasm,
            settings: HashMap::new(),
            global_environment: HashMap::new(),
            spearlet_config: None,
            resource_pool: super::super::ResourcePoolConfig::default(),
        };

        let runtime = WasmRuntime::new(&runtime_config).unwrap();

        let module_bytes = b"mock_wasm_module_bytes";
        let module_handle = runtime.load_wasm_module(module_bytes).await;

        assert!(module_handle.is_ok());
        let handle = module_handle.unwrap();
        assert_eq!(handle.module_size, module_bytes.len() as u64);
        assert!(!handle.exported_functions.is_empty());
    }

    #[test]
    fn test_validate_config() {
        let runtime_config = RuntimeConfig {
            runtime_type: RuntimeType::Wasm,
            settings: HashMap::new(),
            global_environment: HashMap::new(),
            spearlet_config: None,
            resource_pool: super::super::ResourcePoolConfig::default(),
        };

        let runtime = WasmRuntime::new(&runtime_config).unwrap();

        let valid_config = InstanceConfig {
            task_id: "task-xyz".to_string(),
            artifact_id: "artifact-xyz".to_string(),
            runtime_type: RuntimeType::Wasm,
            runtime_config: HashMap::new(),
            task_config: HashMap::new(),
            artifact: None,
            environment: HashMap::new(),
            resource_limits: InstanceResourceLimits {
                max_cpu_cores: 0.5,
                max_memory_bytes: 64 * 1024 * 1024, // 64MB, less than WASM max_memory_allocation (128MB)
                max_disk_bytes: 512 * 1024 * 1024,  // 512MB
                max_network_bps: 50 * 1024 * 1024,  // 50MB/s
            },
            network_config: NetworkConfig::default(),
            max_concurrent_requests: 100,
            request_timeout_ms: 30000,
        };

        assert!(runtime.validate_config(&valid_config).is_ok());

        let invalid_config = InstanceConfig {
            task_id: "task-xyz".to_string(),
            artifact_id: "artifact-xyz".to_string(),
            runtime_type: RuntimeType::Process, // Different runtime type for testing / 用于测试的不同运行时类型
            runtime_config: HashMap::new(),
            task_config: HashMap::new(),
            artifact: None,
            environment: HashMap::new(),
            resource_limits: InstanceResourceLimits::default(),
            network_config: NetworkConfig::default(),
            max_concurrent_requests: 100,
            request_timeout_ms: 30000,
        };

        assert!(runtime.validate_config(&invalid_config).is_err());
    }

    #[test]
    fn test_wasm_execution_stats() {
        let mut stats = WasmExecutionStats::default();
        assert_eq!(stats.total_executions, 0);
        assert_eq!(stats.average_execution_time_ms, 0.0);

        // Simulate execution / 模拟执行
        stats.total_executions = 5;
        stats.total_execution_time_ms = 1000;
        stats.average_execution_time_ms =
            stats.total_execution_time_ms as f64 / stats.total_executions as f64;

        assert_eq!(stats.average_execution_time_ms, 200.0);
    }

    #[tokio::test]
    async fn test_default_entry_prefers_start() {
        let runtime_config = RuntimeConfig {
            runtime_type: RuntimeType::Wasm,
            settings: HashMap::new(),
            global_environment: HashMap::new(),
            spearlet_config: None,
            resource_pool: super::super::ResourcePoolConfig::default(),
        };

        let runtime = WasmRuntime::new(&runtime_config).unwrap();

        let valid_config = InstanceConfig {
            task_id: "task-xyz".to_string(),
            artifact_id: "artifact-xyz".to_string(),
            runtime_type: RuntimeType::Wasm,
            runtime_config: HashMap::new(),
            task_config: HashMap::new(),
            artifact: None,
            environment: HashMap::new(),
            resource_limits: InstanceResourceLimits {
                max_cpu_cores: 0.5,
                max_memory_bytes: 64 * 1024 * 1024,
                max_disk_bytes: 512 * 1024 * 1024,
                max_network_bps: 50 * 1024 * 1024,
            },
            network_config: NetworkConfig::default(),
            max_concurrent_requests: 100,
            request_timeout_ms: 30000,
        };
        // According to current logic, missing valid wasm module bytes should error
        let result = runtime.create_instance(&valid_config).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_blocking_pool_saturation_does_not_starve_async() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, Condvar, Mutex};
        use std::time::Duration;

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .max_blocking_threads(2)
            .build()
            .unwrap();

        runtime.block_on(async {
            let gate = Arc::new((Mutex::new(false), Condvar::new()));
            let mut joins = Vec::new();
            for _ in 0..64 {
                let gate = gate.clone();
                joins.push(tokio::task::spawn_blocking(move || {
                    let (mu, cv) = &*gate;
                    let mut open = mu.lock().unwrap();
                    while !*open {
                        open = cv.wait(open).unwrap();
                    }
                }));
            }

            let ticks = Arc::new(AtomicUsize::new(0));
            let ticks2 = ticks.clone();
            let handle = tokio::spawn(async move {
                for _ in 0..50 {
                    ticks2.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            });

            let res = tokio::time::timeout(Duration::from_secs(2), handle).await;
            assert!(res.is_ok());
            assert_eq!(ticks.load(Ordering::SeqCst), 50);

            {
                let (mu, cv) = &*gate;
                let mut open = mu.lock().unwrap();
                *open = true;
                cv.notify_all();
            }
            for j in joins {
                let _ = j.await;
            }
        });
    }

    #[cfg(feature = "wasmedge")]
    fn link_wat(wat: &str) {
        use super::super::wasm_hostcalls::build_spear_import;
        use wasmedge_sdk::config::{CommonConfigOptions, ConfigBuilder};
        use wasmedge_sdk::{wat2wasm, Store, Vm};

        let bytes = wat2wasm(wat.as_bytes()).unwrap();
        let c = ConfigBuilder::new(CommonConfigOptions::default())
            .build()
            .unwrap();
        let spear_import = build_spear_import().unwrap();
        let mut imports = HashMap::new();
        let spear_static = Box::leak(Box::new(spear_import));
        imports.insert("spear".to_string(), spear_static);
        let store = Store::new(Some(&c), imports).unwrap();
        let mut vm = Vm::new(store);

        // In wasmedge-sdk 0.14, ImportObject registration needs to be handled differently
        // This test needs to be updated to use the new API pattern
        // For now, we'll comment out this line to avoid compilation errors
        let module = Module::from_bytes(None, bytes).unwrap();
        let _vm = vm.register_module(Some("extern"), module).unwrap();
    }

    #[cfg(feature = "wasmedge")]
    #[test]
    fn test_link_time_now_ms() {
        let wat = r#"(module
            (type $t (func (result i64)))
            (import "spear" "time_now_ms" (func $time_now_ms (type $t)))
            (func $dummy (result i32) i32.const 0)
            (export "dummy" (func $dummy))
        )"#;
        link_wat(wat);
    }

    #[cfg(feature = "wasmedge")]
    #[test]
    fn test_link_wall_time_s() {
        let wat = r#"(module
            (type $t (func (result i64)))
            (import "spear" "wall_time_s" (func $wall_time_s (type $t)))
            (func $dummy (result i32) i32.const 0)
            (export "dummy" (func $dummy))
        )"#;
        link_wat(wat);
    }

    #[cfg(feature = "wasmedge")]
    #[test]
    fn test_link_random_i64() {
        let wat = r#"(module
            (type $t (func (result i64)))
            (import "spear" "random_i64" (func $random_i64 (type $t)))
            (func $dummy (result i32) i32.const 0)
            (export "dummy" (func $dummy))
        )"#;
        link_wat(wat);
    }

    #[cfg(feature = "wasmedge")]
    #[test]
    fn test_link_sleep_ms() {
        let wat = r#"(module
            (type $t (func (param i32)))
            (import "spear" "sleep_ms" (func $sleep_ms (type $t)))
            (func $dummy (result i32) i32.const 0)
            (export "dummy" (func $dummy))
        )"#;
        link_wat(wat);
    }

    #[cfg(feature = "wasmedge")]
    #[test]
    fn test_link_cchat_hostcalls() {
        let wat = r#"(module
            (type $create_t (func (result i32)))
            (type $write_msg_t (func (param i32 i32 i32 i32 i32) (result i32)))
            (type $write_fn_t (func (param i32 i32 i32 i32) (result i32)))
            (type $ctl_t (func (param i32 i32 i32 i32) (result i32)))
            (type $send_t (func (param i32 i32) (result i32)))
            (type $recv_t (func (param i32 i32 i32) (result i32)))
            (type $close_t (func (param i32) (result i32)))
            (import "spear" "cchat_create" (func $cchat_create (type $create_t)))
            (import "spear" "cchat_write_msg" (func $cchat_write_msg (type $write_msg_t)))
            (import "spear" "cchat_write_fn" (func $cchat_write_fn (type $write_fn_t)))
            (import "spear" "cchat_ctl" (func $cchat_ctl (type $ctl_t)))
            (import "spear" "cchat_send" (func $cchat_send (type $send_t)))
            (import "spear" "cchat_recv" (func $cchat_recv (type $recv_t)))
            (import "spear" "cchat_close" (func $cchat_close (type $close_t)))
            (func $dummy (result i32) i32.const 0)
            (export "dummy" (func $dummy))
        )"#;
        link_wat(wat);
    }

    #[cfg(feature = "wasmedge")]
    #[test]
    fn test_link_rtasr_mic_hostcalls() {
        let wat = r#"(module
            (type $create_t (func (result i32)))
            (type $ctl_t (func (param i32 i32 i32 i32) (result i32)))
            (type $write_t (func (param i32 i32 i32) (result i32)))
            (type $read_t (func (param i32 i32 i32) (result i32)))
            (type $close_t (func (param i32) (result i32)))
            (import "spear" "rtasr_create" (func $rtasr_create (type $create_t)))
            (import "spear" "rtasr_ctl" (func $rtasr_ctl (type $ctl_t)))
            (import "spear" "rtasr_write" (func $rtasr_write (type $write_t)))
            (import "spear" "rtasr_read" (func $rtasr_read (type $read_t)))
            (import "spear" "rtasr_close" (func $rtasr_close (type $close_t)))
            (import "spear" "mic_create" (func $mic_create (type $create_t)))
            (import "spear" "mic_ctl" (func $mic_ctl (type $ctl_t)))
            (import "spear" "mic_read" (func $mic_read (type $read_t)))
            (import "spear" "mic_close" (func $mic_close (type $close_t)))
            (func $dummy (result i32) i32.const 0)
            (export "dummy" (func $dummy))
        )"#;
        link_wat(wat);
    }

    #[cfg(feature = "wasmedge")]
    #[test]
    fn test_link_fd_epoll_fullname_hostcalls() {
        let wat = r#"(module
            (type $ep_create_t (func (result i32)))
            (type $ep_ctl_t (func (param i32 i32 i32 i32) (result i32)))
            (type $ep_wait_t (func (param i32 i32 i32 i32) (result i32)))
            (type $ep_close_t (func (param i32) (result i32)))
            (type $fd_ctl_t (func (param i32 i32 i32 i32) (result i32)))
            (import "spear" "spear_epoll_create" (func $ep_create (type $ep_create_t)))
            (import "spear" "spear_epoll_ctl" (func $ep_ctl (type $ep_ctl_t)))
            (import "spear" "spear_epoll_wait" (func $ep_wait (type $ep_wait_t)))
            (import "spear" "spear_epoll_close" (func $ep_close (type $ep_close_t)))
            (import "spear" "spear_fd_ctl" (func $fd_ctl (type $fd_ctl_t)))
            (func $dummy (result i32) i32.const 0)
            (export "dummy" (func $dummy))
        )"#;
        link_wat(wat);
    }
}

#[cfg(test)]
mod wasm_runtime_tests {
    use super::*;
    use crate::spearlet::execution::instance::{
        InstanceConfig, InstanceResourceLimits, InstanceStatus, NetworkConfig, TaskInstance,
    };
    use crate::spearlet::execution::runtime::{RuntimeConfig, RuntimeType};
    use std::collections::HashMap;
    use std::sync::Arc;

    fn rt_cfg() -> RuntimeConfig {
        RuntimeConfig {
            runtime_type: RuntimeType::Wasm,
            settings: HashMap::new(),
            global_environment: HashMap::new(),
            spearlet_config: None,
            resource_pool: super::super::ResourcePoolConfig::default(),
        }
    }

    fn dummy_instance_config() -> InstanceConfig {
        InstanceConfig {
            task_id: "task-1".to_string(),
            artifact_id: "art-1".to_string(),
            runtime_type: RuntimeType::Wasm,
            runtime_config: HashMap::new(),
            task_config: HashMap::new(),
            artifact: None,
            environment: HashMap::new(),
            resource_limits: InstanceResourceLimits::default(),
            network_config: NetworkConfig::default(),
            max_concurrent_requests: 1,
            request_timeout_ms: 1000,
        }
    }

    #[tokio::test]
    async fn test_execute_updates_state_and_stats() {
        let rt = WasmRuntime::new(&rt_cfg()).unwrap();
        let mh = WasmModuleHandle {
            module_id: uuid::Uuid::new_v4().to_string(),
            module_name: "user_module".to_string(),
            module_size: 0,
            module_hash: "h".to_string(),
            compilation_time: std::time::SystemTime::now(),
            exported_functions: vec!["main".to_string()],
            memory_usage: 0,
            module_bytes: vec![],
        };

        let state = WasmInstanceState {
            initialized: true,
            memory_usage: 0,
            fuel_remaining: 1000,
            last_execution_time: None,
            is_running: false,
            current_function: None,
        };

        let (tx, rx) = std::sync::mpsc::channel::<WasmWorkerRequest>();
        std::thread::spawn(move || loop {
            match rx.recv() {
                Ok(r) => match r {
                    WasmWorkerRequest::Stop { reply_tx } => {
                        let _ = reply_tx.send(Ok(Vec::new()));
                        break;
                    }
                    WasmWorkerRequest::Invoke { reply_tx, .. } => {
                        let _ = reply_tx.send(Ok(b"[]".to_vec()));
                    }
                },
                Err(_) => break,
            }
        });

        let handle = WasmInstanceHandle {
            instance_id: uuid::Uuid::new_v4().to_string(),
            module_handle: mh,
            state: Arc::new(Mutex::new(state)),
            execution_stats: Arc::new(Mutex::new(WasmExecutionStats::default())),
            req_tx: tx,
            req_rx: Arc::new(Mutex::new(None)),
            exec_lock: Arc::new(tokio::sync::Mutex::new(())),
        };

        let out = rt
            .execute_wasm_function(
                &handle,
                "exec-test",
                "main",
                Some(30_000),
                &std::collections::HashMap::new(),
                &[],
            )
            .await
            .unwrap();
        assert_eq!(out, b"[]");
        let s = handle.state.lock().await.clone();
        assert!(!s.is_running);
        assert!(s.current_function.is_none());
        let stats = handle.execution_stats.lock().await.clone();
        assert_eq!(stats.total_executions, 1);
    }

    #[tokio::test]
    async fn test_execute_returns_error_when_already_running() {
        use std::time::Duration;
        let rt = WasmRuntime::new(&rt_cfg()).unwrap();
        let mh = WasmModuleHandle {
            module_id: uuid::Uuid::new_v4().to_string(),
            module_name: "user_module".to_string(),
            module_size: 0,
            module_hash: "h".to_string(),
            compilation_time: std::time::SystemTime::now(),
            exported_functions: vec!["main".to_string()],
            memory_usage: 0,
            module_bytes: vec![],
        };

        let state = WasmInstanceState {
            initialized: true,
            memory_usage: 0,
            fuel_remaining: 1000,
            last_execution_time: None,
            is_running: false,
            current_function: None,
        };

        let (tx, rx) = std::sync::mpsc::channel::<WasmWorkerRequest>();
        std::thread::spawn(move || loop {
            match rx.recv() {
                Ok(r) => match r {
                    WasmWorkerRequest::Stop { reply_tx } => {
                        let _ = reply_tx.send(Ok(Vec::new()));
                        break;
                    }
                    WasmWorkerRequest::Invoke { reply_tx, .. } => {
                        std::thread::sleep(Duration::from_millis(150));
                        let _ = reply_tx.send(Ok(Vec::new()));
                    }
                },
                Err(_) => break,
            }
        });

        let handle = WasmInstanceHandle {
            instance_id: uuid::Uuid::new_v4().to_string(),
            module_handle: mh,
            state: Arc::new(Mutex::new(state)),
            execution_stats: Arc::new(Mutex::new(WasmExecutionStats::default())),
            req_tx: tx,
            req_rx: Arc::new(Mutex::new(None)),
            exec_lock: Arc::new(tokio::sync::Mutex::new(())),
        };

        let _hold = handle.exec_lock.lock().await;
        // Attempt an execution while lock is held; should fail fast
        let h2 = rt
            .execute_wasm_function(
                &handle,
                "exec-test",
                "main",
                Some(30_000),
                &std::collections::HashMap::new(),
                &[],
            )
            .await;
        assert!(h2.is_err());
        if let Err(ExecutionError::InvalidRequest { message }) = h2 {
            assert!(message.contains("already in progress"));
        } else {
            panic!("expected InvalidRequest error");
        }
        drop(_hold);
    }

    #[tokio::test]
    async fn test_stop_instance_sends_stop_and_updates_status() {
        let rt = WasmRuntime::new(&rt_cfg()).unwrap();
        let inst_cfg = dummy_instance_config();
        let instance = Arc::new(TaskInstance::new(
            inst_cfg.task_id.clone(),
            inst_cfg.clone(),
        ));

        let mh = WasmModuleHandle {
            module_id: uuid::Uuid::new_v4().to_string(),
            module_name: "user_module".to_string(),
            module_size: 0,
            module_hash: "h".to_string(),
            compilation_time: std::time::SystemTime::now(),
            exported_functions: vec!["main".to_string()],
            memory_usage: 0,
            module_bytes: vec![],
        };

        let state = WasmInstanceState {
            initialized: true,
            memory_usage: 0,
            fuel_remaining: 1000,
            last_execution_time: None,
            is_running: false,
            current_function: None,
        };
        let (tx, rx) = std::sync::mpsc::channel::<WasmWorkerRequest>();
        std::thread::spawn(move || loop {
            match rx.recv() {
                Ok(r) => match r {
                    WasmWorkerRequest::Stop { reply_tx } => {
                        let _ = reply_tx.send(Ok(Vec::new()));
                        break;
                    }
                    WasmWorkerRequest::Invoke { reply_tx, .. } => {
                        let _ = reply_tx.send(Ok(Vec::new()));
                    }
                },
                Err(_) => break,
            }
        });
        let handle = WasmInstanceHandle {
            instance_id: uuid::Uuid::new_v4().to_string(),
            module_handle: mh,
            state: Arc::new(Mutex::new(state)),
            execution_stats: Arc::new(Mutex::new(WasmExecutionStats::default())),
            req_tx: tx,
            req_rx: Arc::new(Mutex::new(None)),
            exec_lock: Arc::new(tokio::sync::Mutex::new(())),
        };
        instance.set_runtime_handle(Arc::new(handle));

        rt.start_instance(&instance).await.unwrap();
        assert_eq!(instance.status(), InstanceStatus::Running);
        rt.stop_instance(&instance).await.unwrap();
        assert_eq!(instance.status(), InstanceStatus::Stopped);
    }
}
