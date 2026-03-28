use crate::domain::config::AppConfig;
use crate::infra::platform::PlatformApplyResult;
use anyhow::Result;

pub fn apply_gateway(_cfg: &AppConfig) -> Result<PlatformApplyResult> {
    let mut out = PlatformApplyResult::default();
    out.notes
        .push("Windows gateway adapter currently not enabled (WFP/HNS planned)".to_string());
    Ok(out)
}

pub fn apply_policy_target(
    _cfg: &AppConfig,
    ip: &str,
    _mac: Option<&str>,
) -> Result<PlatformApplyResult> {
    let mut out = PlatformApplyResult::default();
    out.notes.push(format!(
        "policy-route target {} requires WFP/HNS rules on Windows; adapter planned",
        ip
    ));
    Ok(out)
}
