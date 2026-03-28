use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "ok-proxy",
    version,
    about = "Modern multi-engine LAN proxy gateway"
)]
pub struct Cli {
    #[arg(short, long, default_value = "ok-proxy.yaml")]
    pub config: PathBuf,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Init,
    Validate,
    Render,
    Run,
    Status,
    Rollback,
}

impl Cli {
    pub fn parse_args() -> Self {
        Self::parse()
    }
}
