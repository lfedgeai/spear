use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
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

struct Inner {
    procs: HashMap<String, ManagedProc>,
}

struct ManagedProc {
    spec_key: String,
    child: Child,
    backend: BackendInfo,
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

        let ready_probe = params
            .get("ready_probe")
            .map(|s| s.trim().to_ascii_lowercase())
            .unwrap_or_else(|| "http".to_string());

        let server_mode = params
            .get("server_mode")
            .map(|s| s.trim().to_ascii_lowercase())
            .unwrap_or_else(|| "llama".to_string());

        let server_cmd = params
            .get("server_cmd")
            .cloned()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "llama-server".to_string());

        let mut cmd = Command::new(server_cmd);
        cmd.kill_on_drop(true);
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());

        if server_mode == "raw" {
            let raw_args = params.get("server_cmd_args").cloned().unwrap_or_default();
            let args = split_args(&raw_args);
            if args.is_empty() {
                return Err("server_mode=raw requires server_cmd_args".to_string());
            }
            cmd.args(args);
        } else {
            let model_path = resolve_model_path(&self.local_models_dir, model, params)?;
            if !model_path.exists() {
                download_model(http, &model_path, params).await?;
            }

            cmd.arg("-m").arg(model_path);
            cmd.arg("--host").arg("127.0.0.1");
            cmd.arg("--port").arg(port.to_string());

            if let Some(n_threads) = params
                .get("threads")
                .cloned()
                .filter(|v| !v.trim().is_empty())
            {
                cmd.arg("--threads").arg(n_threads);
            }
            if let Some(ctx) = params
                .get("ctx_size")
                .cloned()
                .filter(|v| !v.trim().is_empty())
            {
                cmd.arg("--ctx-size").arg(ctx);
            }
        }

        let child = cmd.spawn().map_err(|e| {
            format!(
                "failed to spawn llama server command (hint: install llama-server in spearlet image, or set params.server_cmd / server_mode=raw): {}",
                e
            )
        })?;

        if ready_probe != "none" {
            let start_timeout_s = params
                .get("start_timeout_s")
                .and_then(|s| s.trim().parse::<u64>().ok())
                .unwrap_or(120);
            wait_ready(http, &base_url, Duration::from_secs(start_timeout_s)).await?;
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
    let file = if let Some(model_url) = params
        .get("model_url")
        .cloned()
        .filter(|s| !s.trim().is_empty())
    {
        let name = Url::parse(&model_url)
            .ok()
            .and_then(|u| {
                u.path_segments()
                    .and_then(|mut s| s.next_back().map(|x| x.to_string()))
            })
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| sanitize_component(model));
        let s = sanitize_component(&name);
        if s.to_ascii_lowercase().ends_with(".gguf") {
            s
        } else {
            format!("{}.gguf", s)
        }
    } else {
        format!("{}.gguf", sanitize_component(model))
    };
    Ok(dir.join(file))
}

#[cfg(test)]
mod tests {
    use super::*;

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

async fn download_model(
    http: &Client,
    model_path: &Path,
    params: &HashMap<String, String>,
) -> Result<(), String> {
    if params
        .get("skip_download")
        .map(|v| v.trim() == "1")
        .unwrap_or(false)
    {
        return Err("model file missing and skip_download=1".to_string());
    }

    let parent = model_path
        .parent()
        .ok_or_else(|| "invalid model_path".to_string())?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|e| e.to_string())?;

    if let Some(model_url) = params
        .get("model_url")
        .cloned()
        .filter(|s| !s.trim().is_empty())
    {
        let timeout_s = params
            .get("download_timeout_s")
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(3600);
        return download_model_from_url(http, &model_url, model_path, timeout_s).await;
    }
    Err(
        "model file missing; set params.model_url (http/https .gguf) or params.model_path"
            .to_string(),
    )
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
