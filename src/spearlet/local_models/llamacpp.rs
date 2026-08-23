use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use reqwest::header::RANGE;
use reqwest::Client;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::time::timeout;
use tracing::warn;
use url::Url;

use crate::proto::sms::{BackendInfo, BackendStatus};
use crate::spearlet::ai::backend_assembly::{backend_spec_from_parts, BackendSpecParts};
use crate::spearlet::config::SpearletConfig;
use crate::spearlet::execution::ai::router::registry::Hosting;
use crate::spearlet::local_models::DEFAULT_LOCAL_MODELS_DIR;

#[derive(Clone)]
pub struct LlamaCppSupervisor {
    inner: Arc<Mutex<Inner>>,
    local_models_dir: String,
}

/// Local model source preflight snapshot / 本地模型来源预检快照
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelSourcePreflight {
    pub source_kind: String,
    pub effective_model_path: String,
    pub model_path_exists: bool,
    pub final_url: Option<String>,
    pub http_status: Option<u16>,
    pub content_length: Option<u64>,
    pub message: String,
}

struct Inner {
    procs: HashMap<String, ManagedProc>,
}

struct ManagedProc {
    spec_key: String,
    child: Child,
    backend: BackendInfo,
}

/// llama.cpp readiness probe mode used during process startup.
/// llama.cpp 进程启动阶段使用的就绪探测模式。
#[derive(Debug, Clone, PartialEq, Eq)]
enum LlamaCppReadyProbe {
    /// Probe the HTTP API until it becomes reachable.
    /// 轮询 HTTP API，直到端点可访问。
    Http,
    /// Skip readiness probing entirely.
    /// 完全跳过就绪探测。
    None,
}

/// Typed llama.cpp launch mode derived from backend metadata.
/// 从 backend metadata 派生的强类型 llama.cpp 启动模式。
#[derive(Debug, Clone, PartialEq, Eq)]
enum LlamaCppLaunchMode {
    /// Managed llama-server launch with structured runtime options.
    /// 使用结构化运行参数启动托管 llama-server。
    Managed {
        threads: Option<String>,
        ctx_size: Option<String>,
    },
    /// Raw process launch with fully provided command arguments.
    /// 使用完整命令参数启动原始进程。
    Raw { server_cmd_args: Vec<String> },
}

/// Typed llama.cpp launch options derived from backend metadata.
/// 从 backend metadata 派生的强类型 llama.cpp 启动选项。
#[derive(Debug, Clone, PartialEq, Eq)]
struct LlamaCppLaunchOptions {
    server_cmd: String,
    ready_probe: LlamaCppReadyProbe,
    start_timeout_s: u64,
    mode: LlamaCppLaunchMode,
}

/// Typed llama.cpp model-source configuration derived from backend metadata.
/// 从 backend metadata 派生的强类型 llama.cpp 模型来源配置。
#[derive(Debug, Clone, PartialEq, Eq)]
struct LlamaCppModelSource {
    model_path: PathBuf,
    model_url: Option<String>,
    skip_download: bool,
    download_timeout_s: Option<u64>,
}

impl LlamaCppSupervisor {
    pub fn new(cfg: &SpearletConfig) -> Self {
        let local_models_dir = if cfg.local_models_dir.trim().is_empty() {
            DEFAULT_LOCAL_MODELS_DIR.to_string()
        } else {
            cfg.local_models_dir.clone()
        };
        Self {
            inner: Arc::new(Mutex::new(Inner {
                procs: HashMap::new(),
            })),
            local_models_dir,
        }
    }

    pub async fn stop_removed(&self, live_ids: &HashSet<String>) {
        let mut inner = self.inner.lock().await;
        let to_stop: Vec<String> = inner
            .procs
            .keys()
            .filter(|id| !live_ids.contains(*id))
            .cloned()
            .collect();
        for id in to_stop {
            if let Some(mut p) = inner.procs.remove(&id) {
                let _ = terminate_child(&mut p.child).await;
            }
        }
    }

    pub async fn stop_all(&self) {
        let mut inner = self.inner.lock().await;
        let keys: Vec<String> = inner.procs.keys().cloned().collect();
        for id in keys {
            if let Some(mut p) = inner.procs.remove(&id) {
                let _ = terminate_child(&mut p.child).await;
            }
        }
    }

    pub async fn get_backend(&self, deployment_id: &str) -> Option<BackendInfo> {
        let mut inner = self.inner.lock().await;
        let Some(p) = inner.procs.get_mut(deployment_id) else {
            return None;
        };
        if let Ok(Some(_)) = p.child.try_wait() {
            inner.procs.remove(deployment_id);
            return None;
        }
        Some(p.backend.clone())
    }

