//! SPEARlet main entry point
//! SPEARlet 主入口点

use clap::Parser;
use spear_next::config::init_tracing;
use spear_next::spearlet::backend_reporter::BackendReporterService;
use spear_next::spearlet::config::CliArgs;
use spear_next::spearlet::grpc_server::GrpcServer;
use spear_next::spearlet::http_gateway::HttpGateway;
use spear_next::spearlet::local_models::{global_managed_backends, LocalModelController};
use spear_next::spearlet::mcp::registry_sync::global_mcp_registry_sync_with_channel;
use spear_next::spearlet::ollama_discovery::maybe_import_ollama_serving_models;
use spear_next::spearlet::registration::RegistrationService;
use spear_next::spearlet::sms_connector::sms_channel_lazy;
use tonic::transport::Channel;

use std::sync::Arc;

// Import remote backends managed by SMS Web Admin / 导入由SMS Web Admin管理的远端 backends
async fn maybe_import_sms_admin_remote_backends(
    cfg: &mut spear_next::spearlet::config::SpearletConfig,
    sms_channel: Option<Channel>,
) {
    use spear_next::proto::sms::admin_llm_config_service_client::AdminLlmConfigServiceClient;
    use spear_next::proto::sms::ListRemoteBackendsRequest;
    use std::collections::HashMap;

    let Some(channel) = sms_channel else {
        return;
    };

    let mut client = AdminLlmConfigServiceClient::new(channel);
    let resp = match client
        .list_remote_backends(ListRemoteBackendsRequest {})
        .await
    {
        Ok(r) => r.into_inner(),
        Err(e) => {
            tracing::warn!(error = %e, "List remote backends from SMS failed");
            return;
        }
    };

    if resp.backends.is_empty() {
        return;
    }

    let mut by_name: HashMap<String, spear_next::spearlet::config::LlmBackendConfig> = cfg
        .llm
        .backends
        .iter()
        .cloned()
        .map(|b| (b.name.clone(), b))
        .collect();

    let imported = resp.backends.len();
    for b in resp.backends {
        if b.name.trim().is_empty() {
            continue;
        }
        let model = if b.model.trim().is_empty() {
            None
        } else {
            Some(b.model)
        };
        let credential_ref = if b.credential_ref.trim().is_empty() {
            None
        } else {
            Some(b.credential_ref)
        };
        by_name.insert(
            b.name.clone(),
            spear_next::spearlet::config::LlmBackendConfig {
                name: b.name,
                kind: b.kind,
                base_url: b.base_url,
                hosting: Some("remote".to_string()),
                model,
                credential_ref,
                weight: b.weight,
                priority: b.priority,
                ops: b.operations,
                features: b.features,
                transports: b.transports,
            },
        );
    }

    cfg.llm.backends = by_name.into_values().collect();
    cfg.llm
        .backends
        .sort_by(|a, b| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()));

    tracing::info!(
        remote_backends = imported,
        "Imported SMS Web Admin remote backends (restart SPEARlet to pick up later changes)"
    );
}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args = CliArgs::parse();
    let log_args = format!("{:?}", args);

    let app_cfg = spear_next::spearlet::config::AppConfig::load_with_cli(&args)?;
    let spearlet_cfg = app_cfg.spearlet;

    init_tracing(&spearlet_cfg.logging.to_logging_config()).unwrap();

    let max_blocking_threads = spearlet_cfg.max_blocking_threads.max(1);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(max_blocking_threads)
        .build()?;

    runtime.block_on(run(args, log_args, spearlet_cfg))
}

