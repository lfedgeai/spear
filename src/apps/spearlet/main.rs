//! SPEARlet main entry point
//! SPEARlet 主入口点

use clap::Parser;
use spear_next::config::init_tracing;
use spear_next::spearlet::backend_reporter::BackendReporterService;
use spear_next::spearlet::config::CliArgs;
use spear_next::spearlet::controller::ControllerGroup;
use spear_next::spearlet::grpc_server::GrpcServer;
use spear_next::spearlet::http_gateway::HttpGateway;
use spear_next::spearlet::ai::credential_sync::CredentialSyncService;
use spear_next::spearlet::local_models::{global_managed_backends, LocalModelController};
use spear_next::spearlet::ai::remote_backend_sync::RemoteBackendSyncService;
use spear_next::spearlet::mcp::registry_sync::global_mcp_registry_sync_with_channel;
use spear_next::spearlet::ollama_discovery::maybe_import_ollama_serving_models;
use spear_next::spearlet::registration::RegistrationService;
use spear_next::spearlet::sms_connector::sms_channel_lazy;
use spear_next::spearlet::execution::ai::engine_holder::{init_global, EngineHolder};
use spear_next::spearlet::execution::ai::router::grpc_filter_stream::RouterFilterStreamHub;
use spear_next::spearlet::execution::ai::router::Router;
use spear_next::spearlet::execution::ai::AiEngine;
use spear_next::spearlet::execution::runtime::{ResourcePoolConfig, RuntimeConfig, RuntimeType};
use spear_next::spearlet::ai::RemoteBackendMergePolicy;
use tonic::transport::Channel;

use std::sync::Arc;

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
    if spearlet_cfg.ai.discovery.ollama.enabled {
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
    let base_cfg_for_remote_sync = Arc::new(spearlet_cfg.clone());

    let config = Arc::new(spearlet_cfg);
    let _ = spear_next::spearlet::ai::credential_resolver::init_global(&config);

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

    let env = spear_next::spearlet::ai::collect_ai_global_environment(&config);
    let runtime_config = RuntimeConfig {
        runtime_type: RuntimeType::Wasm,
        settings: std::collections::HashMap::new(),
        global_environment: env,
        spearlet_config: Some((*config).clone()),
        resource_pool: ResourcePoolConfig::default(),
    };
    let (registry, policy) =
        spear_next::spearlet::execution::ai::router::builder::build_registry_from_runtime_config(
            &runtime_config,
        );
    let grpc_filter_stream = runtime_config
        .spearlet_config
        .as_ref()
        .and_then(|cfg| {
            cfg.ai.router_grpc_filter_stream.clone().map(|mut f| {
                if f.enabled && f.addr.trim().is_empty() {
                    f.addr = cfg.sms_grpc_addr.clone();
                }
                f
            })
        })
        .map(RouterFilterStreamHub::init_global)
        .or_else(RouterFilterStreamHub::global)
        .filter(|h| h.config.enabled);
    let router = Router::new_with_filter(registry, policy, grpc_filter_stream);
    let engine = std::sync::Arc::new(AiEngine::new(router));
    let _holder = init_global(std::sync::Arc::new(EngineHolder::new(engine, 0)));

    let mut controllers = ControllerGroup::new();

    if let Some(ch) = sms_channel.clone() {
        let poll_ms = config.ai.remote_backend_sync.poll_interval_ms;
        let credential_sync =
            CredentialSyncService::new(ch.clone(), std::time::Duration::from_millis(poll_ms));
        controllers.register(credential_sync);

        if config.ai.remote_backend_sync.enabled {
            let merge_policy =
                RemoteBackendMergePolicy::parse(&config.ai.remote_backend_sync.merge_policy)
                    .unwrap_or(RemoteBackendMergePolicy::SmsWinsByName);
            let remote_backend_sync = RemoteBackendSyncService::new(
                base_cfg_for_remote_sync,
                ch,
                std::time::Duration::from_millis(poll_ms),
                merge_policy,
            );
            controllers.register(remote_backend_sync);
        }
    }

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
        controllers.register(std::sync::Arc::new(local_models));

        let backend_reporter = BackendReporterService::new(
            config.clone(),
            sms_channel.clone(),
            Some(managed_backends),
        );
        controllers.register(std::sync::Arc::new(backend_reporter));
    }

    controllers.start_all();

    tokio::signal::ctrl_c().await?;
    tracing::info!("SPEARlet shutting down");
    controllers.shutdown_all();
    let _ = shutdown_tx_grpc.send(());
    let _ = shutdown_tx_http.send(());
    let _ = grpc_handle.await;
    let _ = http_handle.await;

    tracing::info!("SPEARlet shutdown complete");
    Ok(())
}
