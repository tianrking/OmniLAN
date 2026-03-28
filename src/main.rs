mod app;
mod audit;
mod cli;
mod config;
mod enforcement;
mod engine;
mod gateway;
mod platform;
mod service;
mod state;

use anyhow::Result;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .compact()
        .init();

    let args = cli::Cli::parse_args();
    app::run(args).await
}
