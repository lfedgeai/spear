//! SPEARlet configuration / SPEARlet配置

use clap::Parser;
use serde::{Deserialize, Serialize};

use crate::config::base::{LogConfig, ServerConfig};

/// SPEARlet command line arguments / SPEARlet命令行参数
#[derive(Parser, Debug, Clone)]
#[command(
    name = "spearlet",
    version = "0.1.0",
    about = "SPEARlet - SPEAR core agent component\nSPEARlet - SPEAR核心代理组件",
    long_about = "SPEARlet is the core agent component of SPEAR project, similar to kubelet in Kubernetes.\nSPEARlet是SPEAR项目的核心代理组件，类似于Kubernetes中的kubelet。"
)]
pub struct CliArgs {
    /// Configuration file path / 配置文件路径
    #[arg(
        short,
        long,
        value_name = "FILE",
        help = "Configuration file path / 配置文件路径"
    )]
    pub config: Option<String>,

    /// Node name / 节点名称
    #[arg(
        long,
        value_name = "NAME",
        help = "Node name (not unique) / 节点名称（可重复）"
    )]
    pub node_name: Option<String>,

    /// gRPC server address / gRPC服务器地址
    #[arg(
        long,
        value_name = "ADDR",
        help = "gRPC server address (e.g., 0.0.0.0:50052) / gRPC服务器地址"
    )]
    pub grpc_addr: Option<String>,

    /// HTTP gateway address / HTTP网关地址
    #[arg(
        long,
        value_name = "ADDR",
        help = "HTTP gateway address (e.g., 0.0.0.0:8081) / HTTP网关地址"
    )]
    pub http_addr: Option<String>,

    /// SMS service address / SMS服务地址
    #[arg(
        long = "sms-grpc-addr",
        value_name = "ADDR",
        help = "SMS gRPC address (e.g., 127.0.0.1:50051) / SMS gRPC地址"
    )]
    pub sms_grpc_addr: Option<String>,

    /// SMS HTTP gateway address / SMS HTTP网关地址
    #[arg(
        long,
        value_name = "ADDR",
        help = "SMS HTTP gateway address (e.g., 127.0.0.1:8080) / SMS HTTP网关地址"
    )]
    pub sms_http_addr: Option<String>,

    /// Storage backend type / 存储后端类型
    #[arg(
        long,
        value_name = "BACKEND",
        help = "Storage backend type (memory, sled, rocksdb) / 存储后端类型"
    )]
    pub storage_backend: Option<String>,

    /// Storage data directory / 存储数据目录
    #[arg(
        long,
        value_name = "PATH",
        help = "Storage data directory / 存储数据目录"
    )]
    pub storage_path: Option<String>,

    /// Local models directory (download/cache) / 本地模型目录（下载/缓存）
    #[arg(
        long,
        value_name = "PATH",
        help = "Local models directory for downloads/caches / 本地模型下载与缓存目录"
    )]
    pub local_models_dir: Option<String>,

    /// Auto register with SMS / 自动向SMS注册
    #[arg(long, help = "Auto register with SMS / 自动向SMS注册")]
    pub auto_register: Option<bool>,

    /// Log level / 日志级别
    #[arg(
        long,
        value_name = "LEVEL",
        help = "Log level (trace, debug, info, warn, error) / 日志级别"
    )]
    pub log_level: Option<String>,

    #[arg(long, value_name = "FORMAT")]
    pub log_format: Option<String>,

    #[arg(long, value_name = "FILE")]
    pub log_file: Option<String>,

    #[arg(long, value_name = "MS")]
    pub sms_connect_timeout_ms: Option<u64>,

    #[arg(long, value_name = "MS")]
    pub sms_connect_retry_ms: Option<u64>,

    /// Total reconnect timeout after disconnection / 断线后的总重连超时（毫秒）
    #[arg(
        long,
        value_name = "MS",
        help = "Total reconnect timeout after disconnect / 断线后的总重连超时（毫秒）"
    )]
    pub reconnect_total_timeout_ms: Option<u64>,
}

/// Spearlet application configuration / Spearlet应用配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    /// SPEARlet service configuration / SPEARlet服务配置
    pub spearlet: SpearletConfig,
}

impl AppConfig {
    /// Load configuration with CLI arguments / 使用CLI参数加载配置
    pub fn load_with_cli(args: &CliArgs) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let mut config = AppConfig::default();

