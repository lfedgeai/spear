#[path = "../../debug_server/mod.rs"]
mod debug_server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    debug_server::run().await
}
