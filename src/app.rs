use crate::cli::{Cli, Commands};
use crate::config::AppConfig;
use crate::enforcement::{execute_rollback, write_rollback_script, EnforcementOrchestrator};
use crate::engine::from_config;
use crate::gateway::GatewayOrchestrator;
use crate::service;
use crate::state::RuntimeState;
use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use std::fs;
use std::io::BufRead;
use std::process;
use tokio::signal;
use tracing::{info, warn};

pub async fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Init => cmd_init(&cli.config),
        Commands::Validate => cmd_validate(&cli.config),
        Commands::Render => cmd_render(&cli.config),
        Commands::Run => cmd_run(&cli.config).await,
        Commands::Stop => cmd_stop(&cli.config),
        Commands::Status => cmd_status(&cli.config),
        Commands::Audit => cmd_audit(&cli.config),
        Commands::ServiceInstall => cmd_service_install(&cli.config),
        Commands::ServiceUninstall => cmd_service_uninstall(),
        Commands::Rollback => cmd_rollback(&cli.config),
    }
}

fn cmd_init(config_path: &std::path::Path) -> Result<()> {
    AppConfig::save_default(config_path)?;
    println!("default config created at {}", config_path.display());
    Ok(())
}

fn cmd_validate(config_path: &std::path::Path) -> Result<()> {
    let cfg = AppConfig::load(config_path)?;
    let engine = from_config(&cfg);
    engine.ensure_binary(&cfg)?;
    println!(
        "config validation passed: engine={} file={}",
        engine.name(),
        config_path.display()
    );
    Ok(())
}

fn cmd_render(config_path: &std::path::Path) -> Result<()> {
    let cfg = AppConfig::load(config_path)?;
    let engine = from_config(&cfg);
    engine.ensure_binary(&cfg)?;
    let artifacts = engine.prepare(&cfg)?;
    println!(
        "rendered {} config: {}",
        engine.name(),
        artifacts.render_path.display()
    );
    Ok(())
}

async fn cmd_run(config_path: &std::path::Path) -> Result<()> {
    let cfg = AppConfig::load(config_path)?;
    let engine = from_config(&cfg);
    engine.ensure_binary(&cfg)?;

    crate::audit::log(&cfg, "run_start", &format!("engine={}", engine.name()))?;

    let gateway_res = GatewayOrchestrator::apply(&cfg).context(
        "gateway preparation failed (try running with root/sudo and check firewall permissions)",
    )?;
    for note in &gateway_res.notes {
        info!("{}", note);
    }
    let enf_res = EnforcementOrchestrator::apply(&cfg)?;
    for note in enf_res.notes {
        info!("{}", note);
    }

    let artifacts = engine.prepare(&cfg)?;
    let mut cmd = engine.command(&cfg, &artifacts);
    let mut child = cmd
        .spawn()
        .context("failed to start proxy engine process")?;
    let child_pid = child.id();
    info!(
        "started engine={} child_pid={:?} render={}",
        engine.name(),
        child_pid,
        artifacts.render_path.display()
    );
    crate::audit::log(
        &cfg,
        "engine_started",
        &format!("engine={} child_pid={:?}", engine.name(), child_pid),
    )?;

    let mut rollback_commands = vec![];
    rollback_commands.extend(gateway_res.rollback_commands);
    rollback_commands.extend(enf_res.rollback_commands);
    let rollback_script = write_rollback_script(&cfg.runtime.work_dir, &rollback_commands)?;

    let state = RuntimeState {
        started_at: Utc::now(),
        engine: engine.name().to_string(),
        pid: process::id(),
        child_pid,
        config_path: config_path.display().to_string(),
        render_path: artifacts.render_path.display().to_string(),
        rollback_script: rollback_script.display().to_string(),
        audit_file: cfg.runtime.audit_file.display().to_string(),
    };
    state.save(&cfg.runtime.state_file)?;

    tokio::select! {
        result = child.wait() => {
            let status = result.context("failed waiting for child process")?;
            warn!("engine exited with status: {}", status);
            crate::audit::log(&cfg, "engine_exit", &format!("status={}", status))?;
        }
        _ = signal::ctrl_c() => {
            warn!("received ctrl-c, shutting down engine process");
            if let Err(e) = child.kill().await {
                warn!("failed to kill child process: {}", e);
            }
            crate::audit::log(&cfg, "engine_stop_signal", "ctrl-c")?;
        }
    }

    Ok(())
}