    pub async fn ensure_server(
        &self,
        http: &Client,
        deployment_id: &str,
        spec_key: &str,
        model: &str,
        params: &HashMap<String, String>,
    ) -> Result<BackendInfo, String> {
        {
            let mut inner = self.inner.lock().await;
            if let Some(p) = inner.procs.get_mut(deployment_id) {
                if p.spec_key == spec_key {
                    if let Ok(Some(_)) = p.child.try_wait() {
                        inner.procs.remove(deployment_id);
                    } else {
                        return Ok(p.backend.clone());
                    }
                } else {
                    let _ = terminate_child(&mut p.child).await;
                    inner.procs.remove(deployment_id);
                }
            }
        }

        let port = allocate_local_port().await.map_err(|e| e.to_string())?;
        let base_url = format!("http://127.0.0.1:{}/v1", port);
        let launch = parse_llamacpp_launch_options(params)?;

        let mut cmd = Command::new(&launch.server_cmd);
        cmd.kill_on_drop(true);
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());

        match &launch.mode {
            LlamaCppLaunchMode::Raw { server_cmd_args } => {
                cmd.args(server_cmd_args);
            }
            LlamaCppLaunchMode::Managed { threads, ctx_size } => {
                let model_source = resolve_model_source(&self.local_models_dir, model, params)?;
                if !model_source.model_path.exists() {
                    download_model(http, &model_source).await?;
                }

                cmd.arg("-m").arg(&model_source.model_path);
                cmd.arg("--host").arg("127.0.0.1");
                cmd.arg("--port").arg(port.to_string());

                if let Some(n_threads) = threads {
                    cmd.arg("--threads").arg(n_threads);
                }
                if let Some(ctx) = ctx_size {
                    cmd.arg("--ctx-size").arg(ctx);
                }
            }
        }

        let child = cmd.spawn().map_err(|e| {
            format!(
                "failed to spawn llama server command (hint: install llama-server in spearlet image, or set params.server_cmd / server_mode=raw): {}",
                e
            )
        })?;

        if launch.ready_probe != LlamaCppReadyProbe::None {
            wait_ready(http, &base_url, Duration::from_secs(launch.start_timeout_s)).await?;
        }

        let backend = BackendInfo {
            spec: Some(backend_spec_from_parts(BackendSpecParts {
                name: format!("managed/llamacpp/{}", sanitize_name(model)),
                kind: "openai_chat_completion".to_string(),
                operations: vec!["chat_completions".to_string()],
                features: Vec::new(),
                transports: vec!["http".to_string()],
                weight: 100,
                priority: 0,
                base_url: base_url.clone(),
                provider: Some("llamacpp".to_string()),
                model: Some(model.to_string()),
                hosting: Hosting::Local,
                credential_ref: None,
                origin: crate::proto::sms::BackendOrigin::LocalController,
                deployment_id: Some(deployment_id.to_string()),
            })),
            status: BackendStatus::Available as i32,
            status_reason: String::new(),
        };

        let mut inner = self.inner.lock().await;
        inner.procs.insert(
            deployment_id.to_string(),
            ManagedProc {
                spec_key: spec_key.to_string(),
                child,
                backend: backend.clone(),
            },
        );

        Ok(backend)
    }
}

async fn allocate_local_port() -> std::io::Result<u16> {
    let l = TcpListener::bind(("127.0.0.1", 0)).await?;
    Ok(l.local_addr()?.port())
}

fn sanitize_name(model: &str) -> String {
    model
        .trim()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
}

fn split_args(s: &str) -> Vec<String> {
    s.split_whitespace().map(|x| x.to_string()).collect()
}

