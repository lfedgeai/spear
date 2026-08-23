mod config;
mod routes;
mod state;
mod ui;

use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use tokio::net::TcpListener;
use tracing::info;

use self::config::DebugServerConfig;
use self::state::DebugServerState;

pub async fn run() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "debug_server=info".to_string()),
        )
        .with_target(false)
        .compact()
        .try_init()
        .ok();

    let config = DebugServerConfig::parse();
    let state = Arc::new(DebugServerState::new(config.clone())?);
    state.write_env("debug-server")?;

    let listener = TcpListener::bind((config.host.as_str(), config.port)).await?;
    info!(
        host = %config.host,
        port = config.port,
        session = %config.session,
        "debug server listening"
    );

    let app = routes::router(state.clone());
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(state, config.idle))
        .await?;
    Ok(())
}

async fn shutdown_signal(state: Arc<DebugServerState>, idle_secs: u64) {
    if idle_secs == 0 {
        std::future::pending::<()>().await;
        return;
    }

    loop {
        tokio::time::sleep(Duration::from_secs(2)).await;
        if state.idle_for() > Duration::from_secs(idle_secs) {
            break;
        }
    }
}
