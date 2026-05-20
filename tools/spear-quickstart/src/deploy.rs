use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context};
use bollard::errors::Error as BollardError;
use bollard::Docker;
use kube::api::{ListParams, Patch, PatchParams, PostParams};
use kube::config::{KubeConfigOptions, Kubeconfig};
use kube::{Api, Client};
use tokio::runtime::Runtime;

use crate::config;
use crate::config::Config;

#[path = "deploy_docker_local.rs"]
mod deploy_docker_local;

fn bool01(v: bool) -> &'static str {
    if v {
        "1"
    } else {
        "0"
    }
}

fn env_for_kind(cfg: &Config) -> anyhow::Result<HashMap<String, String>> {
    let repo_root = config::repo_root()?;
    let kubeconfig_file = repo_root.join(&cfg.k8s.kind.kubeconfig_file);

    let mut env = HashMap::new();
    env.insert(
        "CLUSTER_NAME".to_string(),
        cfg.k8s.kind.cluster_name.clone(),
    );
    env.insert(
        "REUSE_CLUSTER".to_string(),
        bool01(cfg.k8s.kind.reuse_cluster).to_string(),
    );
    env.insert(
        "KEEP_CLUSTER".to_string(),
        bool01(cfg.k8s.kind.keep_cluster).to_string(),
    );
    env.insert(
        "KUBECONFIG_FILE".to_string(),
        kubeconfig_file.to_string_lossy().to_string(),
    );
    env.insert("NAMESPACE".to_string(), cfg.k8s.namespace.clone());
    env.insert("RELEASE_NAME".to_string(), cfg.k8s.release_name.clone());
    if cfg.images.unified_repo.trim().is_empty() {
        env.insert("SMS_IMAGE_REPO".to_string(), cfg.images.sms_repo.clone());
        env.insert(
            "SPEARLET_IMAGE_REPO".to_string(),
            cfg.images.spearlet_repo.clone(),
        );
    } else {
        env.insert(
            "SPEAR_IMAGE_REPO".to_string(),
            cfg.images.unified_repo.trim().to_string(),
        );
        env.insert(
            "SMS_IMAGE_REPO".to_string(),
            cfg.images.unified_repo.trim().to_string(),
        );
        env.insert(
            "SPEARLET_IMAGE_REPO".to_string(),
            cfg.images.unified_repo.trim().to_string(),
        );
    }
    env.insert("IMAGE_TAG".to_string(), cfg.images.tag.clone());
    env.insert(
        "SPEARLET_WITH_NODE".to_string(),
        bool01(cfg.components.spearlet_with_node).to_string(),
    );
    env.insert(
        "SPEARLET_WITH_LLAMA_SERVER".to_string(),
        bool01(cfg.components.spearlet_with_llama_server).to_string(),
    );
    env.insert(
        "ENABLE_WEB_ADMIN".to_string(),
        bool01(cfg.components.enable_web_admin).to_string(),
    );
    env.insert(
        "ENABLE_ROUTER_FILTER".to_string(),
        bool01(cfg.components.enable_router_filter).to_string(),
    );
    env.insert(
        "ENABLE_E2E".to_string(),
        bool01(cfg.components.enable_e2e).to_string(),
    );
    env.insert("DEBIAN_SUITE".to_string(), cfg.build.debian_suite.clone());
    env.insert("DEBUG".to_string(), bool01(cfg.logging.debug).to_string());
    env.insert("LOG_LEVEL".to_string(), cfg.logging.log_level.clone());
    env.insert("LOG_FORMAT".to_string(), cfg.logging.log_format.clone());
    env.insert(
        "NO_CACHE".to_string(),
        bool01(cfg.build.no_cache).to_string(),
    );
    env.insert(
        "PULL_BASE".to_string(),
        bool01(cfg.build.pull_base).to_string(),
    );
    env.insert("TIMEOUT".to_string(), cfg.timeouts.rollout.clone());

    Ok(env)
}

