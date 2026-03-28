use crate::config::AppConfig;
use anyhow::{Context, Result};
use std::process::Command;

#[derive(Default)]
pub struct PlatformApplyResult {
    pub rollback_commands: Vec<String>,
    pub notes: Vec<String>,
}

pub fn apply_gateway(cfg: &AppConfig) -> Result<PlatformApplyResult> {
    #[cfg(target_os = "linux")]
    {
        return apply_gateway_linux(cfg);
    }
    #[cfg(target_os = "macos")]
    {
        return apply_gateway_macos(cfg);
    }
    #[cfg(target_os = "windows")]
    {
        return apply_gateway_windows(cfg);
    }
    #[allow(unreachable_code)]
    Ok(PlatformApplyResult::default())
}

pub fn apply_policy_target(
    _cfg: &AppConfig,
    ip: &str,
    _mac: Option<&str>,
) -> Result<PlatformApplyResult> {
    #[cfg(target_os = "linux")]
    {
        return apply_policy_target_linux(_cfg, ip, _mac);
    }
    #[cfg(target_os = "macos")]
    {
        let mut out = PlatformApplyResult::default();
        out.notes.push(format!(
            "policy-route target {} requires pf anchor rules on macOS; adapter reserved",
            ip
        ));
        return Ok(out);
    }
    #[cfg(target_os = "windows")]
    {
        let mut out = PlatformApplyResult::default();
        out.notes.push(format!(
            "policy-route target {} requires WFP/HNS rules on Windows; adapter reserved",
            ip
        ));
        return Ok(out);
    }
    #[allow(unreachable_code)]
    Ok(PlatformApplyResult::default())
}

#[cfg(target_os = "linux")]
fn apply_gateway_linux(cfg: &AppConfig) -> Result<PlatformApplyResult> {
    let mut out = PlatformApplyResult::default();
    if cfg.gateway.auto_ip_forward {
        run_sh("sysctl -w net.ipv4.ip_forward=1")?;
        out.rollback_commands
            .push("sysctl -w net.ipv4.ip_forward=0".to_string());
    }
    if cfg.gateway.auto_nat {
        let cidr = &cfg.gateway.lan_cidr;
        maybe_add_rule(
            &format!("iptables -t nat -C POSTROUTING -s {} -j MASQUERADE", cidr),
            &format!("iptables -t nat -A POSTROUTING -s {} -j MASQUERADE", cidr),
            &format!("iptables -t nat -D POSTROUTING -s {} -j MASQUERADE", cidr),
            &mut out.rollback_commands,
        )?;
        let dns_port = cfg.inbound.dns_port;
        for proto in ["udp", "tcp"] {
            maybe_add_rule(
                &format!(
                    "iptables -t nat -C PREROUTING -s {} -p {} --dport 53 -j REDIRECT --to-ports {}",
                    cidr, proto, dns_port
                ),
                &format!(
                    "iptables -t nat -A PREROUTING -s {} -p {} --dport 53 -j REDIRECT --to-ports {}",
                    cidr, proto, dns_port
                ),
                &format!(
                    "iptables -t nat -D PREROUTING -s {} -p {} --dport 53 -j REDIRECT --to-ports {}",
                    cidr, proto, dns_port
                ),
                &mut out.rollback_commands,
            )?;
        }
    }
    Ok(out)
}

#[cfg(target_os = "macos")]
fn apply_gateway_macos(cfg: &AppConfig) -> Result<PlatformApplyResult> {
    let mut out = PlatformApplyResult::default();
    if cfg.gateway.auto_ip_forward {
        run_cmd("sysctl", &["-w", "net.inet.ip.forwarding=1"])?;
        out.rollback_commands
            .push("sysctl -w net.inet.ip.forwarding=0".to_string());
    }
    out.notes.push(
        "macOS NAT/policy route should be applied via pf anchor (planned adapter)".to_string(),
    );
    Ok(out)
}

#[cfg(target_os = "windows")]
fn apply_gateway_windows(_cfg: &AppConfig) -> Result<PlatformApplyResult> {
    let mut out = PlatformApplyResult::default();
    out.notes
        .push("Windows adapter: enable forwarding/NAT via netsh/HNS (planned)".to_string());
    Ok(out)
}