        // Apply environment variables (low priority) / 应用环境变量（较低优先级）
        // Prefix: SPEARLET_  例如：SPEARLET_GRPC_ADDR, SPEARLET_HTTP_ADDR
        if let Ok(v) = std::env::var("SPEARLET_NODE_NAME") {
            if !v.is_empty() {
                config.spearlet.node_name = v;
            }
        }
        if let Ok(v) = std::env::var("SPEARLET_SMS_GRPC_ADDR") {
            if !v.is_empty() {
                config.spearlet.sms_grpc_addr = v;
            }
        }
        if let Ok(v) = std::env::var("SPEARLET_SMS_HTTP_ADDR") {
            if !v.is_empty() {
                config.spearlet.sms_http_addr = v;
            }
        }
        if let Ok(v) = std::env::var("SPEARLET_AUTO_REGISTER") {
            if let Ok(b) = v.parse::<bool>() {
                config.spearlet.auto_register = b;
            }
        }
        if let Ok(v) = std::env::var("SPEARLET_HEARTBEAT_INTERVAL") {
            if let Ok(n) = v.parse::<u64>() {
                config.spearlet.heartbeat_interval = n;
            }
        }
        if let Ok(v) = std::env::var("SPEARLET_CLEANUP_INTERVAL") {
            if let Ok(n) = v.parse::<u64>() {
                config.spearlet.cleanup_interval = n;
            }
        }

        if let Ok(v) = std::env::var("SPEARLET_GRPC_ADDR") {
            if let Ok(a) = v.parse::<std::net::SocketAddr>() {
                config.spearlet.grpc.addr = a;
            }
        }
        if let Ok(v) = std::env::var("SPEARLET_GRPC_ENABLE_TLS") {
            if let Ok(b) = v.parse::<bool>() {
                config.spearlet.grpc.enable_tls = b;
            }
        }
        if let Ok(v) = std::env::var("SPEARLET_GRPC_TLS_CERT_PATH") {
            if !v.is_empty() {
                config.spearlet.grpc.cert_path = Some(v);
            }
        }
        if let Ok(v) = std::env::var("SPEARLET_GRPC_TLS_KEY_PATH") {
            if !v.is_empty() {
                config.spearlet.grpc.key_path = Some(v);
            }
        }
        if let Ok(v) = std::env::var("SPEARLET_HTTP_ADDR") {
            if let Ok(a) = v.parse::<std::net::SocketAddr>() {
                config.spearlet.http.server.addr = a;
            }
        }
        if let Ok(v) = std::env::var("SPEARLET_HTTP_CORS_ENABLED") {
            if let Ok(b) = v.parse::<bool>() {
                config.spearlet.http.cors_enabled = b;
            }
        }
        if let Ok(v) = std::env::var("SPEARLET_HTTP_SWAGGER_ENABLED") {
            if let Ok(b) = v.parse::<bool>() {
                config.spearlet.http.swagger_enabled = b;
            }
        }

        if let Ok(v) = std::env::var("SPEARLET_STORAGE_BACKEND") {
            if !v.is_empty() {
                config.spearlet.storage.backend = v;
            }
        }
        if let Ok(v) = std::env::var("SPEARLET_STORAGE_DATA_DIR") {
            if !v.is_empty() {
                config.spearlet.storage.data_dir = v;
            }
        }
        if let Ok(v) = std::env::var("SPEARLET_LOCAL_MODELS_DIR") {
            if !v.is_empty() {
                config.spearlet.local_models_dir = v;
            }
        }
        if let Ok(v) = std::env::var("SPEARLET_STORAGE_MAX_CACHE_MB") {
            if let Ok(n) = v.parse::<u64>() {
                config.spearlet.storage.max_cache_size_mb = n;
            }
        }
        if let Ok(v) = std::env::var("SPEARLET_STORAGE_COMPRESSION_ENABLED") {
            if let Ok(b) = v.parse::<bool>() {
                config.spearlet.storage.compression_enabled = b;
            }
        }
        if let Ok(v) = std::env::var("SPEARLET_STORAGE_MAX_OBJECT_SIZE") {
            if let Ok(n) = v.parse::<u64>() {
                config.spearlet.storage.max_object_size = n;
            }
        }

