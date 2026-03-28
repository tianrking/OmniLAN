use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "omnilan",
    version,
    about = "Modern dual-core LAN proxy gateway"
)]
pub struct Cli {
    #[arg(short, long, default_value = "omnilan.yaml", global = true)]
    pub config: PathBuf,

    #[arg(long, value_enum, global = true)]
    pub engine: Option<EngineArg>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum EngineArg {
    Mihomo,
    SingBox,
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
    KernelInstall,
    ServiceInstall,
    ServiceUninstall,
    Rollback,
}

impl Cli {
    pub fn parse_args() -> Self {
        Self::parse()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parse_engine_override() {
        let cli = Cli::parse_from(["omnilan", "--engine", "sing-box", "status"]);
        assert!(matches!(cli.engine, Some(EngineArg::SingBox)));
    }
}
