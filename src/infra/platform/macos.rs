use crate::domain::config::AppConfig;
use crate::infra::platform::PlatformApplyResult;
use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

pub fn apply_gateway(cfg: &AppConfig) -> Result<PlatformApplyResult> {
    let mut out = PlatformApplyResult::default();
    if cfg.gateway.auto_ip_forward {
        run_cmd("sysctl", &["-w", "net.inet.ip.forwarding=1"])?;
        out.rollback_commands
            .push("sysctl -w net.inet.ip.forwarding=0".to_string());
    }

    let iface = detect_interface(cfg)?;
    let (gateway_rules, policy_rules, combined_rules) = anchor_paths(cfg);

    let mut rules = vec![];
    if cfg.gateway.auto_nat {
        rules.push(format!(
            "nat on {} from {} to any -> ({})",
            iface, cfg.gateway.lan_cidr, iface
        ));
        rules.push(format!(
            "rdr pass on {} inet proto {{ tcp udp }} from {} to any port 53 -> 127.0.0.1 port {}",
            iface, cfg.gateway.lan_cidr, cfg.inbound.dns_port
        ));
    }
    fs::create_dir_all(&cfg.runtime.work_dir)
        .with_context(|| format!("failed to create {}", cfg.runtime.work_dir.display()))?;
    fs::write(&gateway_rules, rules.join("\n") + "\n")
        .with_context(|| format!("failed to write {}", gateway_rules.display()))?;
    fs::write(&policy_rules, "")
        .with_context(|| format!("failed to reset {}", policy_rules.display()))?;
    merge_anchor_files(&gateway_rules, &policy_rules, &combined_rules)?;
    run_cmd("pfctl", &["-E"])?;
    run_cmd(
        "pfctl",
        &[
            "-a",
            "com.omnilan",
            "-f",
            combined_rules.to_string_lossy().as_ref(),
        ],
    )?;
    out.rollback_commands
        .push("pfctl -a com.omnilan -F all".to_string());
    out.notes.push(format!(
        "pf anchor loaded: {} (iface={})",
        combined_rules.display(),
        iface
    ));
    Ok(out)
}

pub fn apply_policy_target(
    cfg: &AppConfig,
    ip: &str,
    mac: Option<&str>,
) -> Result<PlatformApplyResult> {
    let mut out = PlatformApplyResult::default();
    let iface = detect_interface(cfg)?;
    let (gateway_rules, policy_rules, combined_rules) = anchor_paths(cfg);

    let mut existing = fs::read_to_string(&policy_rules).unwrap_or_default();
    let rules = vec![
        format!(
            "rdr pass on {} inet proto tcp from {} to any -> 127.0.0.1 port {}",
            iface, ip, cfg.inbound.redir_port
        ),
        format!(
            "rdr pass on {} inet proto {{ tcp udp }} from {} to any port 53 -> 127.0.0.1 port {}",
            iface, ip, cfg.inbound.dns_port
        ),
    ];
    if mac.is_some() {
        out.notes.push(format!(
            "macOS pf backend ignores MAC matching for {}, using IP-based policy",
            ip
        ));
    }
    for r in rules {
        if !existing.contains(&r) {
            existing.push_str(&r);
            existing.push('\n');
            out.rollback_commands.push(format!(
                "sed -i '' '/{}/d' {}",
                r.replace('/', "\\/"),
                policy_rules.display()
            ));
        }
    }
    fs::write(&policy_rules, existing)
        .with_context(|| format!("failed to write {}", policy_rules.display()))?;
    merge_anchor_files(&gateway_rules, &policy_rules, &combined_rules)?;
    run_cmd(
        "pfctl",
        &[
            "-a",
            "com.omnilan",
            "-f",
            combined_rules.to_string_lossy().as_ref(),
        ],
    )?;
    out.notes
        .push(format!("policy-route applied via pf for {}", ip));
    Ok(out)
}

fn detect_interface(cfg: &AppConfig) -> Result<String> {
    if let Some(iface) = &cfg.gateway.interface {
        return Ok(iface.clone());
    }
    let out = Command::new("sh")
        .arg("-c")
        .arg("route -n get default | awk '/interface:/{print $2}'")
        .output()
        .context("failed to detect macOS default interface")?;
    let iface = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if iface.is_empty() {
        anyhow::bail!("unable to detect default interface on macOS");
    }
    Ok(iface)
}

fn anchor_paths(cfg: &AppConfig) -> (PathBuf, PathBuf, PathBuf) {
    (
        cfg.runtime.work_dir.join("pf.gateway.conf"),
        cfg.runtime.work_dir.join("pf.policy.conf"),
        cfg.runtime.work_dir.join("pf.anchor.conf"),
    )
}

fn merge_anchor_files(
    gateway_rules: &PathBuf,
    policy_rules: &PathBuf,
    combined: &PathBuf,
) -> Result<()> {
    let a = fs::read_to_string(gateway_rules).unwrap_or_default();
    let b = fs::read_to_string(policy_rules).unwrap_or_default();
    fs::write(combined, format!("{}\n{}", a, b))
        .with_context(|| format!("failed to write {}", combined.display()))?;
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