fn parse_llamacpp_launch_options(
    params: &HashMap<String, String>,
) -> Result<LlamaCppLaunchOptions, String> {
    let ready_probe = match params
        .get("ready_probe")
        .map(|s| s.trim().to_ascii_lowercase())
        .unwrap_or_else(|| "http".to_string())
        .as_str()
    {
        "none" => LlamaCppReadyProbe::None,
        _ => LlamaCppReadyProbe::Http,
    };
    let server_cmd = params
        .get("server_cmd")
        .cloned()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "llama-server".to_string());
    let start_timeout_s = params
        .get("start_timeout_s")
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(120);

    let mode = match params
        .get("server_mode")
        .map(|s| s.trim().to_ascii_lowercase())
        .unwrap_or_else(|| "llama".to_string())
        .as_str()
    {
        "raw" => {
            let raw_args = params.get("server_cmd_args").cloned().unwrap_or_default();
            let server_cmd_args = split_args(&raw_args);
            if server_cmd_args.is_empty() {
                return Err("server_mode=raw requires server_cmd_args".to_string());
            }
            LlamaCppLaunchMode::Raw { server_cmd_args }
        }
        _ => LlamaCppLaunchMode::Managed {
            threads: params
                .get("threads")
                .cloned()
                .filter(|value| !value.trim().is_empty()),
            ctx_size: params
                .get("ctx_size")
                .cloned()
                .filter(|value| !value.trim().is_empty()),
        },
    };

    Ok(LlamaCppLaunchOptions {
        server_cmd,
        ready_probe,
        start_timeout_s,
        mode,
    })
}

fn resolve_model_path(
    local_models_dir: &str,
    model: &str,
    params: &HashMap<String, String>,
) -> Result<PathBuf, String> {
    if let Some(p) = params
        .get("model_path")
        .cloned()
        .filter(|v| !v.trim().is_empty())
    {
        let pb = PathBuf::from(p);
        if pb.is_absolute() {
            return Ok(pb);
        }
        return Ok(Path::new(local_models_dir).join(pb));
    }

    let root = Path::new(local_models_dir).join("llamacpp");
    let dir = root.join("models");
    std::fs::create_dir_all(&dir).map_err(|e| {
        format!(
            "failed to create llamacpp model dir {}: {}",
            dir.display(),
            e
        )
    })?;
    let file = match params
        .get("model_url")
        .cloned()
        .filter(|s| !s.trim().is_empty())
    {
        Some(model_url) => {
            let name = Url::parse(&model_url)
                .ok()
                .and_then(|u| {
                    u.path_segments()
                        .and_then(|mut s| s.next_back().map(|x| x.to_string()))
                })
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| sanitize_component(model));
            let mut file = sanitize_component(&name);
            if !file.to_ascii_lowercase().ends_with(".gguf") {
                file.push_str(".gguf");
            }
            file
        }
        None => format!("{}.gguf", sanitize_component(model)),
    };
    Ok(dir.join(file))
}

fn resolve_model_source(
    local_models_dir: &str,
    model: &str,
    params: &HashMap<String, String>,
) -> Result<LlamaCppModelSource, String> {
    Ok(LlamaCppModelSource {
        model_path: resolve_model_path(local_models_dir, model, params)?,
        model_url: params
            .get("model_url")
            .cloned()
            .filter(|value| !value.trim().is_empty()),
        skip_download: params
            .get("skip_download")
            .map(|value| parse_truthy(value))
            .unwrap_or(false),
        download_timeout_s: params
            .get("download_timeout_s")
            .and_then(|value| value.trim().parse::<u64>().ok()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::get, Router};
    use tokio::net::TcpListener;

    #[test]
    fn resolve_model_path_errors_when_data_dir_is_not_a_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().join("not_a_dir");
        std::fs::write(&data_dir, b"x").unwrap();

        let params = HashMap::<String, String>::new();
        let err = resolve_model_path(
            data_dir.to_string_lossy().as_ref(),
            "qwen2.5-1b-instruct-q4_k_m",
            &params,
        )
        .unwrap_err();
        assert!(err.contains("failed to create llamacpp model dir"));
    }

    #[tokio::test]
    async fn preflight_model_source_accepts_existing_model_path() {
        let tmp = tempfile::tempdir().unwrap();
        let model_path = tmp.path().join("models").join("qwen.gguf");
        std::fs::create_dir_all(model_path.parent().unwrap()).unwrap();
        std::fs::write(&model_path, b"ok").unwrap();

        let client = Client::builder().build().unwrap();
        let mut params = HashMap::new();
        params.insert(
            "model_path".to_string(),
            model_path.to_string_lossy().to_string(),
        );

        let report = preflight_model_source(
            &client,
            tmp.path().to_string_lossy().as_ref(),
            "qwen",
            &params,
        )
        .await
        .unwrap();
        assert_eq!(report.source_kind, "model_path");
        assert!(report.model_path_exists);
        assert_eq!(report.final_url, None);
    }

    #[tokio::test]
    async fn preflight_model_source_verifies_model_url() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new().route(
            "/model.gguf",
            get(|| async { ([("content-length", "16")], "0123456789abcdef") }),
        );
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let client = Client::builder().build().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let mut params = HashMap::new();
        params.insert(
            "model_url".to_string(),
            format!("http://{}/model.gguf", addr),
        );

        let report = preflight_model_source(
            &client,
            tmp.path().to_string_lossy().as_ref(),
            "qwen",
            &params,
        )
        .await
        .unwrap();
        assert_eq!(report.source_kind, "model_url");
        assert!(!report.model_path_exists);
        assert_eq!(report.http_status, Some(200));
        assert!(report.final_url.unwrap().contains("/model.gguf"));

        server.abort();
    }

    #[tokio::test]
    async fn preflight_model_source_rejects_missing_file_when_skip_download_is_enabled() {
        let client = Client::builder().build().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let mut params = HashMap::new();
        params.insert(
            "model_path".to_string(),
            "/tmp/does-not-exist.gguf".to_string(),
        );
        params.insert("skip_download".to_string(), "1".to_string());

        let error = preflight_model_source(
            &client,
            tmp.path().to_string_lossy().as_ref(),
            "qwen",
            &params,
        )
        .await
        .unwrap_err();
        assert!(error.contains("skip_download=1"));
    }

    #[test]
    fn parse_llamacpp_launch_options_requires_raw_args() {
        let mut params = HashMap::new();
        params.insert("server_mode".to_string(), "raw".to_string());

        let error = parse_llamacpp_launch_options(&params).unwrap_err();
        assert!(error.contains("server_mode=raw requires server_cmd_args"));
    }

    #[test]
    fn resolve_model_source_parses_truthy_skip_download() {
        let mut params = HashMap::new();
        params.insert("skip_download".to_string(), "true".to_string());

        let source = resolve_model_source("/tmp/models", "qwen", &params).expect("model source");
        assert!(source.skip_download);
    }
}

