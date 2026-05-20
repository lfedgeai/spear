use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context};
use bollard::Docker;
use tokio::runtime::Runtime;

use crate::config;
use crate::config::Config;

fn parse_port_mapping(s: &str) -> anyhow::Result<(u16, u16)> {
    let s = s.trim();
    let parts: Vec<&str> = s
        .split(':')
        .map(|x| x.trim())
        .filter(|x| !x.is_empty())
        .collect();
    if parts.len() != 2 {
        return Err(anyhow!(
            "invalid port mapping (expected HOST:CONTAINER): {}",
            s
        ));
    }
    let host: u16 = parts[0]
        .parse()
        .with_context(|| format!("parse host port: {}", parts[0]))?;
    let container: u16 = parts[1]
        .parse()
        .with_context(|| format!("parse container port: {}", parts[1]))?;
    if host == 0 || container == 0 {
        return Err(anyhow!("port must not be 0: {}", s));
    }
    Ok((host, container))
}

fn docker_network_exists(name: &str) -> bool {
    Command::new("docker")
        .args(["network", "inspect", name])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn docker_network_ensure(name: &str) -> anyhow::Result<()> {
    if docker_network_exists(name) {
        return Ok(());
    }
    let status = Command::new("docker")
        .args(["network", "create", name])
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("docker network create")?;
    if !status.success() {
        return Err(anyhow!(
            "docker network create failed (rc={})",
            status.code().unwrap_or(1)
        ));
    }
    Ok(())
}

fn docker_network_remove(name: &str) {
    let _ = Command::new("docker")
        .args(["network", "rm", name])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn docker_remove_container(name: &str) {
    let _ = Command::new("docker")
        .args(["rm", "-f", name])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn docker_remove_image(image: &str) {
    let _ = Command::new("docker")
        .args(["image", "rm", "-f", image])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn docker_container_running(name: &str) -> anyhow::Result<bool> {
    let out = Command::new("docker")
        .args(["inspect", "-f", "{{.State.Running}}", name])
        .stdin(Stdio::null())
        .output()
        .context("docker inspect running")?;
    if !out.status.success() {
        return Ok(false);
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim() == "true")
}

pub(super) fn apply(cfg: &Config) -> anyhow::Result<()> {
    super::docker_check_daemon()?;
    super::ensure_tool("docker", &["version"])?;

    let repo_root = config::repo_root()?;
    let build_flags = {
        let mut flags = Vec::new();
        if cfg.build.pull_base {
            flags.push("--pull");
        }
        if cfg.build.no_cache {
            flags.push("--no-cache");
        }
        flags
    };

    let unified_repo = cfg.images.unified_repo.trim();
    let sms_image = if unified_repo.is_empty() {
        format!("{}:{}", cfg.images.sms_repo, cfg.images.tag)
    } else {
        format!("{}:{}", unified_repo, cfg.images.tag)
    };
    let spearlet_image = if unified_repo.is_empty() {
        format!("{}:{}", cfg.images.spearlet_repo, cfg.images.tag)
    } else {
        format!("{}:{}", unified_repo, cfg.images.tag)
    };

    if cfg.build.enabled {
        if unified_repo.is_empty() {
            let mut cmd = Command::new("docker");
            cmd.current_dir(&repo_root);
            cmd.args(["build"]);
            cmd.args(&build_flags);
            cmd.args(["-f", "deploy/docker/sms/Dockerfile"]);
            cmd.args([
                "--build-arg",
                &format!("DEBIAN_SUITE={}", cfg.build.debian_suite),
            ]);
            cmd.args(["-t", &sms_image]);
            cmd.arg(".");
            super::run_checked(&mut cmd, "docker build sms")?;

            let mut cmd = Command::new("docker");
            cmd.current_dir(&repo_root);
            cmd.args(["build"]);
            cmd.args(&build_flags);
            cmd.args(["-f", "deploy/docker/spearlet/Dockerfile"]);
            cmd.args([
                "--build-arg",
                &format!("DEBIAN_SUITE={}", cfg.build.debian_suite),
            ]);
            if cfg.components.spearlet_with_node {
                let target = if cfg.components.spearlet_with_llama_server {
                    "runtime_with_node_and_llama"
                } else {
                    "runtime_with_node"
                };
                cmd.args(["--target", target]);
            }
            cmd.args(["-t", &spearlet_image]);
            cmd.arg(".");
            super::run_checked(&mut cmd, "docker build spearlet")?;
        } else {
            let mut cmd = Command::new("docker");
            cmd.current_dir(&repo_root);
            cmd.args(["build"]);
            cmd.args(&build_flags);
            cmd.args(["-f", "deploy/docker/spear/Dockerfile"]);
            cmd.args([
                "--build-arg",
                &format!("DEBIAN_SUITE={}", cfg.build.debian_suite),
            ]);
            if cfg.components.spearlet_with_node {
                let target = if cfg.components.spearlet_with_llama_server {
                    "runtime_with_node_and_llama"
                } else {
                    "runtime_with_node"
                };
                cmd.args(["--target", target]);
            }
            cmd.args(["-t", &sms_image]);
            cmd.arg(".");
            super::run_checked(&mut cmd, "docker build spear (unified)")?;
        }
    }

    let rt = Runtime::new().context("create tokio runtime")?;
    rt.block_on(async {
        let docker = Docker::connect_with_local_defaults().context("connect docker daemon")?;
        super::docker_assert_image_exists(&docker, &sms_image).await?;
        super::docker_assert_image_exists(&docker, &spearlet_image).await?;
        Ok::<(), anyhow::Error>(())
    })?;

    let net = cfg.docker_local.network_name.trim();
    if net.is_empty() {
        return Err(anyhow!("docker_local.network_name must not be empty"));
    }
    docker_network_ensure(net)?;

    let sms_name = cfg.docker_local.sms_name.trim();
    let spearlet_name = cfg.docker_local.spearlet_name.trim();
    if sms_name.is_empty() {
        return Err(anyhow!("docker_local.sms_name must not be empty"));
    }
    if spearlet_name.is_empty() {
        return Err(anyhow!("docker_local.spearlet_name must not be empty"));
    }

    docker_remove_container(sms_name);
    docker_remove_container(spearlet_name);

    let (sms_http_host, sms_http_container) =
        parse_port_mapping(&cfg.docker_local.publish_sms_http)?;
    let sms_web_admin = if cfg.components.enable_web_admin {
        Some(parse_port_mapping(&cfg.docker_local.publish_sms_web_admin)?)
    } else {
        None
    };
    let (spear_http_host, spear_http_container) =
        parse_port_mapping(&cfg.docker_local.publish_spearlet_http)?;

    let (level, format) = if cfg.logging.debug {
        ("debug".to_string(), "pretty".to_string())
    } else {
        (
            cfg.logging.log_level.clone(),
            cfg.logging.log_format.clone(),
        )
    };

    let mut cmd = Command::new("docker");
    cmd.args(["run", "-d"]);
    cmd.args(["--name", sms_name]);
    cmd.args(["--network", net]);
    cmd.args(["-p", &format!("{sms_http_host}:{sms_http_container}")]);
    if let Some((host, container)) = sms_web_admin {
        cmd.args(["-p", &format!("{host}:{container}")]);
    }
    cmd.args(["-e", "SMS_GRPC_ADDR=0.0.0.0:50051"]);
    cmd.args(["-e", "SMS_HTTP_ADDR=0.0.0.0:8080"]);
    cmd.args(["-e", "SMS_WEB_ADMIN_ADDR=0.0.0.0:8081"]);
    cmd.args(["-e", "SMS_FILES_DIR=/tmp/sms-files"]);
    cmd.args(["-e", "SMS_EXECUTION_LOGS_DIR=/tmp/sms-execution-logs"]);
    cmd.args([
        "-e",
        &format!("SMS_ENABLE_WEB_ADMIN={}", cfg.components.enable_web_admin),
    ]);
    cmd.args(["-e", &format!("SMS_LOG_LEVEL={}", level)]);
    cmd.args(["-e", &format!("SMS_LOG_FORMAT={}", format)]);
    cmd.arg(&sms_image);
    cmd.arg("sms");
    super::run_checked(&mut cmd, "docker run sms")?;

    let mut cmd = Command::new("docker");
    cmd.args(["run", "-d"]);
    cmd.args(["--name", spearlet_name]);
    cmd.args(["--network", net]);
    cmd.args(["-p", &format!("{spear_http_host}:{spear_http_container}")]);
    cmd.args(["-e", "SPEARLET_GRPC_ADDR=0.0.0.0:50052"]);
    cmd.args(["-e", "SPEARLET_HTTP_ADDR=0.0.0.0:8081"]);
    cmd.args(["-e", &format!("SPEARLET_ADVERTISE_IP={}", spearlet_name)]);
    cmd.args(["-e", "SPEARLET_LOCAL_MODELS_DIR=/tmp/spearlet-local-models"]);
    cmd.args(["-e", "SPEARLET_STORAGE_DATA_DIR=/tmp/spearlet-storage"]);
    cmd.args(["-e", &format!("SPEARLET_SMS_GRPC_ADDR={sms_name}:50051")]);
    cmd.args(["-e", &format!("SPEARLET_SMS_HTTP_ADDR={sms_name}:8080")]);
    cmd.args(["-e", "SPEARLET_AUTO_REGISTER=true"]);
    cmd.args(["-e", &format!("SPEARLET_LOG_LEVEL={}", level)]);
    cmd.args(["-e", &format!("SPEARLET_LOG_FORMAT={}", format)]);
    if cfg.components.enable_router_filter {
        cmd.args(["-e", "SPEARLET_LLM_ROUTER_GRPC_FILTER_STREAM_ENABLED=true"]);
    }
    if cfg.components.enable_e2e {
        cmd.args(["-e", "SPEAR_E2E=1"]);
    }
    if cfg.secrets.openai.source == "from-env" {
        if let Ok(v) = std::env::var(&cfg.secrets.openai.env_name) {
            if !v.trim().is_empty() {
                cmd.args(["-e", &format!("OPENAI_API_KEY={}", v.trim())]);
            }
        }
    }
    cmd.arg(&spearlet_image);
    cmd.arg("spearlet");
    super::run_checked(&mut cmd, "docker run spearlet")?;

    let timeout = super::parse_timeout(&cfg.timeouts.rollout)?;
    let start = Instant::now();
    while start.elapsed() < timeout {
        if docker_container_running(sms_name).unwrap_or(false)
            && docker_container_running(spearlet_name).unwrap_or(false)
        {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(300));
    }

    Err(anyhow!(
        "timeout waiting docker containers to run (sms={}, spearlet={})",
        sms_name,
        spearlet_name
    ))
}

pub(super) fn status_text(cfg: &Config) -> anyhow::Result<String> {
    fn run_capture(mut cmd: Command, ctx: &str) -> anyhow::Result<String> {
        cmd.stdin(Stdio::null());
        let out = cmd.output().with_context(|| ctx.to_string())?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        if !out.status.success() {
            return Err(anyhow!(
                "{} failed (rc={})\n\nstdout:\n{}\n\nstderr:\n{}\n",
                ctx,
                out.status.code().unwrap_or(1),
                stdout,
                stderr
            ));
        }
        let mut s = String::new();
        if !stdout.trim().is_empty() {
            s.push_str(&stdout);
            if !stdout.ends_with('\n') {
                s.push('\n');
            }
        }
        if !stderr.trim().is_empty() {
            s.push_str(&stderr);
            if !stderr.ends_with('\n') {
                s.push('\n');
            }
        }
        Ok(s)
    }

    let mut out = String::new();
    out.push_str(&format!("mode: {}\n", cfg.mode.name));
    out.push_str(&format!("network: {}\n", cfg.docker_local.network_name));
    out.push_str(&format!("sms: {}\n", cfg.docker_local.sms_name));
    out.push_str(&format!("spearlet: {}\n\n", cfg.docker_local.spearlet_name));

    out.push_str("== docker ps (filtered) ==\n");
    let mut ps = Command::new("docker");
    ps.args(["ps", "-a"]);
    ps.args(["--filter", &format!("name={}", cfg.docker_local.sms_name)]);
    ps.args([
        "--filter",
        &format!("name={}", cfg.docker_local.spearlet_name),
    ]);
    out.push_str(&run_capture(ps, "docker ps")?);
    out.push('\n');

    out.push_str("== docker port sms ==\n");
    let mut port = Command::new("docker");
    port.args(["port", cfg.docker_local.sms_name.as_str()]);
    out.push_str(&run_capture(port, "docker port sms")?);
    out.push('\n');

    out.push_str("== docker port spearlet ==\n");
    let mut port = Command::new("docker");
    port.args(["port", cfg.docker_local.spearlet_name.as_str()]);
    out.push_str(&run_capture(port, "docker port spearlet")?);

    if let Ok((host, _)) = parse_port_mapping(&cfg.docker_local.publish_sms_http) {
        out.push('\n');
        out.push_str("Hints:\n");
        out.push_str(&format!(
            "  - sms health: http://127.0.0.1:{}/health\n",
            host
        ));
        out.push_str(&format!(
            "  - sms swagger: http://127.0.0.1:{}/swagger-ui/\n",
            host
        ));
        if cfg.components.enable_web_admin {
            if let Ok((admin_host, _)) = parse_port_mapping(&cfg.docker_local.publish_sms_web_admin)
            {
                out.push_str(&format!(
                    "  - sms web admin: http://127.0.0.1:{}/\n",
                    admin_host
                ));
            }
        }
    }

    Ok(out)
}

pub(super) fn cleanup(cfg: &Config, scope: &[&str], yes: bool) -> anyhow::Result<()> {
    let net = cfg.docker_local.network_name.trim();
    let sms_name = cfg.docker_local.sms_name.trim();
    let spearlet_name = cfg.docker_local.spearlet_name.trim();

    let mut remove_containers = false;
    let mut remove_network = false;
    let mut remove_images = false;
    for s in scope {
        match *s {
            "release" | "secret" | "namespace" => {
                remove_containers = true;
            }
            "kind" => {
                remove_network = true;
            }
            "images" => {
                remove_images = true;
            }
            other => return Err(anyhow!("unknown scope item: {}", other)),
        }
    }
    if scope.is_empty() {
        remove_containers = true;
    }

    if (remove_network || remove_images) && !yes {
        return Err(anyhow!("cleanup network/images requires --yes"));
    }

    if remove_containers {
        docker_remove_container(sms_name);
        docker_remove_container(spearlet_name);
    }
    if remove_network && !net.is_empty() {
        docker_network_remove(net);
    }
    if remove_images {
        let unified_repo = cfg.images.unified_repo.trim();
        let sms_image = if unified_repo.is_empty() {
            format!("{}:{}", cfg.images.sms_repo, cfg.images.tag)
        } else {
            format!("{}:{}", unified_repo, cfg.images.tag)
        };
        let spearlet_image = if unified_repo.is_empty() {
            format!("{}:{}", cfg.images.spearlet_repo, cfg.images.tag)
        } else {
            format!("{}:{}", unified_repo, cfg.images.tag)
        };
        docker_remove_image(&sms_image);
        docker_remove_image(&spearlet_image);
    }
    Ok(())
}

pub(super) fn render_plan(cfg: &Config) -> anyhow::Result<String> {
    let repo_root = config::repo_root()?;
    let unified_repo = cfg.images.unified_repo.trim();
    let sms_image = if unified_repo.is_empty() {
        format!("{}:{}", cfg.images.sms_repo, cfg.images.tag)
    } else {
        format!("{}:{}", unified_repo, cfg.images.tag)
    };
    let spearlet_image = if unified_repo.is_empty() {
        format!("{}:{}", cfg.images.spearlet_repo, cfg.images.tag)
    } else {
        format!("{}:{}", unified_repo, cfg.images.tag)
    };

    let mut out = String::new();
    out.push_str("# SPEAR quickstart plan\n\n");

    out.push_str("Config:\n");
    out.push_str(&format!("  mode.name: {}\n", cfg.mode.name));
    out.push_str(&format!(
        "  docker_local.network_name: {}\n",
        cfg.docker_local.network_name
    ));
    out.push_str(&format!(
        "  docker_local.sms_name: {}\n",
        cfg.docker_local.sms_name
    ));
    out.push_str(&format!(
        "  docker_local.spearlet_name: {}\n",
        cfg.docker_local.spearlet_name
    ));
    out.push_str(&format!(
        "  docker_local.publish_sms_http: {}\n",
        cfg.docker_local.publish_sms_http
    ));
    out.push_str(&format!(
        "  docker_local.publish_sms_web_admin: {}\n",
        cfg.docker_local.publish_sms_web_admin
    ));
    out.push_str(&format!(
        "  docker_local.publish_spearlet_http: {}\n",
        cfg.docker_local.publish_spearlet_http
    ));
    out.push('\n');

    out.push_str("Phase A: Validate\n");
    out.push_str("  - Requires: docker daemon\n\n");

    out.push_str("Phase B: Prepare\n");
    out.push_str(&format!("  - repo_root: {}\n", repo_root.display()));
    out.push_str(&format!(
        "  - docker network ensure: {}\n\n",
        cfg.docker_local.network_name
    ));

    out.push_str("Phase C: Build Images\n");
    out.push_str(&format!("  - build.enabled: {}\n", cfg.build.enabled));
    out.push_str(&format!("  - sms image: {}\n", sms_image));
    out.push_str(&format!("  - spearlet image: {}\n", spearlet_image));
    out.push_str(&format!(
        "  - enable router filter: {}\n",
        cfg.components.enable_router_filter
    ));
    out.push('\n');

    out.push_str("Phase D: Run Containers (docker-local)\n");
    out.push_str("  - docker run sms + spearlet on the same network\n");
    Ok(out)
}