fn cmd_status(config_path: &std::path::Path) -> Result<()> {
    let cfg = AppConfig::load(config_path)?;
    let state = RuntimeState::load(&cfg.runtime.state_file)?;
    println!("started_at : {}", state.started_at);
    println!("engine     : {}", state.engine);
    println!("manager_pid: {}", state.pid);
    println!("child_pid  : {:?}", state.child_pid);
    println!("config     : {}", state.config_path);
    println!("rendered   : {}", state.render_path);
    println!("rollback   : {}", state.rollback_script);
    println!("audit      : {}", state.audit_file);
    println!(
        "running    : {}",
        is_pid_running(state.child_pid).unwrap_or(false)
    );
    Ok(())
}

fn cmd_stop(config_path: &std::path::Path) -> Result<()> {
    let cfg = AppConfig::load(config_path)?;
    let state = RuntimeState::load(&cfg.runtime.state_file)?;
    let pid = state
        .child_pid
        .ok_or_else(|| anyhow!("no child pid recorded in state"))?;
    stop_pid(pid)?;
    crate::audit::log(&cfg, "engine_stopped", &format!("pid={}", pid))?;
    println!("stopped child process: {}", pid);
    Ok(())
}

fn cmd_audit(config_path: &std::path::Path) -> Result<()> {
    let cfg = AppConfig::load(config_path)?;
    if !cfg.runtime.audit_file.exists() {
        println!("audit log not found: {}", cfg.runtime.audit_file.display());
        return Ok(());
    }
    let file = fs::File::open(&cfg.runtime.audit_file)
        .with_context(|| format!("failed to open {}", cfg.runtime.audit_file.display()))?;
    let reader = std::io::BufReader::new(file);
    let mut lines: Vec<String> = reader.lines().map_while(std::result::Result::ok).collect();
    let tail = 30usize;
    if lines.len() > tail {
        lines = lines.split_off(lines.len() - tail);
    }
    for line in lines {
        println!("{}", line);
    }
    Ok(())
}

fn cmd_rollback(config_path: &std::path::Path) -> Result<()> {
    let cfg = AppConfig::load(config_path)?;
    let state = RuntimeState::load(&cfg.runtime.state_file)?;
    execute_rollback(&std::path::PathBuf::from(&state.rollback_script))?;
    crate::audit::log(&cfg, "rollback_applied", &state.rollback_script)?;
    let _ = fs::remove_file(&cfg.runtime.state_file);
    println!("rollback complete: {}", state.rollback_script);
    Ok(())
}

fn cmd_service_install(config_path: &std::path::Path) -> Result<()> {
    let cfg = AppConfig::load(config_path)?;
    let path = service::install(&cfg)?;
    println!("service installed: {}", path);
    Ok(())
}

fn cmd_service_uninstall() -> Result<()> {
    let path = service::uninstall()?;
    println!("service uninstalled: {}", path);
    Ok(())
}

fn is_pid_running(pid: Option<u32>) -> Result<bool> {
    let Some(pid) = pid else {
        return Ok(false);
    };
    #[cfg(unix)]
    {
        let status = std::process::Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .status()
            .context("failed to query process status")?;
        return Ok(status.success());
    }
    #[cfg(windows)]
    {
        let output = std::process::Command::new("tasklist")
            .arg("/FI")
            .arg(format!("PID eq {}", pid))
            .output()
            .context("failed to query tasklist")?;
        let text = String::from_utf8_lossy(&output.stdout).to_string();
        return Ok(text.contains(&pid.to_string()));
    }
    #[allow(unreachable_code)]
    Ok(false)
}

fn stop_pid(pid: u32) -> Result<()> {
    #[cfg(unix)]
    {
        let status = std::process::Command::new("kill")
            .arg("-TERM")
            .arg(pid.to_string())
            .status()
            .context("failed to send TERM signal")?;
        if !status.success() {
            anyhow::bail!("failed to stop process {}", pid);
        }
        return Ok(());
    }
    #[cfg(windows)]
    {
        let status = std::process::Command::new("taskkill")
            .arg("/PID")
            .arg(pid.to_string())
            .arg("/T")
            .arg("/F")
            .status()
            .context("failed to run taskkill")?;
        if !status.success() {
            anyhow::bail!("failed to stop process {}", pid);
        }
        return Ok(());
    }
    #[allow(unreachable_code)]
    Ok(())
}
