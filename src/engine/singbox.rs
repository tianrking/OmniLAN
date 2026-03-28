use crate::config::{AppConfig, ProxySourceMode, RouteMode};
use crate::engine::{Engine, EngineArtifacts};
use anyhow::{Context, Result};
use std::fs;
use tokio::process::Command;

pub struct SingBoxEngine;

impl Engine for SingBoxEngine {
    fn name(&self) -> &'static str {
        "sing-box"
    }

    fn ensure_binary(&self, cfg: &AppConfig) -> Result<()> {
        which::which(&cfg.executables.sing_box).with_context(|| {
            format!(
                "cannot find sing-box binary: {} (please install or adjust executables.sing_box)",
                cfg.executables.sing_box
            )
        })?;
        Ok(())
    }

    fn prepare(&self, cfg: &AppConfig) -> Result<EngineArtifacts> {
        fs::create_dir_all(&cfg.runtime.work_dir)
            .with_context(|| format!("failed to create {}", cfg.runtime.work_dir.display()))?;
        let render_path = cfg.runtime.work_dir.join("sing-box.generated.json");
        let rendered = render_sing_box_json(cfg);
        fs::write(&render_path, rendered)
            .with_context(|| format!("failed to write {}", render_path.display()))?;
        Ok(EngineArtifacts { render_path })
    }

    fn command(&self, cfg: &AppConfig, artifacts: &EngineArtifacts) -> Command {
        let mut cmd = Command::new(&cfg.executables.sing_box);
        cmd.arg("run")
            .arg("-D")
            .arg(&cfg.runtime.work_dir)
            .arg("-c")
            .arg(&artifacts.render_path);
        cmd
    }
}

fn render_sing_box_json(cfg: &AppConfig) -> String {
    let route_final = match cfg.routing.mode {
        RouteMode::Rule => "Proxy",
        RouteMode::Global => "Proxy",
        RouteMode::Direct => "direct",
    };
    let cn_rule = if cfg.routing.domestic_direct {
        ", {\"rule_set\": \"geoip-cn\", \"outbound\": \"direct\"}, {\"rule_set\": \"geosite-cn\", \"outbound\": \"direct\"}"
    } else {
        ""
    };
    let ad_rule = if cfg.routing.block_ads {
        ", {\"rule_set\": \"geosite-category-ads-all\", \"outbound\": \"block\"}"
    } else {
        ""
    };

    let proxy_outbound = match cfg.source.mode {
        ProxySourceMode::Subscription => {
            "{\"type\":\"selector\",\"tag\":\"Proxy\",\"outbounds\":[\"auto\",\"direct\"]}"
                .to_string()
        }
        ProxySourceMode::LocalFile => {
            // Keep generated profile runnable even without parsing foreign config formats.
            "{\"type\":\"selector\",\"tag\":\"Proxy\",\"outbounds\":[\"direct\"]}".to_string()
        }
    };

    format!(
        r#"{{
  "log": {{
    "level": "info"
  }},
  "dns": {{
    "servers": [
      {{ "tag": "local", "address": "223.5.5.5", "detour": "direct" }},
      {{ "tag": "remote", "address": "https://1.1.1.1/dns-query", "detour": "Proxy" }}
    ],
    "rules": [
      {{ "rule_set": "geosite-cn", "server": "local" }}
    ],
    "final": "remote"
  }},
  "inbounds": [
    {{
      "type": "mixed",
      "tag": "mixed-in",
      "listen": "{bind_address}",
      "listen_port": {mixed_port}
    }},
    {{
      "type": "redirect",
      "tag": "redir-in",
      "listen": "{bind_address}",
      "listen_port": {redir_port}
    }},
    {{
      "type": "tun",
      "tag": "tun-in",
      "inet4_address": "172.19.0.1/30",
      "auto_route": true,
      "strict_route": false
    }}
  ],
  "outbounds": [
    {proxy_outbound},
    {{ "type": "urltest", "tag": "auto", "outbounds": ["direct"], "url": "http://www.gstatic.com/generate_204", "interval": "2m" }},
    {{ "type": "direct", "tag": "direct" }},
    {{ "type": "block", "tag": "block" }}
  ],
  "route": {{
    "rule_set": [
      {{ "tag": "geoip-cn", "type": "remote", "format": "binary", "url": "https://raw.githubusercontent.com/SagerNet/sing-geoip/rule-set/geoip-cn.srs", "download_detour": "direct" }},
      {{ "tag": "geosite-cn", "type": "remote", "format": "binary", "url": "https://raw.githubusercontent.com/SagerNet/sing-geosite/rule-set/geosite-cn.srs", "download_detour": "direct" }},
      {{ "tag": "geosite-category-ads-all", "type": "remote", "format": "binary", "url": "https://raw.githubusercontent.com/SagerNet/sing-geosite/rule-set/geosite-category-ads-all.srs", "download_detour": "Proxy" }}
    ],
    "rules": [
      {{ "inbound": "mixed-in", "outbound": "Proxy" }},
      {{ "inbound": "redir-in", "outbound": "Proxy" }},
      {{ "inbound": "tun-in", "outbound": "Proxy" }}{cn_rule}{ad_rule}
    ],
    "final": "{route_final}"
  }}
}}
"#,
        bind_address = cfg.inbound.bind_address,
        mixed_port = cfg.inbound.mixed_port,
        redir_port = cfg.inbound.redir_port,
        proxy_outbound = proxy_outbound,
        cn_rule = cn_rule,
        ad_rule = ad_rule,
        route_final = route_final,
    )
}
