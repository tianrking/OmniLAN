mod mihomo;
mod singbox;

use crate::domain::config::{AppConfig, EngineKind};
use anyhow::Result;
use std::path::PathBuf;
use tokio::process::Command;

pub use mihomo::MihomoEngine;
pub use singbox::SingBoxEngine;

pub struct EngineArtifacts {
    pub render_path: PathBuf,
}

pub trait Engine {
    fn name(&self) -> &'static str;
    fn ensure_binary(&self, cfg: &AppConfig) -> Result<()>;
    fn prepare(&self, cfg: &AppConfig) -> Result<EngineArtifacts>;
    fn command(&self, cfg: &AppConfig, artifacts: &EngineArtifacts) -> Command;
}

pub fn from_config(cfg: &AppConfig) -> Box<dyn Engine + Send + Sync> {
    match cfg.engine {
        EngineKind::Mihomo => Box::new(MihomoEngine),
        EngineKind::SingBox => Box::new(SingBoxEngine),
    }
}