        if let Ok(v) = std::env::var("SPEARLET_LOG_LEVEL") {
            if !v.is_empty() {
                config.spearlet.logging.level = v;
            }
        }
        if let Ok(v) = std::env::var("SPEARLET_LOG_FORMAT") {
            if !v.is_empty() {
                config.spearlet.logging.format = v;
            }
        }
        if let Ok(v) = std::env::var("SPEARLET_LOG_FILE") {
            if !v.is_empty() {
                config.spearlet.logging.file = Some(v);
            }
        }

        if let Ok(v) = std::env::var("SPEARLET_SMS_CONNECT_TIMEOUT_MS") {
            if let Ok(n) = v.parse::<u64>() {
                config.spearlet.sms_connect_timeout_ms = n;
            }
        }
        if let Ok(v) = std::env::var("SPEARLET_SMS_CONNECT_RETRY_MS") {
            if let Ok(n) = v.parse::<u64>() {
                config.spearlet.sms_connect_retry_ms = n;
            }
        }
        if let Ok(v) = std::env::var("SPEARLET_RECONNECT_TOTAL_TIMEOUT_MS") {
            if let Ok(n) = v.parse::<u64>() {
                config.spearlet.reconnect_total_timeout_ms = n;
            }
        }

        let mut touch_router_filter_stream = false;
        if std::env::var("SPEARLET_AI_ROUTER_GRPC_FILTER_STREAM_ENABLED").is_ok()
            || std::env::var("SPEARLET_AI_ROUTER_GRPC_FILTER_STREAM_ADDR").is_ok()
            || std::env::var("SPEARLET_AI_ROUTER_GRPC_FILTER_STREAM_DECISION_TIMEOUT_MS").is_ok()
            || std::env::var("SPEARLET_AI_ROUTER_GRPC_FILTER_STREAM_FAIL_OPEN").is_ok()
            || std::env::var("SPEARLET_AI_ROUTER_GRPC_FILTER_STREAM_MAX_CANDIDATES_SENT").is_ok()
            || std::env::var("SPEARLET_AI_ROUTER_GRPC_FILTER_STREAM_MAX_DEBUG_KV").is_ok()
            || std::env::var("SPEARLET_AI_ROUTER_GRPC_FILTER_STREAM_MAX_INFLIGHT_TOTAL").is_ok()
            || std::env::var("SPEARLET_AI_ROUTER_GRPC_FILTER_STREAM_PER_AGENT_MAX_INFLIGHT")
                .is_ok()
            || std::env::var("SPEARLET_AI_ROUTER_GRPC_FILTER_STREAM_CONTENT_FETCH_ENABLED").is_ok()
            || std::env::var("SPEARLET_AI_ROUTER_GRPC_FILTER_STREAM_CONTENT_FETCH_MAX_BYTES")
                .is_ok()
        {
            touch_router_filter_stream = true;
        }
        if touch_router_filter_stream && config.spearlet.ai.router_grpc_filter_stream.is_none() {
            config.spearlet.ai.router_grpc_filter_stream =
                Some(RouterGrpcFilterStreamConfig::default());
        }
        if let Ok(v) = std::env::var("SPEARLET_AI_ROUTER_GRPC_FILTER_STREAM_ENABLED") {
            if let Ok(b) = v.parse::<bool>() {
                if let Some(cfg) = config.spearlet.ai.router_grpc_filter_stream.as_mut() {
                    cfg.enabled = b;
                }
            }
        }
        if let Ok(v) = std::env::var("SPEARLET_AI_ROUTER_GRPC_FILTER_STREAM_ADDR") {
            if !v.is_empty() {
                if let Some(cfg) = config.spearlet.ai.router_grpc_filter_stream.as_mut() {
                    cfg.addr = v;
                }
            }
        }
        if let Ok(v) = std::env::var("SPEARLET_AI_ROUTER_GRPC_FILTER_STREAM_DECISION_TIMEOUT_MS") {
            if let Ok(n) = v.parse::<u64>() {
                if let Some(cfg) = config.spearlet.ai.router_grpc_filter_stream.as_mut() {
                    cfg.decision_timeout_ms = n;
                }
            }
        }
        if let Ok(v) = std::env::var("SPEARLET_AI_ROUTER_GRPC_FILTER_STREAM_FAIL_OPEN") {
            if let Ok(b) = v.parse::<bool>() {
                if let Some(cfg) = config.spearlet.ai.router_grpc_filter_stream.as_mut() {
                    cfg.fail_open = b;
                }
            }
        }
        if let Ok(v) = std::env::var("SPEARLET_AI_ROUTER_GRPC_FILTER_STREAM_MAX_CANDIDATES_SENT") {
            if let Ok(n) = v.parse::<usize>() {
                if let Some(cfg) = config.spearlet.ai.router_grpc_filter_stream.as_mut() {
                    cfg.max_candidates_sent = n;
                }
            }
        }
        if let Ok(v) = std::env::var("SPEARLET_AI_ROUTER_GRPC_FILTER_STREAM_MAX_DEBUG_KV") {
            if let Ok(n) = v.parse::<usize>() {
                if let Some(cfg) = config.spearlet.ai.router_grpc_filter_stream.as_mut() {
                    cfg.max_debug_kv = n;
                }
            }
        }
        if let Ok(v) = std::env::var("SPEARLET_AI_ROUTER_GRPC_FILTER_STREAM_MAX_INFLIGHT_TOTAL") {
            if let Ok(n) = v.parse::<usize>() {
                if let Some(cfg) = config.spearlet.ai.router_grpc_filter_stream.as_mut() {
                    cfg.max_inflight_total = n;
                }
            }
        }
        if let Ok(v) = std::env::var("SPEARLET_AI_ROUTER_GRPC_FILTER_STREAM_PER_AGENT_MAX_INFLIGHT") {
            if let Ok(n) = v.parse::<usize>() {
                if let Some(cfg) = config.spearlet.ai.router_grpc_filter_stream.as_mut() {
                    cfg.per_agent_max_inflight = n;
                }
            }
        }
        if let Ok(v) = std::env::var("SPEARLET_AI_ROUTER_GRPC_FILTER_STREAM_CONTENT_FETCH_ENABLED") {
            if let Ok(b) = v.parse::<bool>() {
                if let Some(cfg) = config.spearlet.ai.router_grpc_filter_stream.as_mut() {
                    cfg.content_fetch_enabled = b;
                }
            }
        }
        if let Ok(v) = std::env::var("SPEARLET_AI_ROUTER_GRPC_FILTER_STREAM_CONTENT_FETCH_MAX_BYTES") {
            if let Ok(n) = v.parse::<usize>() {
                if let Some(cfg) = config.spearlet.ai.router_grpc_filter_stream.as_mut() {
                    cfg.content_fetch_max_bytes = n;
                }
            }
        }
        if let Ok(v) = std::env::var("SPEARLET_AI_ENABLE_STUB_BACKEND") {
            if let Ok(b) = v.parse::<bool>() {
                config.spearlet.ai.enable_stub_backend = b;
            }
        }
        if let Ok(v) = std::env::var("SPEARLET_AI_REMOTE_BACKEND_SYNC_ENABLED") {
            if let Ok(b) = v.parse::<bool>() {
                config.spearlet.ai.remote_backend_sync.enabled = b;
            }
        }
        if let Ok(v) = std::env::var("SPEARLET_AI_REMOTE_BACKEND_SYNC_POLL_INTERVAL_MS") {
            if let Ok(n) = v.parse::<u64>() {
                config.spearlet.ai.remote_backend_sync.poll_interval_ms = n;
            }
        }
        if let Ok(v) = std::env::var("SPEARLET_AI_REMOTE_BACKEND_SYNC_MERGE_POLICY") {
            if !v.trim().is_empty() {
                config.spearlet.ai.remote_backend_sync.merge_policy = v;
            }
        }

