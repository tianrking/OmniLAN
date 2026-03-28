use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "omnilan",
    version,
    about = "Modern dual-core LAN proxy gateway"
)]
pub struct Cli {
    #[arg(short, long, default_value = "omnilan.yaml")]
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
    Stop {
        #[arg(long, default_value_t = false)]
        rollback: bool,
    },
    Status,
    Audit,
    Doctor,
    ServiceInstall,
    ServiceUninstall,
    Rollback,
}

impl Cli {
    pub fn parse_args() -> Self {
        Self::parse()
    }
}