#[cfg(target_os = "linux")]
fn apply_policy_target_linux(
    cfg: &AppConfig,
    ip: &str,
    mac: Option<&str>,
) -> Result<PlatformApplyResult> {
    let mut out = PlatformApplyResult::default();
    let redir_port = cfg.inbound.redir_port;
    let dns_port = cfg.inbound.dns_port;

    maybe_add_rule(
        &format!(
            "iptables -t nat -C PREROUTING -s {} -p tcp -j REDIRECT --to-ports {}",
            ip, redir_port
        ),
        &format!(
            "iptables -t nat -A PREROUTING -s {} -p tcp -j REDIRECT --to-ports {}",
            ip, redir_port
        ),
        &format!(
            "iptables -t nat -D PREROUTING -s {} -p tcp -j REDIRECT --to-ports {}",
            ip, redir_port
        ),
        &mut out.rollback_commands,
    )?;

    for proto in ["udp", "tcp"] {
        maybe_add_rule(
            &format!(
                "iptables -t nat -C PREROUTING -s {} -p {} --dport 53 -j REDIRECT --to-ports {}",
                ip, proto, dns_port
            ),
            &format!(
                "iptables -t nat -A PREROUTING -s {} -p {} --dport 53 -j REDIRECT --to-ports {}",
                ip, proto, dns_port
            ),
            &format!(
                "iptables -t nat -D PREROUTING -s {} -p {} --dport 53 -j REDIRECT --to-ports {}",
                ip, proto, dns_port
            ),
            &mut out.rollback_commands,
        )?;
    }

    if let Some(mac) = mac {
        maybe_add_rule(
            &format!(
                "iptables -t nat -C PREROUTING -m mac --mac-source {} -p tcp -j REDIRECT --to-ports {}",
                mac, redir_port
            ),
            &format!(
                "iptables -t nat -A PREROUTING -m mac --mac-source {} -p tcp -j REDIRECT --to-ports {}",
                mac, redir_port
            ),
            &format!(
                "iptables -t nat -D PREROUTING -m mac --mac-source {} -p tcp -j REDIRECT --to-ports {}",
                mac, redir_port
            ),
            &mut out.rollback_commands,
        )?;
        for proto in ["udp", "tcp"] {
            maybe_add_rule(
                &format!(
                    "iptables -t nat -C PREROUTING -m mac --mac-source {} -p {} --dport 53 -j REDIRECT --to-ports {}",
                    mac, proto, dns_port
                ),
                &format!(
                    "iptables -t nat -A PREROUTING -m mac --mac-source {} -p {} --dport 53 -j REDIRECT --to-ports {}",
                    mac, proto, dns_port
                ),
                &format!(
                    "iptables -t nat -D PREROUTING -m mac --mac-source {} -p {} --dport 53 -j REDIRECT --to-ports {}",
                    mac, proto, dns_port
                ),
                &mut out.rollback_commands,
            )?;
        }
    }

    Ok(out)
}

#[cfg(target_os = "linux")]
fn maybe_add_rule(
    check: &str,
    add: &str,
    rollback: &str,
    rollbacks: &mut Vec<String>,
) -> Result<()> {
    if !run_sh_status(check)?.success() {
        run_sh(add)?;
        rollbacks.push(rollback.to_string());
    }
    Ok(())
}

fn run_cmd(cmd: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(cmd)
        .args(args)
        .status()
        .with_context(|| format!("failed to run command: {} {:?}", cmd, args))?;
    if !status.success() {
        anyhow::bail!("command failed: {} {:?}", cmd, args);
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn run_sh(script: &str) -> Result<()> {
    let status = run_sh_status(script)?;
    if !status.success() {
        anyhow::bail!("command failed: {}", script);
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn run_sh_status(script: &str) -> Result<std::process::ExitStatus> {
    Command::new("sh")
        .arg("-c")
        .arg(script)
        .status()
        .with_context(|| format!("failed to run shell command: {}", script))
}
