// E2E test using testcontainers to run SMS and SPEARlet in Docker
// 端到端测试：使用testcontainers在Docker中运行SMS与SPEARlet并建立连接
// Run with Docker installed; test is ignored by default
// 运行需安装Docker；默认忽略该测试

use std::path::PathBuf;
use std::time::Duration;

use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::Command;
use testcontainers::{clients, core::WaitFor, GenericImage, RunnableImage};

fn to_abs_path(p: PathBuf) -> PathBuf {
    if p.is_absolute() {
        return p;
    }
    std::env::current_dir().unwrap().join(p)
}

fn canonical_or_abs(p: PathBuf) -> PathBuf {
    if p.exists() {
        std::fs::canonicalize(&p).unwrap_or_else(|_| to_abs_path(p))
    } else {
        to_abs_path(p)
    }
}

fn target_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("CARGO_TARGET_DIR") {
        canonical_or_abs(PathBuf::from(dir))
    } else {
        canonical_or_abs(PathBuf::from("target"))
    }
}

fn binary_path(name: &str) -> PathBuf {
    // Prefer externally provided binary directory (Linux target) / 优先使用外部提供的二进制目录（Linux目标）
    if let Ok(dir) = std::env::var("E2E_BIN_DIR") {
        let mut p = PathBuf::from(dir);
        p.push(name);
        return canonical_or_abs(p);
    }
    // Fallback to debug / 回退到debug
    let mut p = target_dir();
    p.push("debug");
    p.push(name);
    canonical_or_abs(p)
}

fn build_dir_for_current_mode() -> PathBuf {
    if let Ok(dir) = std::env::var("E2E_BIN_DIR") {
        let mut p = PathBuf::from(dir);
        p.push("build");
        return canonical_or_abs(p);
    }
    let mut p = target_dir();
    p.push("debug");
    p.push("build");
    canonical_or_abs(p)
}

fn find_wasmedge_lib_dir() -> Option<PathBuf> {
    let build_dir = build_dir_for_current_mode();
    let entries = std::fs::read_dir(&build_dir).ok()?;
    for ent in entries.flatten() {
        let ty = ent.file_type().ok()?;
        if !ty.is_dir() {
            continue;
        }
        let name = ent.file_name().to_string_lossy().to_string();
        if !name.starts_with("wasmedge-sys-") {
            continue;
        }
        let standalone = ent.path().join("out").join("standalone");
        let s_entries = std::fs::read_dir(&standalone).ok()?;
        for s in s_entries.flatten() {
            let sty = s.file_type().ok()?;
            if !sty.is_dir() {
                continue;
            }
            let sname = s.file_name().to_string_lossy().to_string();
            if !sname.starts_with("WasmEdge-") {
                continue;
            }
            let lib64 = s.path().join("lib64");
            if lib64.exists() {
                return Some(canonical_or_abs(lib64));
            }
        }
    }
    None
}

fn write_spearlet_e2e_config() -> PathBuf {
    let dir = target_dir().join("e2e");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!(
        "spearlet-{}.toml",
        uuid::Uuid::new_v4().simple().to_string()
    ));
    let toml = r#"
[[spearlet.ai.backends]]
name = "stub_local"
kind = "stub"
base_url = ""
hosting = "local"
weight = 100
priority = 0
ops = ["chat_completions"]
features = []
transports = ["in_process"]

[[spearlet.ai.backends]]
name = "stub_remote"
kind = "stub"
base_url = ""
hosting = "remote"
weight = 100
priority = 0
ops = ["chat_completions"]
features = []
transports = ["in_process"]
"#;
    std::fs::write(&path, toml.trim_start()).unwrap();
    canonical_or_abs(path)
}

fn http_get_body(host: &str, port: u16, path: &str) -> String {
    let mut stream = TcpStream::connect((host, port)).unwrap();
    let req = format!(
        "GET {} HTTP/1.1\r\nHost: {}:{}\r\nConnection: close\r\n\r\n",
        path, host, port
    );
    stream.write_all(req.as_bytes()).unwrap();
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).unwrap();
    let s = String::from_utf8_lossy(&buf).to_string();
    if let Some(i) = s.find("\r\n\r\n") {
        return s[i + 4..].to_string();
    }
    s
}

