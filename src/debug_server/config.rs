use clap::Parser;

pub const DEFAULT_SESSION: &str = "default-session";
pub const DEFAULT_CLIENT: &str = "default-client";
pub const DEFAULT_STREAM: &str = "default-stream";

#[derive(Debug, Clone, Parser)]
pub struct DebugServerConfig {
    #[arg(long, default_value = "0.0.0.0")]
    pub host: String,
    #[arg(long, default_value_t = 7777)]
    pub port: u16,
    #[arg(long, default_value = DEFAULT_SESSION)]
    pub session: String,
    #[arg(long)]
    pub outdir: String,
    #[arg(long, default_value_t = 1200)]
    pub idle: u64,
}
