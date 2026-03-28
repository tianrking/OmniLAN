use crate::domain::config::AppConfig;
use anyhow::Result;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[derive(Default, Debug, Clone)]
pub struct PlatformApplyResult {
    pub rollback_commands: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PlatformCapabilities {
    pub os: &'static str,
    pub gateway: bool,
    pub policy_route_ip: bool,
    pub policy_route_mac: bool,
    pub service: bool,
}

pub fn capabilities() -> PlatformCapabilities {
    #[cfg(target_os = "linux")]
    {
        return PlatformCapabilities {
            os: "linux",
            gateway: true,
            policy_route_ip: true,
            policy_route_mac: true,
            service: true,
        };
    }
    #[cfg(target_os = "macos")]
    {
        return PlatformCapabilities {
            os: "macos",
            gateway: true,
            policy_route_ip: true,
            policy_route_mac: false,
            service: true,
        };
    }
    #[cfg(target_os = "windows")]
    {
        return PlatformCapabilities {
            os: "windows",
            gateway: false,
            policy_route_ip: false,
            policy_route_mac: false,
            service: true,
        };
    }
    #[allow(unreachable_code)]
    PlatformCapabilities {
        os: "unknown",
        gateway: false,
        policy_route_ip: false,
        policy_route_mac: false,
        service: false,
    }
}

pub fn apply_gateway(cfg: &AppConfig) -> Result<PlatformApplyResult> {
    #[cfg(target_os = "linux")]
    {
        return linux::apply_gateway(cfg);
    }
    #[cfg(target_os = "macos")]
    {
        return macos::apply_gateway(cfg);
    }
    #[cfg(target_os = "windows")]
    {
        return windows::apply_gateway(cfg);
    }
    #[allow(unreachable_code)]
    Ok(PlatformApplyResult::default())
}

pub fn apply_policy_target(
    cfg: &AppConfig,
    ip: &str,
    mac: Option<&str>,
) -> Result<PlatformApplyResult> {
    #[cfg(target_os = "linux")]
    {
        return linux::apply_policy_target(cfg, ip, mac);
    }
    #[cfg(target_os = "macos")]
    {
        return macos::apply_policy_target(cfg, ip, mac);
    }
    #[cfg(target_os = "windows")]
    {
        return windows::apply_policy_target(cfg, ip, mac);
    }
    #[allow(unreachable_code)]
    Ok(PlatformApplyResult::default())
}

pub fn command_exists(cmd: &str) -> bool {
    which::which(cmd).is_ok()
}