fn docker_logs(name: &str) -> String {
    let out = Command::new("docker").args(["logs", name]).output().ok();
    let mut s = String::new();
    if let Some(o) = out {
        s.push_str(&String::from_utf8_lossy(&o.stdout));
        if !o.stderr.is_empty() {
            s.push_str("\n[stderr]\n");
            s.push_str(&String::from_utf8_lossy(&o.stderr));
        }
    }
    s
}

fn docker_inspect_state(name: &str) -> String {
    let out = Command::new("docker")
        .args([
            "inspect",
            "--format",
            "{{.State.Status}} exit={{.State.ExitCode}} oom={{.State.OOMKilled}} err={{.State.Error}} started={{.State.StartedAt}} finished={{.State.FinishedAt}}",
            name,
        ])
        .output()
        .ok();
    out.map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default()
}

#[test]
#[ignore]
fn e2e_spearlet_connects_to_sms_with_testcontainers() {
    // Skip on non-Linux hosts unless E2E_BIN_DIR is provided (Linux binary) / 非Linux主机跳过，除非提供E2E_BIN_DIR（Linux二进制）
    if std::env::consts::OS != "linux" && std::env::var("E2E_BIN_DIR").is_err() {
        eprintln!("E2E requires Linux binaries. Set E2E_BIN_DIR to Linux-target binaries (e.g., target/x86_64-unknown-linux-musl/release). Skipping.");
        return;
    }
    // Check docker availability / 检查docker可用性
    if std::process::Command::new("docker")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("Docker not available, skipping E2E test");
        return;
    }
    if std::process::Command::new("docker")
        .arg("info")
        .output()
        .is_err()
    {
        eprintln!("Docker daemon not reachable, skipping E2E test");
        return;
    }

    let docker = clients::Cli::default();

    // Paths to host-built binaries / 宿主机已构建的二进制路径
    let sms_bin = binary_path("sms");
    let spearlet_bin = binary_path("spearlet");
    println!(
        "Using binaries: sms={:?}, spearlet={:?}",
        sms_bin, spearlet_bin
    );
    assert!(sms_bin.exists(), "sms binary not found at {:?}", sms_bin);
    assert!(
        spearlet_bin.exists(),
        "spearlet binary not found at {:?}",
        spearlet_bin
    );

    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let sms_name = format!("spear-e2e-sms-{}", suffix);
    let spear_name = format!("spear-e2e-spearlet-{}", suffix);
    let net_name = "spear-e2e-net";
    let wasmedge_lib_dir = find_wasmedge_lib_dir().expect("wasmedge lib64 dir not found");
    let config_path = write_spearlet_e2e_config();

    let net_ok = Command::new("docker")
        .args(["network", "inspect", net_name])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !net_ok {
        let _ = Command::new("docker")
            .args(["network", "create", net_name])
            .output();
    }

    // Cleanup existing containers / 清理已有容器
    let _ = Command::new("docker")
        .args(["rm", "-f", sms_name.as_str(), spear_name.as_str()])
        .output();

    // Start SMS container / 启动SMS容器
    let sms_image = GenericImage::new("debian", "bookworm-slim")
        .with_exposed_port(50051)
        .with_exposed_port(8080)
        .with_wait_for(WaitFor::message_on_stdout("SMS gRPC server listening"))
        .with_volume(sms_bin.to_string_lossy().to_string(), "/usr/local/bin/sms")
        .with_volume(
            wasmedge_lib_dir.to_string_lossy().to_string(),
            "/opt/wasmedge/lib",
        )
        .with_env_var("SMS_GRPC_ADDR", "0.0.0.0:50051")
        .with_env_var("SMS_HTTP_ADDR", "0.0.0.0:8080")
        .with_env_var("RUST_LOG", "info")
        .with_env_var("LD_LIBRARY_PATH", "/opt/wasmedge/lib")
        .with_entrypoint("/usr/local/bin/sms");

    // Attach to user-defined network at creation / 在创建时附加到用户自定义网络
    let _sms_container = match std::panic::catch_unwind(|| {
        docker.run(
            RunnableImage::from(sms_image)
                .with_container_name(sms_name.as_str())
                .with_network(net_name),
        )
    }) {
        Ok(c) => c,
        Err(_) => {
            let out = Command::new("docker")
                .args(["logs", sms_name.as_str()])
                .output()
                .ok();
            let logs = out
                .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                .unwrap_or_default();
            panic!("Failed to start SMS container. Logs:\n{}", logs);
        }
    };

    // Brief wait to ensure SMS is up / 等待SMS启动
    std::thread::sleep(Duration::from_millis(300));

    // Start SPEARlet container to connect to SMS via host gateway / 启动SPEARlet容器并通过宿主网关连接SMS
    // testcontainers injects host.testcontainers.internal -> docker host
    let spear_image = GenericImage::new("debian", "bookworm-slim")
        .with_exposed_port(50052)
        .with_exposed_port(8081)
        .with_volume(
            spearlet_bin.to_string_lossy().to_string(),
            "/usr/local/bin/spearlet",
        )
        .with_volume(
            wasmedge_lib_dir.to_string_lossy().to_string(),
            "/opt/wasmedge/lib",
        )
        .with_volume(
            config_path.to_string_lossy().to_string(),
            "/etc/spearlet/config.toml",
        )
        .with_env_var("RUST_LOG", "info")
        .with_env_var("SPEAR_E2E", "1")
        .with_env_var("LD_LIBRARY_PATH", "/opt/wasmedge/lib")
        .with_entrypoint("/usr/local/bin/spearlet");

    let spear_container = match std::panic::catch_unwind(|| {
        docker.run(
            RunnableImage::from((
                spear_image,
                vec![
                    "--config".to_string(),
                    "/etc/spearlet/config.toml".to_string(),
                    "--sms-grpc-addr".to_string(),
                    format!("{}:{}", sms_name, 50051),
                    "--sms-http-addr".to_string(),
                    format!("{}:{}", sms_name, 8080),
                    "--auto-register".to_string(),
                    "true".to_string(),
                ],
            ))
            .with_container_name(spear_name.as_str())
            .with_network(net_name),
        )
    }) {
        Ok(c) => c,
        Err(_) => {
            let out = Command::new("docker")
                .args(["logs", spear_name.as_str()])
                .output()
                .ok();
            let logs = out
                .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                .unwrap_or_default();
            panic!("Failed to start SPEARlet container. Logs:\n{}", logs);
        }
    };

    let start = std::time::Instant::now();
    loop {
        let out = Command::new("docker")
            .args(["logs", spear_name.as_str()])
            .output()
            .ok();
        let logs = out
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();
        if logs.contains("Connected to SMS successfully") {
            break;
        }
        if start.elapsed() > Duration::from_secs(30) {
            panic!("Failed to start SPEARlet container. Logs:\n{}", logs);
        }
        std::thread::sleep(Duration::from_millis(300));
    }

    let http_port = spear_container.get_host_port_ipv4(8081);
    let url_path = "/health";
    let start = std::time::Instant::now();
    let body = loop {
        let b = http_get_body("127.0.0.1", http_port, url_path);
        if !b.trim().is_empty() {
            break b;
        }
        if start.elapsed() > Duration::from_secs(30) {
            let sms_logs = docker_logs(sms_name.as_str());
            let spear_logs = docker_logs(spear_name.as_str());
            let sms_state = docker_inspect_state(sms_name.as_str());
            let spear_state = docker_inspect_state(spear_name.as_str());
            panic!(
                "E2E failed.\n\n[response]\n{}\n\n[SMS state]\n{}\n\n[SMS logs]\n{}\n\n[SPEARlet state]\n{}\n\n[SPEARlet logs]\n{}",
                b, sms_state, sms_logs, spear_state, spear_logs
            );
        }
        std::thread::sleep(Duration::from_millis(300));
    };

    // Print logs for visibility when running with --nocapture / 输出日志以便查看
    if let Ok(out) = Command::new("docker")
        .args(["logs", sms_name.as_str()])
        .output()
    {
        println!("[SMS logs]\n{}", String::from_utf8_lossy(&out.stdout));
    }
    if let Ok(out) = Command::new("docker")
        .args(["logs", spear_name.as_str()])
        .output()
    {
        println!("[SPEARlet logs]\n{}", String::from_utf8_lossy(&out.stdout));
    }
    println!("[e2e response]\n{}", body);

    // If we reach here without panic, basic E2E succeeded / 到此处且未panic则E2E基本成功
}
