use crate::domain::config::AppConfig;
use crate::infra::platform::PlatformApplyResult;
use anyhow::{Context, Result};
use std::process::Command;

pub fn apply_gateway(cfg: &AppConfig) -> Result<PlatformApplyResult> {
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

pub fn apply_policy_target(
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

fn run_sh(script: &str) -> Result<()> {
    let status = run_sh_status(script)?;
    if !status.success() {
        anyhow::bail!("command failed: {}", script);
    }
    Ok(())
}

fn run_sh_status(script: &str) -> Result<std::process::ExitStatus> {
    Command::new("sh")
        .arg("-c")
        .arg(script)
        .status()
        .with_context(|| format!("failed to run shell command: {}", script))
}
