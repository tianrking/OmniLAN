use crate::cli::{Cli, Commands};
use crate::config::AppConfig;
use crate::enforcement::{execute_rollback, write_rollback_script, EnforcementOrchestrator};
use crate::engine::from_config;
use crate::gateway::GatewayOrchestrator;
use crate::state::RuntimeState;
use anyhow::{Context, Result};
use chrono::Utc;
use std::fs;
use std::process;
use tokio::signal;
use tracing::{info, warn};

pub async fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Init => cmd_init(&cli.config),
        Commands::Validate => cmd_validate(&cli.config),
        Commands::Render => cmd_render(&cli.config),
        Commands::Run => cmd_run(&cli.config).await,
        Commands::Status => cmd_status(&cli.config),
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
