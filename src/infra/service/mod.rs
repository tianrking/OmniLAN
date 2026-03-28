use crate::domain::config::AppConfig;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

pub fn install(cfg: &AppConfig, config_path: &Path) -> Result<String> {
    let managed_config = cfg.runtime.work_dir.join("omnilan.yaml");
    if let Some(parent) = managed_config.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::copy(config_path, &managed_config).with_context(|| {
        format!(
            "failed to copy config {} -> {}",
            config_path.display(),
            managed_config.display()
        )
    })?;

    #[cfg(target_os = "linux")]
    {
        let unit = render_systemd_unit(cfg, &managed_config);
        let path = "/etc/systemd/system/omnilan.service";
        fs::write(path, unit).context("failed to write systemd unit")?;
        run("systemctl", &["daemon-reload"])?;
        run("systemctl", &["enable", "--now", "omnilan.service"])?;
        return Ok(path.to_string());
    }
    #[cfg(target_os = "macos")]
    {
        let plist = render_launchd_plist(cfg, &managed_config);
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let path = home.join("Library/LaunchAgents/com.omnilan.agent.plist");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).context("failed to create launchd dir")?;
        }
        fs::write(&path, plist).context("failed to write launchd plist")?;
        run("launchctl", &["unload", path.to_string_lossy().as_ref()]).ok();
        run(
            "launchctl",
            &["load", "-w", path.to_string_lossy().as_ref()],
        )?;
        return Ok(path.display().to_string());
    }
    #[cfg(target_os = "windows")]
    {
        let task_name = "OmniLAN";
        let exe = std::env::current_exe().context("failed to resolve current exe")?;
        let tr = format!(
            "\"{}\" run -c \"{}\"",
            exe.display(),
            managed_config.display()
        );
        run(
            "schtasks",
            &[
                "/Create", "/SC", "ONSTART", "/RL", "HIGHEST", "/TN", task_name, "/TR", &tr, "/F",
            ],
        )?;
        return Ok(format!("ScheduledTask:{}", task_name));
    }
    #[allow(unreachable_code)]
    Ok("unsupported".to_string())
}

pub fn uninstall() -> Result<String> {
    #[cfg(target_os = "linux")]
    {
        run("systemctl", &["disable", "--now", "omnilan.service"]).ok();
        run("rm", &["-f", "/etc/systemd/system/omnilan.service"])?;
        run("systemctl", &["daemon-reload"])?;
        return Ok("/etc/systemd/system/omnilan.service".to_string());
    }
    #[cfg(target_os = "macos")]
    {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let path = home.join("Library/LaunchAgents/com.omnilan.agent.plist");
        run("launchctl", &["unload", path.to_string_lossy().as_ref()]).ok();
        fs::remove_file(&path).ok();
        return Ok(path.display().to_string());
    }
    #[cfg(target_os = "windows")]
    {
        run("schtasks", &["/Delete", "/TN", "OmniLAN", "/F"]).ok();
        return Ok("ScheduledTask:OmniLAN".to_string());
    }
    #[allow(unreachable_code)]
    Ok("unsupported".to_string())
}

#[cfg(target_os = "linux")]
fn render_systemd_unit(_cfg: &AppConfig, managed_config: &Path) -> String {
    format!(
        r#"[Unit]
Description=OmniLAN Gateway Service
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart={} run -c {}
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
"#,
        current_bin(),
        managed_config.display()
    )
}

#[cfg(target_os = "macos")]
fn render_launchd_plist(_cfg: &AppConfig, managed_config: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>com.omnilan.agent</string>
  <key>ProgramArguments</key>
  <array>
    <string>{}</string>
    <string>run</string>
    <string>-c</string>
    <string>{}</string>
  </array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
</dict>
</plist>
"#,
        current_bin(),
        managed_config.display()
    )
}

fn current_bin() -> String {
    std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "omnilan".to_string())
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
