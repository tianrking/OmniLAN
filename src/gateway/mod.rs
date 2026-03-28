use crate::config::AppConfig;
use crate::platform;
use anyhow::Result;

pub struct GatewayOrchestrator;

#[derive(Default)]
pub struct GatewayResult {
    pub rollback_commands: Vec<String>,
    pub notes: Vec<String>,
}

impl GatewayOrchestrator {
    pub fn apply(cfg: &AppConfig) -> Result<GatewayResult> {
        let result = platform::apply_gateway(cfg)?;
        Ok(GatewayResult {
            rollback_commands: result.rollback_commands,
            notes: result.notes,
        })
    }
}