fn ensure_tool(name: &str, args: &[&str]) -> anyhow::Result<()> {
    let status = Command::new(name)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    match status {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == ErrorKind::NotFound => Err(anyhow!("missing dependency: {}", name)),
        Err(e) => Err(anyhow!("failed to run {}: {}", name, e)),
    }
}

fn docker_check_daemon() -> anyhow::Result<()> {
    let docker = Docker::connect_with_local_defaults().context("connect docker daemon")?;
    let rt = Runtime::new().context("create tokio runtime")?;
    rt.block_on(async {
        docker.version().await.context("docker version (api)")?;
        Ok::<(), anyhow::Error>(())
    })?;
    Ok(())
}

async fn docker_assert_image_exists(docker: &Docker, image: &str) -> anyhow::Result<()> {
    match docker.inspect_image(image).await {
        Ok(_) => Ok(()),
        Err(BollardError::DockerResponseServerError {
            status_code: 404, ..
        }) => Err(anyhow!("docker image not found: {}", image)),
        Err(e) => Err(anyhow!("docker inspect image failed ({}): {}", image, e)),
    }
}

fn run_checked(cmd: &mut Command, ctx: &str) -> anyhow::Result<()> {
    cmd.stdin(Stdio::inherit());
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());
    let status = cmd.status().with_context(|| ctx.to_string())?;
    if !status.success() {
        return Err(anyhow!(
            "{} failed (rc={})",
            ctx,
            status.code().unwrap_or(1)
        ));
    }
    Ok(())
}

fn kubeconfig_path(repo_root: &Path, cfg: &Config) -> PathBuf {
    repo_root.join(&cfg.k8s.kind.kubeconfig_file)
}

fn parse_timeout(s: &str) -> anyhow::Result<Duration> {
    let s = s.trim();
    if s.is_empty() {
        return Err(anyhow!("timeout is empty"));
    }

    let (num, unit) = s.split_at(s.len().saturating_sub(1));
    let n: u64 = num
        .parse()
        .with_context(|| format!("parse timeout number: {}", s))?;

    match unit {
        "s" => Ok(Duration::from_secs(n)),
        "m" => Ok(Duration::from_secs(n * 60)),
        "h" => Ok(Duration::from_secs(n * 3600)),
        _ => {
            Ok(Duration::from_secs(s.parse().with_context(|| {
                format!("parse timeout seconds: {}", s)
            })?))
        }
    }
}

async fn kube_client_from_kubeconfig(kubeconfig_path: &Path) -> anyhow::Result<Client> {
    let kubeconfig = Kubeconfig::read_from(kubeconfig_path).context("read kubeconfig file")?;
    let opts = KubeConfigOptions {
        context: None,
        cluster: None,
        user: None,
    };
    let config = kube::Config::from_custom_kubeconfig(kubeconfig, &opts)
        .await
        .context("load kube config")?;
    Client::try_from(config).context("create kube client")
}

async fn ensure_namespace(client: Client, ns: &str) -> anyhow::Result<()> {
    use k8s_openapi::api::core::v1::Namespace;
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

    let api: Api<Namespace> = Api::all(client);
    match api.get_opt(ns).await.context("get namespace")? {
        Some(_) => Ok(()),
        None => {
            let ns_obj = Namespace {
                metadata: ObjectMeta {
                    name: Some(ns.to_string()),
                    ..Default::default()
                },
                ..Default::default()
            };
            api.create(&PostParams::default(), &ns_obj)
                .await
                .context("create namespace")?;
            Ok(())
        }
    }
}

