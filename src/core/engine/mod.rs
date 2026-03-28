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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::config::{AppConfig, EngineKind, ProxySourceMode};
    use tempfile::tempdir;

    #[test]
    fn mihomo_prepare_renders_file() {
        let dir = tempdir().expect("tempdir");
        let mut cfg = AppConfig {
            engine: EngineKind::Mihomo,
            ..AppConfig::default()
        };
        cfg.runtime.work_dir = dir.path().join("work");
        cfg.runtime.state_file = cfg.runtime.work_dir.join("state.json");
        cfg.runtime.audit_file = cfg.runtime.work_dir.join("audit.log");
        cfg.source.mode = ProxySourceMode::Subscription;
        cfg.source.subscription_url = Some("https://example.com/sub".to_string());

        let engine = MihomoEngine;
        let artifacts = engine.prepare(&cfg).expect("prepare");
        let rendered = std::fs::read_to_string(&artifacts.render_path).expect("read render");
        assert!(rendered.contains("mixed-port"));
        assert!(rendered.contains("proxy-providers"));
    }

    #[test]
    fn singbox_prepare_renders_file() {
        let dir = tempdir().expect("tempdir");
        let mut cfg = AppConfig {
            engine: EngineKind::SingBox,
            ..AppConfig::default()
        };
        cfg.runtime.work_dir = dir.path().join("work");
        cfg.runtime.state_file = cfg.runtime.work_dir.join("state.json");
        cfg.runtime.audit_file = cfg.runtime.work_dir.join("audit.log");
        cfg.source.mode = ProxySourceMode::Subscription;
        cfg.source.subscription_url = Some("https://example.com/sub".to_string());

        let engine = SingBoxEngine;
        let artifacts = engine.prepare(&cfg).expect("prepare");
        let rendered = std::fs::read_to_string(&artifacts.render_path).expect("read render");
        assert!(rendered.contains("\"inbounds\""));
        assert!(rendered.contains("\"route\""));
    }
}
