use spear_next::proto::sms::node_service_server::NodeServiceServer;
use spear_next::sms::service::SmsServiceImpl;
use spear_next::spearlet::config::SpearletConfig;
use spear_next::spearlet::registration::RegistrationService;
use spear_next::spearlet::sms_connector::sms_channel_lazy;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::time::{sleep, timeout, Duration};
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Server;

async fn start_sms_server(bind: Option<String>) -> (tokio::task::JoinHandle<()>, String) {
    let bind_addr = bind.unwrap_or_else(|| "127.0.0.1:0".to_string());
    let listener = TcpListener::bind(&bind_addr).await.unwrap();
    let addr = listener.local_addr().unwrap();
    let service = SmsServiceImpl::new(
        Arc::new(tokio::sync::RwLock::new(
            spear_next::sms::services::NodeService::new(),
        )),
        Arc::new(spear_next::sms::services::ResourceService::new()),
        Arc::new(spear_next::sms::config::SmsConfig::default()),
    )
    .await;
    let handle = tokio::spawn(async move {
        Server::builder()
            .add_service(NodeServiceServer::new(service))
            .serve_with_incoming(TcpListenerStream::new(listener))
            .await
            .unwrap();
    });
    (handle, format!("{}:{}", addr.ip(), addr.port()))
}

async fn wait_registered(svc: &RegistrationService, max_ms: u64) -> bool {
    let deadline = Duration::from_millis(max_ms);
    let fut = async {
        loop {
            if svc.get_state().await.is_registered() {
                break true;
            }
            sleep(Duration::from_millis(50)).await;
        }
    };
    timeout(deadline, fut).await.unwrap_or(false)
}

fn make_spearlet_config(sms_grpc_addr: String) -> Arc<SpearletConfig> {
    Arc::new(SpearletConfig {
        node_name: "test-node".to_string(),
        max_blocking_threads: 512,
        grpc: spear_next::config::base::ServerConfig {
            addr: "127.0.0.1:50055".parse().unwrap(),
            ..Default::default()
        },
        http: spear_next::spearlet::config::HttpConfig::default(),
        storage: spear_next::spearlet::config::StorageConfig::default(),
        local_models_dir: String::new(),
        logging: spear_next::config::base::LogConfig {
            level: "debug".to_string(),
            format: "json".to_string(),
            file: None,
        },
        sms_grpc_addr: sms_grpc_addr,
        sms_http_addr: "127.0.0.1:8080".to_string(),
        auto_register: true,
        heartbeat_interval: 1,
        cleanup_interval: 10,
        sms_connect_timeout_ms: 3_000,
        sms_connect_retry_ms: 200,
        reconnect_total_timeout_ms: 30_000,
        ai: spear_next::spearlet::config::AiConfig::default(),
    })
}

#[tokio::test]
async fn test_attempt_reconnect_and_register() {
    let (_h, sms_grpc_addr) = start_sms_server(None).await;
    let cfg = make_spearlet_config(sms_grpc_addr);
    let svc = RegistrationService::new(cfg.clone(), sms_channel_lazy(&cfg).ok());
    svc.start().await.unwrap();
    assert!(wait_registered(&svc, 1200).await);
}

#[tokio::test]
async fn test_reconnect_after_server_restart() {
    let (h1, sms_grpc_addr) = start_sms_server(None).await;
    let cfg = make_spearlet_config(sms_grpc_addr.clone());
    let svc = RegistrationService::new(cfg.clone(), sms_channel_lazy(&cfg).ok());
    svc.start().await.unwrap();
    assert!(wait_registered(&svc, 1200).await);

    // Stop server to simulate disconnect
    h1.abort();
    sleep(Duration::from_millis(300)).await;

    // Restart server
    let (_h2, _addr2) = start_sms_server(Some(sms_grpc_addr.clone())).await;

    // Wait bounded for reconnect + immediate re-register
    assert!(wait_registered(&svc, 1500).await);
}