fn sanitize_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        let ok = ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.');
        if ok {
            out.push(ch);
        } else if ch == '/' {
            out.push('_');
        } else {
            out.push('_');
        }
    }
    while out.contains("__") {
        out = out.replace("__", "_");
    }
    out.trim_matches('_').to_string()
}

/// Preflight local model source resolution from the node-side runtime.
/// 在节点侧运行时执行本地模型来源预检。
pub async fn preflight_model_source(
    http: &Client,
    local_models_dir: &str,
    model: &str,
    params: &HashMap<String, String>,
) -> Result<ModelSourcePreflight, String> {
    let source = resolve_model_source(local_models_dir, model, params)?;
    if tokio::fs::metadata(&source.model_path).await.is_ok() {
        return Ok(ModelSourcePreflight {
            source_kind: "model_path".to_string(),
            effective_model_path: source.model_path.display().to_string(),
            model_path_exists: true,
            final_url: None,
            http_status: None,
            content_length: None,
            message: "model file already exists".to_string(),
        });
    }

    if source.skip_download {
        return Err("model file missing and skip_download=1".to_string());
    }

    let model_url = source.model_url.ok_or_else(|| {
        "model file missing; set params.model_url (http/https .gguf) or params.model_path"
            .to_string()
    })?;
    let timeout_s = source.download_timeout_s.unwrap_or(10).clamp(1, 30);
    let (final_url, http_status, content_length) =
        verify_model_url_access(http, &model_url, timeout_s).await?;

    Ok(ModelSourcePreflight {
        source_kind: "model_url".to_string(),
        effective_model_path: source.model_path.display().to_string(),
        model_path_exists: false,
        final_url: Some(final_url),
        http_status: Some(http_status),
        content_length,
        message: "model_url is reachable from the node".to_string(),
    })
}

/// Verify model_url reachability without downloading the full artifact.
/// 在不下载完整模型文件的情况下验证 model_url 可访问性。
async fn verify_model_url_access(
    http: &Client,
    model_url: &str,
    timeout_s: u64,
) -> Result<(String, u16, Option<u64>), String> {
    let url = Url::parse(model_url).map_err(|e| format!("invalid model_url: {}", e))?;
    match url.scheme() {
        "http" | "https" => {}
        scheme => return Err(format!("unsupported model_url scheme: {}", scheme)),
    }

    let head_attempt = async {
        let response = http
            .head(model_url)
            .send()
            .await
            .map_err(|e| format!("model_url preflight HEAD failed: {}", e))?;
        if response.status().is_success() {
            return Ok((
                response.url().to_string(),
                response.status().as_u16(),
                response.content_length(),
            ));
        }
        Err(format!(
            "model_url preflight HEAD failed: http_status={} url={}",
            response.status(),
            model_url
        ))
    };

    if let Ok(result) = timeout(Duration::from_secs(timeout_s), head_attempt).await {
        if let Ok(success) = result {
            return Ok(success);
        }
    }

    let range_attempt = async {
        let response = http
            .get(model_url)
            .header(RANGE, "bytes=0-0")
            .send()
            .await
            .map_err(|e| format!("model_url preflight GET failed: {}", e))?;
        if response.status().is_success() || response.status().as_u16() == 206 {
            return Ok((
                response.url().to_string(),
                response.status().as_u16(),
                response.content_length(),
            ));
        }
        Err(format!(
            "model_url preflight GET failed: http_status={} url={}",
            response.status(),
            model_url
        ))
    };

    match timeout(Duration::from_secs(timeout_s), range_attempt).await {
        Ok(result) => result,
        Err(_) => Err("model_url preflight timeout".to_string()),
    }
}

