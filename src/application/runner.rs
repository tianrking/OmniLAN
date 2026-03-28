use crate::cli::{Cli, Commands, EngineArg};
use crate::core::enforcement::{execute_rollback, write_rollback_script, EnforcementOrchestrator};
use crate::core::engine::from_config;
use crate::core::gateway::GatewayOrchestrator;
use crate::domain::config::AppConfig;
use crate::domain::state::RuntimeState;
use crate::infra::kernel;
use crate::infra::platform;
use crate::infra::service;
use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use std::fs;
use std::io::BufRead;
use std::process;
use tokio::signal;
use tracing::{info, warn};

pub async fn run(cli: Cli) -> Result<()> {
    let engine_override = cli.engine.clone();
    match cli.command {
        Commands::Init => cmd_init(&cli.config),
        Commands::Validate => cmd_validate(&cli.config, engine_override.as_ref()),
        Commands::Render => cmd_render(&cli.config, engine_override.as_ref()),
        Commands::Run => cmd_run(&cli.config, engine_override.as_ref()).await,
        Commands::Stop { rollback } => cmd_stop(&cli.config, engine_override.as_ref(), rollback),
        Commands::Status => cmd_status(&cli.config, engine_override.as_ref()),
        Commands::Audit => cmd_audit(&cli.config, engine_override.as_ref()),
        Commands::Doctor => cmd_doctor(&cli.config, engine_override.as_ref()),
        Commands::KernelInstall => cmd_kernel_install(&cli.config, engine_override.as_ref()),
        Commands::ServiceInstall => cmd_service_install(&cli.config, engine_override.as_ref()),
        Commands::ServiceUninstall => cmd_service_uninstall(),
        Commands::Rollback => cmd_rollback(&cli.config, engine_override.as_ref()),
    }
}

fn cmd_init(config_path: &std::path::Path) -> Result<()> {
    AppConfig::save_default(config_path)?;
    println!("default config created at {}", config_path.display());
    Ok(())
}

fn cmd_validate(config_path: &std::path::Path, engine_override: Option<&EngineArg>) -> Result<()> {
    let cfg = load_config(config_path, engine_override)?;
    let engine = from_config(&cfg);
    engine.ensure_binary(&cfg)?;
    println!(
        "config validation passed: engine={} file={}",
        engine.name(),
        config_path.display()
    );
    Ok(())
}

fn cmd_render(config_path: &std::path::Path, engine_override: Option<&EngineArg>) -> Result<()> {
    let cfg = load_config(config_path, engine_override)?;
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

async fn cmd_run(config_path: &std::path::Path, engine_override: Option<&EngineArg>) -> Result<()> {
    let cfg = load_config(config_path, engine_override)?;
    let engine = from_config(&cfg);
    engine.ensure_binary(&cfg)?;

    crate::infra::audit::log(&cfg, "run_start", &format!("engine={}", engine.name()))?;

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
    crate::infra::audit::log(
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
            crate::infra::audit::log(&cfg, "engine_exit", &format!("status={}", status))?;
        }
        _ = signal::ctrl_c() => {
            warn!("received ctrl-c, shutting down engine process");
            if let Err(e) = child.kill().await {
                warn!("failed to kill child process: {}", e);
            }
            crate::infra::audit::log(&cfg, "engine_stop_signal", "ctrl-c")?;
        }
    }

    Ok(())
}

