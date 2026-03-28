use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EngineKind {
    Mihomo,
    SingBox,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProxySourceMode {
    Subscription,
    LocalFile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeviceMode {
    All,
    Allowlist,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RouteMode {
    Rule,
    Global,
    Direct,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub engine: EngineKind,
    pub runtime: RuntimeConfig,
    pub executables: Executables,
    pub inbound: InboundConfig,
    pub source: SourceConfig,
    pub gateway: GatewayConfig,
    pub enforcement: EnforcementConfig,
    pub routing: RoutingConfig,
    pub device_policy: DevicePolicy,
    pub security: SecurityConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub work_dir: PathBuf,
    pub state_file: PathBuf,
    pub audit_file: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Executables {
    pub mihomo: String,
    pub sing_box: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboundConfig {
    pub bind_address: String,
    pub mixed_port: u16,
    pub redir_port: u16,
    pub dns_port: u16,
    pub api_port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceConfig {
    pub mode: ProxySourceMode,
    pub subscription_url: Option<String>,
    pub local_file: Option<PathBuf>,
    pub provider_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    pub interface: Option<String>,
    pub lan_cidr: String,
    pub auto_ip_forward: bool,
    pub auto_nat: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EnforcementMode {
    GatewayOnly,
    DhcpAssist,
    PolicyRoute,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnforcementConfig {
    pub mode: EnforcementMode,
    pub targets: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingConfig {
    pub mode: RouteMode,
    pub domestic_direct: bool,
    pub block_ads: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevicePolicy {
    pub mode: DeviceMode,
    pub allowlist: Vec<DeviceRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceRule {
    pub name: String,
    pub ip: String,
    pub mac: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub allow_unsafe_experimental: bool,
    pub enable_audit_log: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let base = home.join(".ok-proxy");
        Self {
            engine: EngineKind::Mihomo,
            runtime: RuntimeConfig {
                work_dir: base.clone(),
                state_file: base.join("state.json"),
                audit_file: base.join("audit.log"),
            },
            executables: Executables {
                mihomo: "mihomo".to_string(),
                sing_box: "sing-box".to_string(),
            },
            inbound: InboundConfig {
                bind_address: "0.0.0.0".to_string(),
                mixed_port: 7890,
                redir_port: 7892,
                dns_port: 5353,
                api_port: 9090,
            },
            source: SourceConfig {
                mode: ProxySourceMode::Subscription,
                subscription_url: Some("https://example.com/subscription".to_string()),
                local_file: None,
                provider_name: "default-subscription".to_string(),
            },
            gateway: GatewayConfig {
                interface: None,
                lan_cidr: "192.168.0.0/16".to_string(),
                auto_ip_forward: true,
                auto_nat: true,
            },
            enforcement: EnforcementConfig {
                mode: EnforcementMode::GatewayOnly,
                targets: vec![],
            },
            routing: RoutingConfig {
                mode: RouteMode::Rule,
                domestic_direct: true,
                block_ads: true,
            },
            device_policy: DevicePolicy {
                mode: DeviceMode::All,
                allowlist: vec![],
            },
            security: SecurityConfig {
                allow_unsafe_experimental: false,
                enable_audit_log: true,
            },
        }
    }
}

impl AppConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read config file: {}", path.display()))?;
        let cfg: AppConfig = serde_yaml::from_str(&raw)
            .with_context(|| format!("failed to parse yaml: {}", path.display()))?;
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn save_default(path: &Path) -> Result<()> {
        if path.exists() {
            return Err(anyhow!("config already exists: {}", path.display()));
        }
        let cfg = AppConfig::default();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create dir: {}", parent.display()))?;
        }
        let rendered = serde_yaml::to_string(&cfg).context("failed to render default config")?;
        fs::write(path, rendered)
            .with_context(|| format!("failed to write config: {}", path.display()))?;
        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        match self.source.mode {
            ProxySourceMode::Subscription => {
                let url = self.source.subscription_url.as_deref().unwrap_or_default();
                if url.is_empty() {
                    return Err(anyhow!(
                        "source.subscription_url is required when mode=subscription"
                    ));
                }
            }
            ProxySourceMode::LocalFile => {
                let local =
                    self.source.local_file.as_ref().ok_or_else(|| {
                        anyhow!("source.local_file is required when mode=local-file")
                    })?;
                if !local.exists() {
                    return Err(anyhow!("source.local_file not found: {}", local.display()));
                }
            }
        }

        if matches!(self.device_policy.mode, DeviceMode::Allowlist)
            && self.device_policy.allowlist.is_empty()
        {
            return Err(anyhow!(
                "device_policy.allowlist must not be empty when mode=allowlist"
            ));
        }

        if self.inbound.dns_port == 0
            || self.inbound.mixed_port == 0
            || self.inbound.redir_port == 0
            || self.inbound.api_port == 0
        {
            return Err(anyhow!("inbound ports must be non-zero"));
        }

        if matches!(self.enforcement.mode, EnforcementMode::PolicyRoute)
            && self.enforcement.targets.is_empty()
        {
            return Err(anyhow!(
                "enforcement.targets must not be empty when mode=policy-route"
            ));
        }

        Ok(())
    }
}