async fn download_model(http: &Client, source: &LlamaCppModelSource) -> Result<(), String> {
    if source.skip_download {
        return Err("model file missing and skip_download=1".to_string());
    }

    let parent = source
        .model_path
        .parent()
        .ok_or_else(|| "invalid model_path".to_string())?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|e| e.to_string())?;

    if let Some(model_url) = &source.model_url {
        let timeout_s = source.download_timeout_s.unwrap_or(3600);
        return download_model_from_url(http, model_url, &source.model_path, timeout_s).await;
    }
    Err(
        "model file missing; set params.model_url (http/https .gguf) or params.model_path"
            .to_string(),
    )
}

fn parse_truthy(value: &str) -> bool {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => true,
        _ => false,
    }
}

async fn download_model_from_url(
    http: &Client,
    model_url: &str,
    model_path: &Path,
    timeout_s: u64,
) -> Result<(), String> {
    let url = Url::parse(model_url).map_err(|e| format!("invalid model_url: {}", e))?;
    match url.scheme() {
        "http" | "https" => {}
        s => return Err(format!("unsupported model_url scheme: {}", s)),
    }

    let now_ms = chrono::Utc::now().timestamp_millis();
    let tmp_path = model_path.with_extension(format!("part-{}", now_ms));
    let parent = model_path
        .parent()
        .ok_or_else(|| "invalid model_path".to_string())?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|e| e.to_string())?;

    let fut = async {
        let resp = http
            .get(model_url)
            .send()
            .await
            .map_err(|e| format!("model_url download request failed: {}", e))?;
        if !resp.status().is_success() {
            return Err(format!(
                "model_url download failed: http_status={} url={}",
                resp.status(),
                model_url
            ));
        }

        let mut file = tokio::fs::File::create(&tmp_path)
            .await
            .map_err(|e| format!("failed to create file {}: {}", tmp_path.display(), e))?;
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let b = chunk.map_err(|e| format!("download stream error: {}", e))?;
            file.write_all(&b)
                .await
                .map_err(|e| format!("failed to write file {}: {}", tmp_path.display(), e))?;
        }
        file.flush()
            .await
            .map_err(|e| format!("failed to flush file {}: {}", tmp_path.display(), e))?;

        tokio::fs::rename(&tmp_path, model_path)
            .await
            .map_err(|e| {
                format!(
                    "failed to move downloaded file into place ({} -> {}): {}",
                    tmp_path.display(),
                    model_path.display(),
                    e
                )
            })?;

        Ok(())
    };

    match timeout(Duration::from_secs(timeout_s), fut).await {
        Ok(r) => r,
        Err(_) => Err("model_url download timeout".to_string()),
    }
}

async fn wait_ready(http: &Client, base_url: &str, total: Duration) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + total;
    let health = format!("{}/health", base_url.trim_end_matches('/'));
    let models = format!("{}/models", base_url.trim_end_matches('/'));

    loop {
        if tokio::time::Instant::now() > deadline {
            return Err("llama-server start timeout".to_string());
        }

        if let Ok(resp) = http.get(&health).send().await {
            if resp.status().is_success() {
                return Ok(());
            }
        }
        if let Ok(resp) = http.get(&models).send().await {
            if resp.status().is_success() {
                return Ok(());
            }
        }

        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

async fn terminate_child(child: &mut Child) -> Result<(), String> {
    let _ = child.kill().await;
    match timeout(Duration::from_secs(3), child.wait()).await {
        Ok(_) => Ok(()),
        Err(_) => {
            warn!("timeout waiting for llama server to exit");
            Ok(())
        }
    }
}