async fn apply_openai_secret_from_env(client: Client, cfg: &Config) -> anyhow::Result<()> {
    use std::collections::BTreeMap;

    use k8s_openapi::api::core::v1::Secret;
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

    if cfg.secrets.openai.source != "from-env" {
        return Ok(());
    }
    let env_name = cfg.secrets.openai.env_name.trim();
    if env_name.is_empty() {
        return Ok(());
    }
    let v = match std::env::var(env_name) {
        Ok(v) if !v.is_empty() => v,
        _ => return Ok(()),
    };

    let ns = &cfg.k8s.namespace;
    let secret_name = &cfg.secrets.openai.k8s_secret_name;
    let secret_key = &cfg.secrets.openai.k8s_secret_key;

    let mut string_data = BTreeMap::new();
    string_data.insert(secret_key.clone(), v);

    let secret = Secret {
        metadata: ObjectMeta {
            name: Some(secret_name.clone()),
            namespace: Some(ns.clone()),
            ..Default::default()
        },
        type_: Some("Opaque".to_string()),
        string_data: Some(string_data),
        ..Default::default()
    };

    let api: Api<Secret> = Api::namespaced(client, ns);
    let pp = PatchParams::apply("spear-quickstart").force();
    api.patch(secret_name, &pp, &Patch::Apply(&secret))
        .await
        .context("apply openai secret")?;
    Ok(())
}

fn pod_ready(pod: &k8s_openapi::api::core::v1::Pod) -> bool {
    pod.status
        .as_ref()
        .and_then(|s| s.conditions.as_ref())
        .map(|conds| {
            conds
                .iter()
                .any(|c| c.type_ == "Ready" && c.status == "True")
        })
        .unwrap_or(false)
}

