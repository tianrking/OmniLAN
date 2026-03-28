use crate::domain::config::AppConfig;
use anyhow::{Context, Result};
use chrono::Utc;
use serde_json::json;
use std::fs::{self, OpenOptions};
use std::io::Write;

pub fn log(cfg: &AppConfig, event: &str, detail: &str) -> Result<()> {
    if !cfg.security.enable_audit_log {
        return Ok(());
    }

    if let Some(parent) = cfg.runtime.audit_file.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create audit dir: {}", parent.display()))?;
    }

    let entry = json!({
        "ts": Utc::now().to_rfc3339(),
        "event": event,
        "detail": detail
    });

    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&cfg.runtime.audit_file)
        .with_context(|| {
            format!(
                "failed to open audit file: {}",
                cfg.runtime.audit_file.display()
            )
        })?;
    writeln!(f, "{entry}").context("failed to write audit entry")?;
    Ok(())
}