async fn run(
    args: CliArgs,
    log_args: String,
    mut spearlet_cfg: spear_next::spearlet::config::SpearletConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if spearlet_cfg.llm.discovery.ollama.enabled {
        match maybe_import_ollama_serving_models(&mut spearlet_cfg).await {
            Ok(n) => {
                if n > 0 {
                    tracing::info!(imported = n, "Imported Ollama serving models");
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "Ollama model import failed");
            }
        }
    }

    let sms_channel = if spearlet_cfg.sms_grpc_addr.trim().is_empty() {
        None
    } else {
        Some(sms_channel_lazy(&spearlet_cfg)?)
    };
    maybe_import_sms_admin_remote_backends(&mut spearlet_cfg, sms_channel.clone()).await;

    let config = Arc::new(spearlet_cfg);

    tracing::info!("Starting SPEARlet with args: {}", log_args);

    tracing::info!("SPEARlet starting with:");
    tracing::info!("  - gRPC server on: {}", config.grpc.addr);
    tracing::info!("  - HTTP gateway on: {}", config.http.server.addr);
    tracing::info!("  - SMS gRPC target at: {}", config.sms_grpc_addr);
    tracing::info!("  - Node Name: {}", config.node_name);
    tracing::info!("  - Storage backend: {:?}", config.storage.backend);
    tracing::info!("  - Auto register: {}", config.auto_register);

    let sms_channel = sms_channel;

    global_mcp_registry_sync_with_channel(config.clone(), sms_channel.clone());

    let grpc_server = GrpcServer::new(config.clone(), sms_channel.clone()).await?;
    let (shutdown_tx_grpc, shutdown_rx_grpc) = tokio::sync::oneshot::channel::<()>();

    let object_service = grpc_server.get_object_service();
    let function_service = grpc_server.get_function_service();
    let health_service = spear_next::spearlet::grpc_server::HealthService::new(
        object_service,
        function_service.clone(),
    );

    let grpc_handle = tokio::spawn(async move {
        if let Err(e) = grpc_server
            .start_with_shutdown(async move {
                let _ = shutdown_rx_grpc.await;
            })
            .await
        {
            tracing::error!("gRPC server error: {}", e);
        }
    });

    let grpc_channel = Channel::from_shared(format!("http://{}", config.grpc.addr))?.connect_lazy();

    let http_gateway = HttpGateway::new(
        config.clone(),
        Arc::new(health_service),
        function_service.clone(),
        spear_next::proto::spearlet::object_service_client::ObjectServiceClient::new(
            grpc_channel.clone(),
        ),
        spear_next::proto::spearlet::invocation_service_client::InvocationServiceClient::new(
            grpc_channel.clone(),
        ),
        spear_next::proto::spearlet::execution_service_client::ExecutionServiceClient::new(
            grpc_channel,
        ),
    );
    let (shutdown_tx_http, shutdown_rx_http) = tokio::sync::oneshot::channel::<()>();
    let http_handle = tokio::spawn(async move {
        if let Err(e) = http_gateway
            .start_with_shutdown(async move {
                let _ = shutdown_rx_http.await;
            })
            .await
        {
            tracing::error!("HTTP gateway error: {}", e);
        }
    });

    let connect_requested = config.auto_register
        || args.sms_grpc_addr.is_some()
        || std::env::var("SPEARLET_SMS_GRPC_ADDR")
            .ok()
            .map(|v| !v.is_empty())
            .unwrap_or(false);
    if connect_requested {
        let managed_backends = global_managed_backends();
        let registration_service = RegistrationService::new(config.clone(), sms_channel.clone());
        if let Err(e) = registration_service.start().await {
            tracing::error!("Registration service start failed: {}", e);
            return Err(e);
        }
        tracing::info!(
            "Registration service started (heartbeat every {}s)",
            config.heartbeat_interval
        );
        let execution_manager = function_service.get_execution_manager();
        let subscriber = spear_next::spearlet::task_events::TaskEventSubscriber::new(
            config.clone(),
            sms_channel.clone(),
            execution_manager,
        );
        subscriber.start().await;

        let local_models = LocalModelController::new(
            config.clone(),
            sms_channel.clone(),
            managed_backends.clone(),
        );
        local_models.start();

        let backend_reporter = BackendReporterService::new(
            config.clone(),
            sms_channel.clone(),
            Some(managed_backends),
        );
        backend_reporter.start();
    }

    tokio::signal::ctrl_c().await?;
    tracing::info!("SPEARlet shutting down");
    let _ = shutdown_tx_grpc.send(());
    let _ = shutdown_tx_http.send(());
    let _ = grpc_handle.await;
    let _ = http_handle.await;

    tracing::info!("SPEARlet shutdown complete");
    Ok(())
}