        // Try loading from home directory first / 优先从用户主目录加载配置
        // Home path: ~/.spear/config.toml
        // 主目录路径：~/.spear/config.toml
        if args.config.is_none() {
            // Prefer SPEAR_HOME if set to avoid interfering with global HOME in tests
            // 若设置了SPEAR_HOME则优先使用，避免测试中修改全局HOME产生干扰
            let base_home = std::env::var_os("SPEAR_HOME").or_else(|| std::env::var_os("HOME"));
            if let Some(home_dir) = base_home {
                let home_path = std::path::PathBuf::from(home_dir)
                    .join(".spear")
                    .join("config.toml");
                if home_path.exists() {
                    match std::fs::read_to_string(&home_path) {
                        Ok(cfg) => match toml::from_str::<AppConfig>(&cfg) {
                            Ok(c) => {
                                config = c;
                            }
                            Err(e) => {
                                tracing::warn!("Failed to parse home config: {}", e);
                            }
                        },
                        Err(e) => {
                            tracing::warn!("Failed to read home config: {}", e);
                        }
                    }
                }
            }
        }

        // Load from CLI-provided path (highest file priority) / 从命令行提供的路径加载（文件最高优先级）
        if let Some(config_path) = &args.config {
            let config_content = std::fs::read_to_string(config_path)?;
            config = toml::from_str(&config_content)?;
        }

