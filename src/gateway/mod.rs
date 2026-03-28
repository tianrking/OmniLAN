use crate::config::AppConfig;
use anyhow::{Context, Result};
use std::process::Command;
use tracing::info;

pub struct GatewayOrchestrator;

#[derive(Default)]
pub struct GatewayResult {
    pub rollback_commands: Vec<String>,
}

impl GatewayOrchestrator {
    pub fn apply(cfg: &AppConfig) -> Result<GatewayResult> {
        let mut out = GatewayResult::default();
        if cfg.gateway.auto_ip_forward {
            out.rollback_commands.extend(enable_ip_forward()?);
        }
        if cfg.gateway.auto_nat {
            out.rollback_commands.extend(setup_nat(cfg)?);
            out.rollback_commands.extend(setup_dns_hijack(cfg)?);
        }
        Ok(out)
    }
}

fn enable_ip_forward() -> Result<Vec<String>> {
    let mut rollback = vec![];
    #[cfg(target_os = "linux")]
    {
        run("sysctl", &["-w", "net.ipv4.ip_forward=1"])?;
        rollback.push("sysctl -w net.ipv4.ip_forward=0".to_string());
    }

    #[cfg(target_os = "macos")]
    {
        run("sysctl", &["-w", "net.inet.ip.forwarding=1"])?;
        rollback.push("sysctl -w net.inet.ip.forwarding=0".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        info!("windows ip forward setup should be handled by netsh; skipping automatic setup");
    }

    Ok(rollback)
}

fn setup_nat(cfg: &AppConfig) -> Result<Vec<String>> {
    #[allow(unused_mut)]
    let mut rollback = vec![];
    let _ = cfg;

    #[cfg(target_os = "linux")]
    {
        let cidr = cfg.gateway.lan_cidr.as_str();
        let check = Command::new("iptables")
            .args([
                "-t",
                "nat",
                "-C",
                "POSTROUTING",
                "-s",
                cidr,
                "-j",
                "MASQUERADE",
            ])
            .output();
        if check.is_err()
            || !check
                .context("failed to check existing iptables rule")?
                .status
                .success()
        {
            run(
                "iptables",
                &[
                    "-t",
                    "nat",
                    "-A",
                    "POSTROUTING",
                    "-s",
                    cidr,
                    "-j",
                    "MASQUERADE",
                ],
            )?;
            rollback.push(format!(
                "iptables -t nat -D POSTROUTING -s {} -j MASQUERADE",
                cidr
            ));
        }
    }

    #[cfg(target_os = "macos")]
    {
        info!(
            "macOS NAT is usually managed by pf; please ensure pf rules are configured if needed"
        );
    }

    #[cfg(target_os = "windows")]
    {
        info!("windows NAT can be managed via netsh/HNS; auto-NAT not implemented");
    }

    Ok(rollback)
}

fn setup_dns_hijack(cfg: &AppConfig) -> Result<Vec<String>> {
    #[allow(unused_mut)]
    let mut rollback = vec![];
    let _ = cfg;

    #[cfg(target_os = "linux")]
    {
        if !matches!(cfg.engine, crate::config::EngineKind::Mihomo) {
            return Ok(rollback);
        }
        let cidr = cfg.gateway.lan_cidr.as_str();
        let dns_port = cfg.inbound.dns_port.to_string();
        for proto in ["udp", "tcp"] {
            let check = Command::new("iptables")
                .args([
                    "-t",
                    "nat",
                    "-C",
                    "PREROUTING",
                    "-s",
                    cidr,
                    "-p",
                    proto,
                    "--dport",
                    "53",
                    "-j",
                    "REDIRECT",
                    "--to-ports",
                    dns_port.as_str(),
                ])
                .output();
            if check.is_err()
                || !check
                    .context("failed to check existing dns hijack rule")?
                    .status
                    .success()
            {
                let status = Command::new("iptables")
                    .args([
                        "-t",
                        "nat",
                        "-A",
                        "PREROUTING",
                        "-s",
                        cidr,
                        "-p",
                        proto,
                        "--dport",
                        "53",
                        "-j",
                        "REDIRECT",
                        "--to-ports",
                        dns_port.as_str(),
                    ])
                    .status()
                    .context("failed to apply dns hijack rule")?;
                if !status.success() {
                    anyhow::bail!("failed to apply dns hijack rule for proto={}", proto);
                }
                rollback.push(format!(
                    "iptables -t nat -D PREROUTING -s {} -p {} --dport 53 -j REDIRECT --to-ports {}",
                    cidr, proto, cfg.inbound.dns_port
                ));
            }
        }
    }

    Ok(rollback)
}

fn run(cmd: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(cmd)
        .args(args)
        .status()
        .with_context(|| format!("failed to run command: {} {:?}", cmd, args))?;
    if !status.success() {
        anyhow::bail!("command failed: {} {:?}", cmd, args);
    }
    Ok(())
}
