use crate::core::engine::{Engine, EngineArtifacts};
use crate::domain::config::{AppConfig, ProxySourceMode, RouteMode};
use anyhow::{Context, Result};
use std::fs;
use tokio::process::Command;

pub struct MihomoEngine;

impl Engine for MihomoEngine {
    fn name(&self) -> &'static str {
        "mihomo"
    }

    fn ensure_binary(&self, cfg: &AppConfig) -> Result<()> {
        which::which(&cfg.executables.mihomo).with_context(|| {
            format!(
                "cannot find mihomo binary: {} (please install or adjust executables.mihomo)",
                cfg.executables.mihomo
            )
        })?;
        Ok(())
    }

    fn prepare(&self, cfg: &AppConfig) -> Result<EngineArtifacts> {
        fs::create_dir_all(&cfg.runtime.work_dir)
            .with_context(|| format!("failed to create {}", cfg.runtime.work_dir.display()))?;
        let render_path = cfg.runtime.work_dir.join("mihomo.generated.yaml");
        let rendered = render_mihomo_yaml(cfg);
        fs::write(&render_path, rendered)
            .with_context(|| format!("failed to write {}", render_path.display()))?;
        Ok(EngineArtifacts { render_path })
    }

    fn command(&self, cfg: &AppConfig, artifacts: &EngineArtifacts) -> Command {
        let mut cmd = Command::new(&cfg.executables.mihomo);
        cmd.arg("-d")
            .arg(&cfg.runtime.work_dir)
            .arg("-f")
            .arg(&artifacts.render_path);
        cmd
    }
}

fn render_mihomo_yaml(cfg: &AppConfig) -> String {
    let mode = match cfg.routing.mode {
        RouteMode::Rule => "rule",
        RouteMode::Global => "global",
        RouteMode::Direct => "direct",
    };

    let provider_block = match cfg.source.mode {
        ProxySourceMode::Subscription => {
            let url = cfg.source.subscription_url.clone().unwrap_or_default();
            format!(
                "proxy-providers:\n  {name}:\n    type: http\n    url: \"{url}\"\n    interval: 1800\n    path: ./proxy_provider/{name}.yaml\n    health-check:\n      enable: true\n      interval: 120\n      url: http://www.gstatic.com/generate_204",
                name = cfg.source.provider_name,
                url = url
            )
        }
        ProxySourceMode::LocalFile => {
            let path = cfg
                .source
                .local_file
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            format!(
                "proxy-providers:\n  {name}:\n    type: file\n    path: \"{path}\"",
                name = cfg.source.provider_name,
                path = path
            )
        }
    };

    let ad_rule = if cfg.routing.block_ads {
        "\n  - GEOSITE,category-ads-all,REJECT"
    } else {
        ""
    };

    let direct_rule = if cfg.routing.domestic_direct {
        "\n  - GEOIP,CN,DIRECT\n  - GEOSITE,CN,DIRECT"
    } else {
        ""
    };

    format!(
        r#"mixed-port: {mixed_port}
redir-port: {redir_port}
allow-lan: true
bind-address: "{bind_address}"
mode: {mode}
log-level: info
external-controller: 0.0.0.0:{api_port}
ipv6: false

tun:
  enable: true
  stack: mixed
  auto-route: true
  auto-detect-interface: true

dns:
  enable: true
  listen: 0.0.0.0:{dns_port}
  enhanced-mode: fake-ip
  fake-ip-range: 198.18.0.1/16
  nameserver:
    - 223.5.5.5
    - 119.29.29.29
    - 1.1.1.1

{provider_block}

proxy-groups:
  - name: Proxy
    type: select
    use:
      - {provider_name}
    proxies:
      - Auto
      - DIRECT
  - name: Auto
    type: url-test
    use:
      - {provider_name}
    url: http://www.gstatic.com/generate_204
    interval: 120

rules:{direct_rule}{ad_rule}
  - MATCH,Proxy
"#,
        mixed_port = cfg.inbound.mixed_port,
        redir_port = cfg.inbound.redir_port,
        bind_address = cfg.inbound.bind_address,
        mode = mode,
        api_port = cfg.inbound.api_port,
        dns_port = cfg.inbound.dns_port,
        provider_block = provider_block,
        provider_name = cfg.source.provider_name,
        direct_rule = direct_rule,
        ad_rule = ad_rule,
    )
}