        // Override with CLI arguments / 使用CLI参数覆盖
        if let Some(node_name) = &args.node_name {
            config.spearlet.node_name = node_name.clone();
        }

        if let Some(grpc_addr) = &args.grpc_addr {
            config.spearlet.grpc.addr = grpc_addr.parse()?;
        }

        if let Some(http_addr) = &args.http_addr {
            config.spearlet.http.server.addr = http_addr.parse()?;
        }

        if let Some(sms_grpc_addr) = &args.sms_grpc_addr {
            config.spearlet.sms_grpc_addr = sms_grpc_addr.clone();
        }
        if let Some(sms_http_addr) = &args.sms_http_addr {
            config.spearlet.sms_http_addr = sms_http_addr.clone();
        }

        if let Some(storage_backend) = &args.storage_backend {
            config.spearlet.storage.backend = storage_backend.clone();
        }

        if let Some(storage_path) = &args.storage_path {
            config.spearlet.storage.data_dir = storage_path.clone();
        }

        if let Some(p) = &args.local_models_dir {
            config.spearlet.local_models_dir = p.clone();
        }

        if let Some(auto_register) = args.auto_register {
            config.spearlet.auto_register = auto_register;
        }

        if let Some(log_level) = &args.log_level {
            config.spearlet.logging.level = log_level.clone();
        }
        if let Some(log_format) = &args.log_format {
            config.spearlet.logging.format = log_format.clone();
        }
        if let Some(log_file) = &args.log_file {
            if !log_file.is_empty() {
                config.spearlet.logging.file = Some(log_file.clone());
            }
        }

        if let Some(t) = args.sms_connect_timeout_ms {
            config.spearlet.sms_connect_timeout_ms = t;
        }
        if let Some(r) = args.sms_connect_retry_ms {
            config.spearlet.sms_connect_retry_ms = r;
        }
        if let Some(rt) = args.reconnect_total_timeout_ms {
            config.spearlet.reconnect_total_timeout_ms = rt;
        }

        // Implicit auto-register rule: when SMS address is provided via CLI or env, enable auto_register by default
        // 隐式自动注册规则：当通过CLI或环境变量提供了SMS地址时，默认启用auto_register
        let env_sms = std::env::var("SPEARLET_SMS_GRPC_ADDR")
            .ok()
            .map(|v| !v.is_empty())
            .unwrap_or(false);
        let cli_sms = args
            .sms_grpc_addr
            .as_ref()
            .map(|v| !v.is_empty())
            .unwrap_or(false);
        if args.auto_register.is_none() && (env_sms || cli_sms) {
            config.spearlet.auto_register = true;
        }

        validate_spearlet_config(&config.spearlet)?;
        Ok(config)
    }
}

fn validate_spearlet_config(
    cfg: &SpearletConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    for b in cfg.ai.backends.iter() {
        let hosting = b.hosting.as_deref().map(|s| s.trim()).unwrap_or("");
        if hosting.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("ai backend hosting is required: {}", b.name),
            )
            .into());
        }
        let hosting = hosting.to_ascii_lowercase();
        if hosting != "local" && hosting != "remote" {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "invalid ai backend hosting (expected local|remote): {}",
                    b.name
                ),
            )
            .into());
        }
    }
    Ok(())
}

/// SPEARlet service configuration / SPEARlet服务配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SpearletConfig {
    /// Node name / 节点名称
    pub node_name: String,
    pub max_blocking_threads: usize,
    /// gRPC server configuration / gRPC服务器配置
    pub grpc: ServerConfig,
    /// HTTP gateway configuration / HTTP网关配置
    pub http: HttpConfig,
    /// Storage configuration / 存储配置
    pub storage: StorageConfig,
    /// Local models directory (download/cache) / 本地模型目录（下载/缓存）
    pub local_models_dir: String,
    /// Logging configuration / 日志配置
    pub logging: LogConfig,
    /// SMS service address / SMS服务地址
    pub sms_grpc_addr: String,
    /// SMS HTTP gateway address / SMS HTTP网关地址
    pub sms_http_addr: String,
    /// Auto register with SMS / 自动向SMS注册
    pub auto_register: bool,
    /// Heartbeat interval in seconds / 心跳间隔(秒)
    pub heartbeat_interval: u64,
    /// Cleanup interval in seconds / 清理间隔(秒)
    pub cleanup_interval: u64,
    pub sms_connect_timeout_ms: u64,
    pub sms_connect_retry_ms: u64,
    /// Total reconnect timeout after disconnection / 断线后的总重连超时（毫秒）
    pub reconnect_total_timeout_ms: u64,
    pub ai: AiConfig,
}

