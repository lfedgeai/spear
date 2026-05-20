use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context};
use serde::{Deserialize, Serialize};

pub const DEFAULT_CONFIG_TOML: &str = r#"version = 1

[mode]
name = "k8s-kind"

[paths]
workdir = "."
state_dir = ".tmp/spear-quickstart"

[build]
enabled = true
pull_base = true
no_cache = false
debian_suite = "trixie"

[images]
tag = "local"
unified_repo = ""
sms_repo = "spear-sms"
spearlet_repo = "spear-spearlet"

[components]
enable_web_admin = true
enable_router_filter = true
enable_e2e = false
spearlet_with_node = true
spearlet_with_llama_server = true

[logging]
debug = true
log_level = "info"
log_format = "json"

[timeouts]
rollout = "300s"

[k8s]
namespace = "spear"
release_name = "spear"
chart_path = "deploy/helm/spear"
values_files = ["deploy/helm/spear/values-openai.yaml"]

[k8s.port_forward]
enabled = false
auto_start = false
local_port = 18082
remote_port = 8081

[k8s.port_forward_console]
enabled = false
auto_start = false
local_port = 18080
remote_port = 8080

[k8s.kind]
cluster_name = "spear-openai"
reuse_cluster = false
keep_cluster = true
kubeconfig_file = ".tmp/kubeconfig-kind-spear-openai"

[k8s.existing]
kubeconfig = ""
context = ""

[secrets.openai]
source = "from-env"
env_name = "OPENAI_API_KEY"
k8s_secret_name = "openai-api-key"
k8s_secret_key = "OPENAI_API_KEY"

[docker_local]
network_name = "spear-quickstart"
sms_name = "spear-sms"
spearlet_name = "spear-spearlet"
publish_sms_http = "18080:8080"
publish_sms_web_admin = "18082:8081"
publish_spearlet_http = "18081:8081"
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub version: i64,
    pub mode: ModeConfig,
    pub paths: PathsConfig,
    pub build: BuildConfig,
    pub images: ImagesConfig,
    pub components: ComponentsConfig,
    pub logging: LoggingConfig,
    pub timeouts: TimeoutsConfig,
    pub k8s: K8sConfig,
    pub secrets: SecretsConfig,
    pub docker_local: DockerLocalConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModeConfig {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathsConfig {
    pub workdir: String,
    pub state_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildConfig {
    pub enabled: bool,
    pub pull_base: bool,
    pub no_cache: bool,
    pub debian_suite: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImagesConfig {
    pub tag: String,
    #[serde(default)]
    pub unified_repo: String,
    pub sms_repo: String,
    pub spearlet_repo: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentsConfig {
    pub enable_web_admin: bool,
    #[serde(default)]
    pub enable_router_filter: bool,
    pub enable_e2e: bool,
    pub spearlet_with_node: bool,
    pub spearlet_with_llama_server: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub debug: bool,
    pub log_level: String,
    pub log_format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutsConfig {
    pub rollout: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct K8sConfig {
    pub namespace: String,
    pub release_name: String,
    pub chart_path: String,
    pub values_files: Vec<String>,
    #[serde(default)]
    pub port_forward: K8sPortForwardConfig,
    #[serde(
        default = "default_k8s_port_forward_console",
        alias = "port_forward_assistant"
    )]
    pub port_forward_console: K8sPortForwardConfig,
    pub kind: K8sKindConfig,
    pub existing: K8sExistingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct K8sPortForwardConfig {
    pub enabled: bool,
    pub auto_start: bool,
    pub local_port: u16,
    pub remote_port: u16,
}

impl Default for K8sPortForwardConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            auto_start: false,
            local_port: 18082,
            remote_port: 8081,
        }
    }
}

fn default_k8s_port_forward_console() -> K8sPortForwardConfig {
    K8sPortForwardConfig {
        enabled: false,
        auto_start: false,
        local_port: 18080,
        remote_port: 8080,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct K8sKindConfig {
    pub cluster_name: String,
    pub reuse_cluster: bool,
    pub keep_cluster: bool,
    pub kubeconfig_file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct K8sExistingConfig {
    pub kubeconfig: String,
    pub context: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretsConfig {
    pub openai: OpenAISecretsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAISecretsConfig {
    pub source: String,
    pub env_name: String,
    pub k8s_secret_name: String,
    pub k8s_secret_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerLocalConfig {
    pub network_name: String,
    pub sms_name: String,
    pub spearlet_name: String,
    pub publish_sms_http: String,
    #[serde(default = "default_publish_sms_web_admin")]
    pub publish_sms_web_admin: String,
    pub publish_spearlet_http: String,
}

fn default_publish_sms_web_admin() -> String {
    "18082:8081".to_string()
}

pub fn ensure_default_config(path: &Path, force: bool) -> anyhow::Result<()> {
    if path.exists() && !force {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create dir {}", parent.display()))?;
    }
    fs::write(path, DEFAULT_CONFIG_TOML).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

pub fn load_config(path: &Path) -> anyhow::Result<Config> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let cfg: Config =
        toml::from_str(&text).with_context(|| format!("parse toml {}", path.display()))?;
    validate(&cfg)?;
    Ok(cfg)
}

pub fn save_config(path: &Path, cfg: &Config) -> anyhow::Result<()> {
    validate(cfg)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create dir {}", parent.display()))?;
    }
    let text = toml::to_string_pretty(cfg).context("serialize toml")?;
    fs::write(path, text).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

pub fn repo_root() -> anyhow::Result<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            anyhow!(
                "failed to compute repo root from {}",
                manifest_dir.display()
            )
        })?;
    Ok(repo_root)
}

pub fn validate(cfg: &Config) -> anyhow::Result<()> {
    if cfg.version != 1 {
        return Err(anyhow!("unsupported config version: {}", cfg.version));
    }
    match cfg.mode.name.as_str() {
        "k8s-kind" | "k8s-existing" | "docker-local" => {}
        other => return Err(anyhow!("unsupported mode.name: {}", other)),
    }
    if cfg.k8s.namespace.trim().is_empty() {
        return Err(anyhow!("k8s.namespace must not be empty"));
    }
    if cfg.k8s.release_name.trim().is_empty() {
        return Err(anyhow!("k8s.release_name must not be empty"));
    }
    if cfg.k8s.port_forward.enabled {
        if cfg.k8s.port_forward.local_port == 0 {
            return Err(anyhow!("k8s.port_forward.local_port must not be 0"));
        }
        if cfg.k8s.port_forward.remote_port == 0 {
            return Err(anyhow!("k8s.port_forward.remote_port must not be 0"));
        }
    }
    if cfg.k8s.port_forward_console.enabled {
        if cfg.k8s.port_forward_console.local_port == 0 {
            return Err(anyhow!(
                "k8s.port_forward_console.local_port must not be 0"
            ));
        }
        if cfg.k8s.port_forward_console.remote_port == 0 {
            return Err(anyhow!(
                "k8s.port_forward_console.remote_port must not be 0"
            ));
        }
    }
    Ok(())
}
