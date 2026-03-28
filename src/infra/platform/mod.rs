use crate::domain::config::AppConfig;
use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

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
    cfg: &AppConfig,
    ip: &str,
    mac: Option<&str>,
) -> Result<PlatformApplyResult> {
    #[cfg(target_os = "linux")]
    {
        return apply_policy_target_linux(cfg, ip, mac);
    }
    #[cfg(target_os = "macos")]
    {
        return apply_policy_target_macos(cfg, ip, mac);
    }
    #[cfg(target_os = "windows")]
    {
        let mut out = PlatformApplyResult::default();
        out.notes.push(format!(
            "policy-route target {} requires WFP/HNS rules on Windows; adapter planned",
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

    let iface = detect_macos_interface(cfg)?;
    let (gateway_rules, policy_rules, combined_rules) = macos_anchor_paths(cfg);

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
    // reset policy file each run to avoid stale rules.
    fs::write(&policy_rules, "")
        .with_context(|| format!("failed to reset {}", policy_rules.display()))?;
    merge_macos_anchor_files(&gateway_rules, &policy_rules, &combined_rules)?;
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

#[cfg(target_os = "windows")]
fn apply_gateway_windows(_cfg: &AppConfig) -> Result<PlatformApplyResult> {
    let mut out = PlatformApplyResult::default();
    out.notes
        .push("Windows gateway adapter currently not enabled (WFP/HNS planned)".to_string());
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

#[cfg(target_os = "macos")]
fn apply_policy_target_macos(
    cfg: &AppConfig,
    ip: &str,
    mac: Option<&str>,
) -> Result<PlatformApplyResult> {
    let mut out = PlatformApplyResult::default();
    let iface = detect_macos_interface(cfg)?;
    let (gateway_rules, policy_rules, combined_rules) = macos_anchor_paths(cfg);

    let mut existing = fs::read_to_string(&policy_rules).unwrap_or_default();
    let mut lines = vec![
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
    for l in lines.drain(..) {
        if !existing.contains(&l) {
            existing.push_str(&l);
            existing.push('\n');
            let rollback = format!(
                "sed -i '' '/{}/d' {}",
                l.replace('/', "\\/"),
                policy_rules.display()
            );
            out.rollback_commands.push(rollback);
        }
    }
    fs::write(&policy_rules, existing)
        .with_context(|| format!("failed to write {}", policy_rules.display()))?;
    merge_macos_anchor_files(&gateway_rules, &policy_rules, &combined_rules)?;
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

#[cfg(target_os = "macos")]
fn detect_macos_interface(cfg: &AppConfig) -> Result<String> {
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

#[cfg(target_os = "macos")]
fn macos_anchor_paths(cfg: &AppConfig) -> (PathBuf, PathBuf, PathBuf) {
    (
        cfg.runtime.work_dir.join("pf.gateway.conf"),
        cfg.runtime.work_dir.join("pf.policy.conf"),
        cfg.runtime.work_dir.join("pf.anchor.conf"),
    )
}

#[cfg(target_os = "macos")]
fn merge_macos_anchor_files(
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

pub fn command_exists(cmd: &str) -> bool {
    which::which(cmd).is_ok()
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