impl SpearletConfig {
    pub fn compute_node_uuid(&self) -> String {
        if let Ok(u) = uuid::Uuid::parse_str(&self.node_name) {
            return u.to_string();
        }
        let base = format!(
            "{}:{}:{}",
            self.grpc.addr.ip(),
            self.grpc.addr.port(),
            self.node_name
        );
        uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, base.as_bytes()).to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct AiConfig {
    pub default_policy: Option<String>,
    pub enable_stub_backend: bool,
    /// SMS remote backend sync configuration / SMS 远端 backend 同步配置
    pub remote_backend_sync: RemoteBackendSyncConfig,
    pub credentials: Vec<AiCredentialConfig>,
    pub backends: Vec<AiBackendConfig>,
    pub discovery: AiDiscoveryConfig,
    /// Router gRPC filter stream configuration / Router gRPC 过滤 stream 配置
    pub router_grpc_filter_stream: Option<RouterGrpcFilterStreamConfig>,
}

/// Remote backend sync configuration / 远端 backend 同步配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RemoteBackendSyncConfig {
    /// Enable syncing remote backends from SMS / 是否启用从 SMS 同步远端 backend
    pub enabled: bool,
    /// Poll interval in ms / 轮询间隔（毫秒）
    pub poll_interval_ms: u64,
    /// Merge policy when local and SMS backends have the same name
    /// / 当本地与 SMS 同名 backend 冲突时的合并策略
    pub merge_policy: String,
}

impl Default for RemoteBackendSyncConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            poll_interval_ms: 15_000,
            merge_policy: "sms_wins_by_name".to_string(),
        }
    }
}

/// Router gRPC filter stream configuration / Router gRPC 过滤 stream 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RouterGrpcFilterStreamConfig {
    /// Enable router filter stream / 启用 Router filter stream
    pub enabled: bool,
    /// Router filter server gRPC address (host:port); empty means using sms_grpc_addr.
    /// Router filter 服务端 gRPC 地址（host:port）；为空时表示使用 sms_grpc_addr。
    pub addr: String,
    /// Decision timeout budget in ms / 决策超时预算（毫秒）
    pub decision_timeout_ms: u64,
    /// Fail open when filter unavailable / filter 不可用时 fail-open
    pub fail_open: bool,
    /// Max candidates to send to agent / 发给 agent 的最大候选数
    pub max_candidates_sent: usize,
    /// Max debug kv entries accepted / 接受的最大 debug kv 数
    pub max_debug_kv: usize,
    /// Max total inflight across all agents / 所有 agent 的总 in-flight 上限
    pub max_inflight_total: usize,
    /// Max inflight per agent / 单个 agent 的 in-flight 上限
    pub per_agent_max_inflight: usize,

    /// Forward request payload snippet to filter server (size-bounded).
    /// 向 filter 服务端透传请求 payload 片段（有大小限制）。
    pub content_fetch_enabled: bool,
    pub content_fetch_max_bytes: usize,
}

impl Default for RouterGrpcFilterStreamConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            addr: String::new(),
            decision_timeout_ms: 2_000,
            fail_open: true,
            max_candidates_sent: 64,
            max_debug_kv: 32,
            max_inflight_total: 4096,
            per_agent_max_inflight: 512,
            content_fetch_enabled: false,
            content_fetch_max_bytes: 64 * 1024,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AiDiscoveryConfig {
    pub ollama: OllamaDiscoveryConfig,
}

impl Default for AiDiscoveryConfig {
    fn default() -> Self {
        Self {
            ollama: OllamaDiscoveryConfig::default(),
        }
    }
}


#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OllamaDiscoveryConfig {
    pub enabled: bool,
    pub scope: String,
    pub base_url: String,
    pub allow_remote: bool,
    pub timeout_ms: u64,
    pub max_models: usize,
    pub allow_models: Vec<String>,
    pub deny_models: Vec<String>,
    pub name_prefix: String,
    pub name_conflict: String,
    pub default_weight: u32,
    pub default_priority: i32,
    pub default_ops: Vec<String>,
    pub default_features: Vec<String>,
    pub default_transports: Vec<String>,
}

