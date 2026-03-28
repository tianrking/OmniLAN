use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeState {
    pub started_at: DateTime<Utc>,
    pub engine: String,
    pub pid: u32,
    pub child_pid: Option<u32>,
    pub config_path: String,
    pub render_path: String,
    pub rollback_script: String,
    pub audit_file: String,
}

impl RuntimeState {
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create dir: {}", parent.display()))?;
        }
        let raw =
            serde_json::to_string_pretty(self).context("failed to serialize runtime state")?;
        fs::write(path, raw)
            .with_context(|| format!("failed to write state: {}", path.display()))?;
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read state: {}", path.display()))?;
        let state: RuntimeState =
            serde_json::from_str(&raw).context("failed to parse runtime state json")?;
        Ok(state)
    }
}
