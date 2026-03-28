use crate::domain::config::{AppConfig, EngineKind};
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const MIHOMO_VERSION: &str = "v1.19.8";
const SINGBOX_VERSION: &str = "v1.12.4";

pub fn install(cfg: &AppConfig, engine: Option<EngineKind>) -> Result<Vec<String>> {
    fs::create_dir_all(bin_dir(cfg))
        .with_context(|| format!("failed to create {}", bin_dir(cfg).display()))?;
    let mut installed = vec![];
    match engine {
        Some(EngineKind::Mihomo) => {
            installed.push(install_mihomo(cfg)?);
        }
        Some(EngineKind::SingBox) => {
            installed.push(install_singbox(cfg)?);
        }
        None => {
            installed.push(install_mihomo(cfg)?);
            installed.push(install_singbox(cfg)?);
        }
    }
    Ok(installed)
}

fn install_mihomo(cfg: &AppConfig) -> Result<String> {
    let asset = mihomo_asset()?;
    let dest = PathBuf::from(&cfg.executables.mihomo);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let mirror_prefixes = [
        "",
        "https://ghp.ci/",
        "https://hub.gitmirror.com/",
        "https://github.moeyy.xyz/",
    ];
    let src = format!(
        "https://github.com/MetaCubeX/mihomo/releases/download/{}/{}",
        MIHOMO_VERSION, asset
    );
    download_with_mirrors(&src, &dest, &mirror_prefixes)?;
    chmod_exec(&dest)?;
    Ok(format!("mihomo -> {}", dest.display()))
}

fn install_singbox(cfg: &AppConfig) -> Result<String> {
    let (asset, bin_name, is_zip) = singbox_asset()?;
    let dest = PathBuf::from(&cfg.executables.sing_box);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let tmp_dir = cfg.runtime.work_dir.join("tmp-kernel");
    fs::create_dir_all(&tmp_dir)
        .with_context(|| format!("failed to create {}", tmp_dir.display()))?;
    let archive = tmp_dir.join(&asset);

    let mirror_prefixes = ["", "https://ghp.ci/", "https://hub.gitmirror.com/"];
    let src = format!(
        "https://github.com/SagerNet/sing-box/releases/download/{}/{}",
        SINGBOX_VERSION, asset
    );
    download_with_mirrors(&src, &archive, &mirror_prefixes)?;
    extract_archive(&archive, &tmp_dir, is_zip)?;
    let candidate = tmp_dir.join(bin_name);
    if !candidate.exists() {
        anyhow::bail!(
            "cannot find extracted sing-box binary: {}",
            candidate.display()
        );
    }
    fs::copy(&candidate, &dest).with_context(|| {
        format!(
            "failed to install {} -> {}",
            candidate.display(),
            dest.display()
        )
    })?;
    chmod_exec(&dest)?;
    Ok(format!("sing-box -> {}", dest.display()))
}

fn download_with_mirrors(src: &str, dest: &Path, mirror_prefixes: &[&str]) -> Result<()> {
    let mut last_err = None;
    for p in mirror_prefixes {
        let url = format!("{}{}", p, src);
        let status = Command::new("curl")
            .args(["-fsSL", "-o"])
            .arg(dest)
            .arg(&url)
            .status();
        match status {
            Ok(s) if s.success() => return Ok(()),
            Ok(s) => {
                last_err = Some(anyhow::anyhow!("download failed {} status={}", url, s));
            }
            Err(e) => {
                last_err = Some(anyhow::anyhow!("download failed {} error={}", url, e));
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("download failed: {}", src)))
}

fn extract_archive(archive: &Path, out_dir: &Path, is_zip: bool) -> Result<()> {
    if is_zip {
        #[cfg(windows)]
        {
            let status = Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-Command",
                    &format!(
                        "Expand-Archive -Path '{}' -DestinationPath '{}' -Force",
                        archive.display(),
                        out_dir.display()
                    ),
                ])
                .status()
                .context("failed to extract zip")?;
            if !status.success() {
                anyhow::bail!("failed to extract zip archive");
            }
            return Ok(());
        }
        #[cfg(not(windows))]
        {
            let status = Command::new("unzip")
                .arg("-o")
                .arg(archive)
                .arg("-d")
                .arg(out_dir)
                .status()
                .context("failed to extract zip")?;
            if !status.success() {
                anyhow::bail!("failed to extract zip archive");
            }
            return Ok(());
        }
    }

    let status = Command::new("tar")
        .arg("-xzf")
        .arg(archive)
        .arg("-C")
        .arg(out_dir)
        .status()
        .context("failed to extract tar.gz")?;
    if !status.success() {
        anyhow::bail!("failed to extract tar.gz archive");
    }
    Ok(())
}

fn bin_dir(cfg: &AppConfig) -> PathBuf {
    cfg.runtime.work_dir.join("bin")
}

fn mihomo_asset() -> Result<String> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let suffix = match (os, arch) {
        ("linux", "x86_64") => "linux-amd64",
        ("linux", "aarch64") => "linux-arm64",
        ("macos", "x86_64") => "darwin-amd64",
        ("macos", "aarch64") => "darwin-arm64",
        _ => anyhow::bail!("unsupported platform for mihomo: {}/{}", os, arch),
    };
    Ok(format!("mihomo-{}", suffix))
}

fn singbox_asset() -> Result<(String, String, bool)> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let ver = SINGBOX_VERSION.trim_start_matches('v');
    match (os, arch) {
        ("linux", "x86_64") => Ok((
            format!("sing-box-{}-linux-amd64.tar.gz", ver),
            "sing-box".to_string(),
            false,
        )),
        ("linux", "aarch64") => Ok((
            format!("sing-box-{}-linux-arm64.tar.gz", ver),
            "sing-box".to_string(),
            false,
        )),
        ("macos", "x86_64") => Ok((
            format!("sing-box-{}-darwin-amd64.tar.gz", ver),
            "sing-box".to_string(),
            false,
        )),
        ("macos", "aarch64") => Ok((
            format!("sing-box-{}-darwin-arm64.tar.gz", ver),
            "sing-box".to_string(),
            false,
        )),
        ("windows", "x86_64") => Ok((
            format!("sing-box-{}-windows-amd64.zip", ver),
            "sing-box.exe".to_string(),
            true,
        )),
        _ => anyhow::bail!("unsupported platform for sing-box: {}/{}", os, arch),
    }
}

fn chmod_exec(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = fs::metadata(path)?.permissions();
        perm.set_mode(0o755);
        fs::set_permissions(path, perm)?;
    }
    Ok(())
}