impl Default for OllamaDiscoveryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            scope: "serving".to_string(),
            base_url: "http://127.0.0.1:11434".to_string(),
            allow_remote: false,
            timeout_ms: 1500,
            max_models: 32,
            allow_models: Vec::new(),
            deny_models: Vec::new(),
            name_prefix: "ollama/".to_string(),
            name_conflict: "skip".to_string(),
            default_weight: 100,
            default_priority: 0,
            default_ops: vec!["chat_completions".to_string()],
            default_features: Vec::new(),
            default_transports: vec!["http".to_string()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AiCredentialConfig {
    pub name: String,
    pub kind: String,
    pub api_key_env: String,
}

impl Default for AiCredentialConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            kind: "env".to_string(),
            api_key_env: String::new(),
        }
    }
}


#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AiBackendConfig {
    pub name: String,
    pub kind: String,
    pub base_url: String,
    pub hosting: Option<String>,
    pub model: Option<String>,
    pub credential_ref: Option<String>,
    pub provider: Option<String>,
    pub origin: Option<String>,
    pub deployment_id: Option<String>,
    pub weight: u32,
    pub priority: i32,
    pub ops: Vec<String>,
    pub features: Vec<String>,
    pub transports: Vec<String>,
}

impl Default for AiBackendConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            kind: String::new(),
            base_url: String::new(),
            hosting: None,
            model: None,
            credential_ref: None,
            provider: None,
            origin: None,
            deployment_id: None,
            weight: 100,
            priority: 0,
            ops: Vec::new(),
            features: Vec::new(),
            transports: Vec::new(),
        }
    }
}


// gRPC server configuration / gRPC服务器配置
// gRPC uses base ServerConfig / gRPC使用基础ServerConfig

/// HTTP gateway configuration / HTTP网关配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HttpConfig {
    /// HTTP server settings / HTTP服务器设置
    pub server: ServerConfig,
    /// Enable CORS / 启用CORS
    pub cors_enabled: bool,
    /// Enable Swagger UI / 启用Swagger UI
    pub swagger_enabled: bool,
}

/// Storage configuration / 存储配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Storage backend type / 存储后端类型
    pub backend: String,
    /// Storage data directory / 存储数据目录
    pub data_dir: String,
    /// Maximum cache size in MB / 最大缓存大小(MB)
    pub max_cache_size_mb: u64,
    /// Enable compression / 启用压缩
    pub compression_enabled: bool,
    /// Maximum object size in bytes / 最大对象大小(字节)
    pub max_object_size: u64,
}

/// Logging configuration / 日志配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level / 日志级别
    pub level: String,
    /// Log format / 日志格式
    pub format: String,
    /// Log output file / 日志输出文件
    pub output_file: Option<String>,
}

impl Default for SpearletConfig {
    fn default() -> Self {
        Self {
            node_name: "spearlet-node".to_string(),
            max_blocking_threads: 512,
            grpc: ServerConfig {
                addr: "0.0.0.0:50052".parse().unwrap(),
                ..Default::default()
            },
            http: HttpConfig::default(),
            storage: StorageConfig::default(),
            local_models_dir: String::new(),
            logging: LogConfig::default(),
            sms_grpc_addr: "127.0.0.1:50051".to_string(),
            sms_http_addr: "127.0.0.1:8080".to_string(),
            auto_register: false,
            heartbeat_interval: 30,
            cleanup_interval: 300,
            sms_connect_timeout_ms: 15000,
            sms_connect_retry_ms: 500,
            reconnect_total_timeout_ms: 300_000,
            ai: AiConfig::default(),
        }
    }
}

// Grpc defaults provided via ServerConfig::default with override in SpearletConfig

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                addr: "0.0.0.0:8081".parse().unwrap(),
                ..Default::default()
            },
            cors_enabled: true,
            swagger_enabled: true,
        }
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            backend: "memory".to_string(), // Changed from rocksdb to memory / 从rocksdb改为memory
            data_dir: "./data/spearlet".to_string(),
            max_cache_size_mb: 512,
            compression_enabled: true,
            max_object_size: 64 * 1024 * 1024, // 64MB default / 默认64MB
        }
    }
}

// Logging uses base LogConfig / 日志使用基础LogConfig