async fn wait_pod_ready(
    client: Client,
    ns: &str,
    name: &str,
    timeout: Duration,
) -> anyhow::Result<()> {
    use k8s_openapi::api::core::v1::Pod;

    let api: Api<Pod> = Api::namespaced(client, ns);
    let start = Instant::now();
    loop {
        if start.elapsed() > timeout {
            return Err(anyhow!("timeout waiting for pod Ready: {}", name));
        }
        if let Ok(pod) = api.get(name).await {
            if pod_ready(&pod) {
                return Ok(());
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn wait_pods_ready_by_label(
    client: Client,
    ns: &str,
    label_selector: &str,
    timeout: Duration,
) -> anyhow::Result<()> {
    use k8s_openapi::api::core::v1::Pod;

    let api: Api<Pod> = Api::namespaced(client, ns);
    let lp = ListParams::default().labels(label_selector);
    let start = Instant::now();
    loop {
        if start.elapsed() > timeout {
            return Err(anyhow!(
                "timeout waiting for pods Ready (label={})",
                label_selector
            ));
        }
        let pods = api.list(&lp).await.context("list pods")?;
        let items = pods.items;
        if !items.is_empty() && items.iter().all(pod_ready) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

fn apply_k8s_kind_native(cfg: &Config) -> anyhow::Result<()> {
    docker_check_daemon()?;
    ensure_tool("kind", &["version"])?;
    ensure_tool("helm", &["version"])?;

    let repo_root = config::repo_root()?;
    let kubeconfig = kubeconfig_path(&repo_root, cfg);
    if let Some(dir) = kubeconfig.parent() {
        fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    }

    let kubeconfig_str = kubeconfig.to_string_lossy().to_string();
    let cluster_name = &cfg.k8s.kind.cluster_name;

    let clusters_out = Command::new("kind")
        .current_dir(&repo_root)
        .env("KUBECONFIG", &kubeconfig_str)
        .args(["get", "clusters"])
        .output()
        .context("kind get clusters")?;
    if !clusters_out.status.success() {
        return Err(anyhow!(
            "kind get clusters failed (rc={})",
            clusters_out.status.code().unwrap_or(1)
        ));
    }
    let clusters_text = String::from_utf8_lossy(&clusters_out.stdout);
    let cluster_exists = clusters_text.lines().any(|l| l.trim() == cluster_name);

    let mut apply_result = Ok(());

    if cluster_exists {
        if !cfg.k8s.kind.reuse_cluster {
            apply_result = apply_result.and_then(|_| {
                let mut cmd = Command::new("kind");
                cmd.current_dir(&repo_root)
                    .env("KUBECONFIG", &kubeconfig_str)
                    .args(["delete", "cluster", "--name", cluster_name]);
                run_checked(&mut cmd, "kind delete cluster")
            });
            apply_result = apply_result.and_then(|_| {
                let mut cmd = Command::new("kind");
                cmd.current_dir(&repo_root)
                    .env("KUBECONFIG", &kubeconfig_str)
                    .args(["create", "cluster", "--name", cluster_name]);
                run_checked(&mut cmd, "kind create cluster")
            });
        }
    } else {
        apply_result = apply_result.and_then(|_| {
            let mut cmd = Command::new("kind");
            cmd.current_dir(&repo_root)
                .env("KUBECONFIG", &kubeconfig_str)
                .args(["create", "cluster", "--name", cluster_name]);
            run_checked(&mut cmd, "kind create cluster")
        });
    }

    apply_result = apply_result.and_then(|_| {
        let mut cmd = Command::new("kind");
        cmd.current_dir(&repo_root)
            .env("KUBECONFIG", &kubeconfig_str)
            .args([
                "export",
                "kubeconfig",
                "--name",
                cluster_name,
                "--kubeconfig",
                &kubeconfig_str,
            ]);
        run_checked(&mut cmd, "kind export kubeconfig")
    });

    apply_result = apply_result.and_then(|_| {
        let rt = Runtime::new().context("create tokio runtime")?;
        rt.block_on(async {
            let client = kube_client_from_kubeconfig(&kubeconfig).await?;
            client
                .apiserver_version()
                .await
                .context("kube apiserver version")?;
            Ok::<(), anyhow::Error>(())
        })
    });

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
        ensure_tool("docker", &["version"])?;
        if unified_repo.is_empty() {
            apply_result = apply_result.and_then(|_| {
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
                run_checked(&mut cmd, "docker build sms")
            });

            apply_result = apply_result.and_then(|_| {
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
                run_checked(&mut cmd, "docker build spearlet")
            });
        } else {
            apply_result = apply_result.and_then(|_| {
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
                run_checked(&mut cmd, "docker build spear (unified)")
            });
        }

        apply_result = apply_result.and_then(|_| {
            let rt = Runtime::new().context("create tokio runtime")?;
            rt.block_on(async {
                let docker =
                    Docker::connect_with_local_defaults().context("connect docker daemon")?;
                docker_assert_image_exists(&docker, &sms_image).await?;
                docker_assert_image_exists(&docker, &spearlet_image).await?;
                Ok::<(), anyhow::Error>(())
            })?;

            let mut cmd = Command::new("kind");
            cmd.current_dir(&repo_root);
            cmd.env("KUBECONFIG", &kubeconfig_str);
            cmd.args(["load", "docker-image", "--name", cluster_name]);
            cmd.arg(&sms_image);
            if sms_image != spearlet_image {
                cmd.arg(&spearlet_image);
            }
            run_checked(&mut cmd, "kind load docker-image")
        });
    }

    let ns = &cfg.k8s.namespace;
    apply_result = apply_result.and_then(|_| {
        let rt = Runtime::new().context("create tokio runtime")?;
        rt.block_on(async {
            let client = kube_client_from_kubeconfig(&kubeconfig).await?;
            ensure_namespace(client.clone(), ns).await?;
            apply_openai_secret_from_env(client.clone(), cfg).await?;
            Ok::<(), anyhow::Error>(())
        })
    });

    let release = &cfg.k8s.release_name;
    let chart_path = repo_root.join(&cfg.k8s.chart_path);
    let mut helm_args = Vec::<String>::new();
    helm_args.push("upgrade".to_string());
    helm_args.push("--install".to_string());
    helm_args.push(release.to_string());
    helm_args.push(chart_path.to_string_lossy().to_string());
    helm_args.push("-n".to_string());
    helm_args.push(ns.to_string());
    helm_args.push("--create-namespace".to_string());
    for vf in &cfg.k8s.values_files {
        helm_args.push("-f".to_string());
        helm_args.push(repo_root.join(vf).to_string_lossy().to_string());
    }

    let (level, format) = if cfg.logging.debug {
        ("debug".to_string(), "pretty".to_string())
    } else {
        (
            cfg.logging.log_level.clone(),
            cfg.logging.log_format.clone(),
        )
    };

    helm_args.push("--set".to_string());
    helm_args.push(format!("sms.config.logging.level={}", level));
    helm_args.push("--set".to_string());
    helm_args.push(format!("sms.config.logging.format={}", format));
    helm_args.push("--set".to_string());
    helm_args.push(format!("spearlet.config.logging.level={}", level));
    helm_args.push("--set".to_string());
    helm_args.push(format!("spearlet.config.logging.format={}", format));
    helm_args.push("--set".to_string());
    helm_args.push(format!(
        "sms.image.repository={}",
        if unified_repo.is_empty() {
            cfg.images.sms_repo.as_str()
        } else {
            unified_repo
        }
    ));
    helm_args.push("--set".to_string());
    helm_args.push(format!("sms.image.tag={}", cfg.images.tag));
    helm_args.push("--set".to_string());
    helm_args.push(format!(
        "spearlet.image.repository={}",
        if unified_repo.is_empty() {
            cfg.images.spearlet_repo.as_str()
        } else {
            unified_repo
        }
    ));
    helm_args.push("--set".to_string());
    helm_args.push(format!("spearlet.image.tag={}", cfg.images.tag));

    if !cfg.components.enable_web_admin {
        helm_args.push("--set".to_string());
        helm_args.push("sms.config.enableWebAdmin=false".to_string());
    }

    helm_args.push("--set".to_string());
    helm_args.push(format!(
        "routerFilter.enabled={}",
        if cfg.components.enable_router_filter {
            "true"
        } else {
            "false"
        }
    ));

    if cfg.components.enable_e2e {
        helm_args.push("--set".to_string());
        helm_args.push("e2e.enabled=true".to_string());
    }

    apply_result = apply_result.and_then(|_| {
        let mut cmd = Command::new("helm");
        cmd.current_dir(&repo_root)
            .env("KUBECONFIG", &kubeconfig_str)
            .args(helm_args);
        run_checked(&mut cmd, "helm upgrade --install")
    });

    apply_result = apply_result.and_then(|_| {
        let timeout = parse_timeout(&cfg.timeouts.rollout)?;
        let rt = Runtime::new().context("create tokio runtime")?;
        rt.block_on(async {
            let client = kube_client_from_kubeconfig(&kubeconfig).await?;
            wait_pod_ready(
                client.clone(),
                ns,
                &format!("{}-spear-sms-0", release),
                timeout,
            )
            .await?;
            wait_pods_ready_by_label(
                client.clone(),
                ns,
                "app.kubernetes.io/component=spearlet",
                timeout,
            )
            .await?;
            Ok::<(), anyhow::Error>(())
        })
    });

    if !cfg.k8s.kind.keep_cluster && !cfg.k8s.kind.reuse_cluster && apply_result.is_err() {
        let _ = Command::new("kind")
            .current_dir(&repo_root)
            .env("KUBECONFIG", &kubeconfig_str)
            .args(["delete", "cluster", "--name", cluster_name])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    apply_result
}

pub fn render_plan(cfg: &Config) -> anyhow::Result<String> {
    match cfg.mode.name.as_str() {
        "k8s-kind" => {
            let env = env_for_kind(cfg)?;
            let repo_root = config::repo_root()?;
            let script = repo_root.join("scripts/kind-openai-quickstart.sh");

            let mut out = String::new();
            out.push_str("# SPEAR quickstart plan\n\n");

            out.push_str("Config:\n");
            out.push_str(&format!("  mode.name: {}\n", cfg.mode.name));
            out.push_str(&format!("  k8s.namespace: {}\n", cfg.k8s.namespace));
            out.push_str(&format!("  k8s.release_name: {}\n", cfg.k8s.release_name));
            out.push_str(&format!(
                "  k8s.kind.cluster_name: {}\n",
                cfg.k8s.kind.cluster_name
            ));
            out.push_str(&format!(
                "  k8s.kind.reuse_cluster: {}\n",
                cfg.k8s.kind.reuse_cluster
            ));
            out.push_str(&format!(
                "  k8s.kind.keep_cluster: {}\n",
                cfg.k8s.kind.keep_cluster
            ));
            out.push_str(&format!(
                "  k8s.kind.kubeconfig_file: {}\n",
                cfg.k8s.kind.kubeconfig_file
            ));
            out.push_str("\n");

            out.push_str("Phase A: Validate\n");
            out.push_str("  - Requires: docker, kind, helm\n");
            out.push_str("  - Requires: kube API access (via kubeconfig)\n\n");

            out.push_str("Phase B: Prepare\n");
            out.push_str(&format!("  - repo_root: {}\n", repo_root.display()));
            out.push_str(&format!(
                "  - kubeconfig_file: {}\n\n",
                repo_root.join(&cfg.k8s.kind.kubeconfig_file).display()
            ));

            out.push_str("Phase C: Build Images\n");
            out.push_str(&format!("  - build.enabled: {}\n", cfg.build.enabled));
            out.push_str(&format!("  - build.pull_base: {}\n", cfg.build.pull_base));
            out.push_str(&format!("  - build.no_cache: {}\n", cfg.build.no_cache));
            out.push_str(&format!(
                "  - build.debian_suite: {}\n",
                cfg.build.debian_suite
            ));
            out.push_str(&format!("  - images.tag: {}\n", cfg.images.tag));
            out.push_str("\n");

            out.push_str("Phase D: Deploy (k8s-kind)\n");
            out.push_str(
                "  - Runs native Rust steps (set SPEAR_QUICKSTART_USE_SCRIPT=1 to delegate)\n\n",
            );

            out.push_str("Delegation:\n");
            out.push_str("  env:\n");
            let mut keys: Vec<_> = env.keys().collect();
            keys.sort();
            for k in keys {
                out.push_str(&format!("    {}={}\n", k, env.get(k).unwrap()));
            }
            out.push_str("\n");
            out.push_str("  command:\n");
            out.push_str("    helm/kubectl/kind/docker commands\n");
            out.push_str(&format!("    fallback script: {}\n", script.display()));

            Ok(out)
        }
        "docker-local" => deploy_docker_local::render_plan(cfg),
        other => Err(anyhow!(
            "mode {} is not implemented in Rust quickstart yet",
            other
        )),
    }
}

pub fn plan(cfg: &Config) -> anyhow::Result<()> {
    println!("{}", render_plan(cfg)?);
    Ok(())
}

pub fn apply(cfg: &Config, yes: bool) -> anyhow::Result<()> {
    match cfg.mode.name.as_str() {
        "k8s-kind" => {
            if !yes {
                return Err(anyhow!(
                    "apply requires --yes (interactive confirmations not implemented)"
                ));
            }

            if std::env::var("SPEAR_QUICKSTART_USE_SCRIPT").ok().as_deref() == Some("1") {
                let env = env_for_kind(cfg)?;
                let repo_root = config::repo_root()?;
                let script = repo_root.join("scripts/kind-openai-quickstart.sh");
                let mut cmd = Command::new(script);
                cmd.current_dir(&repo_root);
                cmd.envs(env);
                cmd.stdin(Stdio::inherit());
                cmd.stdout(Stdio::inherit());
                cmd.stderr(Stdio::inherit());
                let status = cmd.status().context("run kind-openai-quickstart.sh")?;
                if !status.success() {
                    return Err(anyhow!("apply failed (rc={})", status.code().unwrap_or(1)));
                }
                Ok(())
            } else {
                apply_k8s_kind_native(cfg)
            }
        }
        "docker-local" => {
            if !yes {
                return Err(anyhow!(
                    "apply requires --yes (interactive confirmations not implemented)"
                ));
            }
            deploy_docker_local::apply(cfg)
        }
        other => Err(anyhow!(
            "mode {} is not implemented in Rust quickstart yet",
            other
        )),
    }
}

pub fn status(cfg: &Config) -> anyhow::Result<()> {
    println!("{}", status_text(cfg)?);
    Ok(())
}

pub fn status_text(cfg: &Config) -> anyhow::Result<String> {
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

    match cfg.mode.name.as_str() {
        "k8s-kind" => {
            let repo_root = config::repo_root()?;
            let kubeconfig = repo_root.join(&cfg.k8s.kind.kubeconfig_file);
            let ns = &cfg.k8s.namespace;
            let release = &cfg.k8s.release_name;

            let kubeconfig_str = kubeconfig.to_string_lossy().to_string();

            let mut out = String::new();
            out.push_str(&format!("mode: {}\n", cfg.mode.name));
            out.push_str(&format!("namespace: {}\n", ns));
            out.push_str(&format!("release: {}\n\n", release));

            out.push_str("== helm status ==\n");
            let mut helm = Command::new("helm");
            helm.current_dir(&repo_root)
                .env("KUBECONFIG", &kubeconfig_str)
                .args(["-n", ns, "status", release]);
            out.push_str(&run_capture(helm, "helm status")?);
            out.push('\n');

            out.push_str("== kubectl get pods -o wide ==\n");
            let mut kubectl = Command::new("kubectl");
            kubectl
                .current_dir(&repo_root)
                .env("KUBECONFIG", &kubeconfig_str)
                .args(["-n", ns, "get", "pods", "-o", "wide"]);
            out.push_str(&run_capture(kubectl, "kubectl get pods")?);

            Ok(out)
        }
        "docker-local" => deploy_docker_local::status_text(cfg),
        other => Err(anyhow!(
            "mode {} is not implemented in Rust quickstart yet",
            other
        )),
    }
}

pub fn cleanup(cfg: &Config, scope_csv: &str, yes: bool) -> anyhow::Result<()> {
    let scope: Vec<&str> = scope_csv
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    match cfg.mode.name.as_str() {
        "k8s-kind" => {
            let repo_root = config::repo_root()?;
            let kubeconfig = repo_root.join(&cfg.k8s.kind.kubeconfig_file);
            let ns = &cfg.k8s.namespace;
            let release = &cfg.k8s.release_name;

            for s in &scope {
                match *s {
                    "release" => {
                        let _ = Command::new("helm")
                            .current_dir(&repo_root)
                            .env("KUBECONFIG", kubeconfig.to_string_lossy().to_string())
                            .args(["-n", ns, "uninstall", release])
                            .stdin(Stdio::inherit())
                            .stdout(Stdio::inherit())
                            .stderr(Stdio::inherit())
                            .status();
                    }
                    "secret" => {
                        let _ = Command::new("kubectl")
                            .current_dir(&repo_root)
                            .env("KUBECONFIG", kubeconfig.to_string_lossy().to_string())
                            .args([
                                "-n",
                                ns,
                                "delete",
                                "secret",
                                &cfg.secrets.openai.k8s_secret_name,
                                "--ignore-not-found=true",
                            ])
                            .stdin(Stdio::inherit())
                            .stdout(Stdio::inherit())
                            .stderr(Stdio::inherit())
                            .status();
                    }
                    "namespace" => {
                        if !yes {
                            return Err(anyhow!("cleanup namespace requires --yes"));
                        }
                        let _ = Command::new("kubectl")
                            .current_dir(&repo_root)
                            .env("KUBECONFIG", kubeconfig.to_string_lossy().to_string())
                            .args(["delete", "namespace", ns, "--ignore-not-found=true"])
                            .stdin(Stdio::inherit())
                            .stdout(Stdio::inherit())
                            .stderr(Stdio::inherit())
                            .status();
                    }
                    "kind" => {
                        if !yes {
                            return Err(anyhow!("cleanup kind requires --yes"));
                        }
                        let _ = Command::new("kind")
                            .current_dir(&repo_root)
                            .args(["delete", "cluster", "--name", &cfg.k8s.kind.cluster_name])
                            .stdin(Stdio::inherit())
                            .stdout(Stdio::inherit())
                            .stderr(Stdio::inherit())
                            .status();
                    }
                    other => return Err(anyhow!("unknown scope item: {}", other)),
                }
            }

            Ok(())
        }
        "docker-local" => deploy_docker_local::cleanup(cfg, &scope, yes),
        other => Err(anyhow!(
            "mode {} is not implemented in Rust quickstart yet",
            other
        )),
    }
}