fn cmd_status(config_path: &std::path::Path, engine_override: Option<&EngineArg>) -> Result<()> {
    let cfg = load_config(config_path, engine_override)?;
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

fn cmd_stop(
    config_path: &std::path::Path,
    engine_override: Option<&EngineArg>,
    rollback: bool,
) -> Result<()> {
    let cfg = load_config(config_path, engine_override)?;
    let state = RuntimeState::load(&cfg.runtime.state_file)?;
    let pid = state
        .child_pid
        .ok_or_else(|| anyhow!("no child pid recorded in state"))?;
    stop_pid(pid)?;
    crate::infra::audit::log(&cfg, "engine_stopped", &format!("pid={}", pid))?;
    if rollback {
        execute_rollback(&std::path::PathBuf::from(&state.rollback_script))?;
        crate::infra::audit::log(&cfg, "rollback_applied", &state.rollback_script)?;
    }
    println!("stopped child process: {}", pid);
    if rollback {
        println!("rollback complete: {}", state.rollback_script);
    }
    Ok(())
}

fn cmd_audit(config_path: &std::path::Path, engine_override: Option<&EngineArg>) -> Result<()> {
    let cfg = load_config(config_path, engine_override)?;
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

fn cmd_rollback(config_path: &std::path::Path, engine_override: Option<&EngineArg>) -> Result<()> {
    let cfg = load_config(config_path, engine_override)?;
    let state = RuntimeState::load(&cfg.runtime.state_file)?;
    execute_rollback(&std::path::PathBuf::from(&state.rollback_script))?;
    crate::infra::audit::log(&cfg, "rollback_applied", &state.rollback_script)?;
    let _ = fs::remove_file(&cfg.runtime.state_file);
    println!("rollback complete: {}", state.rollback_script);
    Ok(())
}

fn cmd_service_install(
    config_path: &std::path::Path,
    engine_override: Option<&EngineArg>,
) -> Result<()> {
    let cfg = load_config(config_path, engine_override)?;
    let path = service::install(&cfg, config_path)?;
    println!("service installed: {}", path);
    Ok(())
}

fn cmd_service_uninstall() -> Result<()> {
    let path = service::uninstall()?;
    println!("service uninstalled: {}", path);
    Ok(())
}

fn cmd_doctor(config_path: &std::path::Path, engine_override: Option<&EngineArg>) -> Result<()> {
    let cfg = load_config(config_path, engine_override)?;
    let caps = platform::capabilities();
    println!("platform        : {}", caps.os);
    println!("engine          : {}", cfg.engine.as_str());
    println!("gateway         : {}", caps.gateway);
    println!("policy_route_ip : {}", caps.policy_route_ip);
    println!("policy_route_mac: {}", caps.policy_route_mac);
    println!("service         : {}", caps.service);

    let engine_bin = match cfg.engine {
        crate::domain::config::EngineKind::Mihomo => cfg.executables.mihomo.as_str(),
        crate::domain::config::EngineKind::SingBox => cfg.executables.sing_box.as_str(),
    };
    println!(
        "engine_bin      : {} ({})",
        engine_bin,
        platform::command_exists(engine_bin)
    );

    #[cfg(target_os = "linux")]
    {
        println!("iptables        : {}", platform::command_exists("iptables"));
    }
    #[cfg(target_os = "macos")]
    {
        println!("pfctl           : {}", platform::command_exists("pfctl"));
    }
    #[cfg(target_os = "windows")]
    {
        println!("netsh           : {}", platform::command_exists("netsh"));
    }
    Ok(())
}

fn cmd_kernel_install(
    config_path: &std::path::Path,
    engine_override: Option<&EngineArg>,
) -> Result<()> {
    let cfg = load_config(config_path, engine_override)?;
    let target = engine_override.map(|v| match v {
        EngineArg::Mihomo => crate::domain::config::EngineKind::Mihomo,
        EngineArg::SingBox => crate::domain::config::EngineKind::SingBox,
    });
    let installed = kernel::install(&cfg, target)?;
    for line in installed {
        println!("{}", line);
    }
    Ok(())
}

fn load_config(
    config_path: &std::path::Path,
    engine_override: Option<&EngineArg>,
) -> Result<AppConfig> {
    let mut cfg = AppConfig::load(config_path)?;
    if let Some(override_engine) = engine_override {
        cfg.engine = match override_engine {
            EngineArg::Mihomo => crate::domain::config::EngineKind::Mihomo,
            EngineArg::SingBox => crate::domain::config::EngineKind::SingBox,
        };
    }
    Ok(cfg)
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
