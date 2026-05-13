use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use solana_sdk::signature::Keypair;
use std::path::PathBuf;

const CONFIG_DIR: &str = ".solignition";
const CONFIG_FILE: &str = "config.toml";

/// Reject non-HTTPS API URLs unless the host is a loopback address.
///
/// The CLI talks to the deployer over a public network; allowing plain HTTP would
/// leak request bodies (including signed auth headers, file contents, etc.) to any
/// network observer. Loopback URLs are permitted for local development.
pub fn validate_api_url(api_url: &str) -> Result<()> {
    let parsed = url::Url::parse(api_url)
        .with_context(|| format!("Invalid API URL: {}", api_url))?;

    let scheme = parsed.scheme();
    if scheme == "https" {
        return Ok(());
    }

    if scheme == "http" {
        let host = parsed.host_str().unwrap_or("");
        if matches!(host, "localhost" | "127.0.0.1" | "::1") {
            return Ok(());
        }
    }

    Err(anyhow!(
        "API URL must use HTTPS (got `{}`). Plain HTTP is only allowed for \
         localhost/127.0.0.1/::1 during local development.",
        api_url
    ))
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub api_url: String,
    pub rpc_url: String,
    pub keypair_path: Option<PathBuf>,
    pub program_id: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_url: "https://api.solignition.ngrok.app".into(),
            rpc_url: "https://api.devnet.solana.com".into(),
            keypair_path: None,
            program_id: "HVzpjSxwECnb6uY9Jnia48oJp4xrQiz5jgc5hZC5df63".into(),
        }
    }
}

impl Config {
    pub fn config_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(CONFIG_DIR)
            .join(CONFIG_FILE)
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let content =
            std::fs::read_to_string(&path).context("Failed to read config file")?;
        toml::from_str(&content).context("Failed to parse config file")
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    /// Resolve the keypair path, falling back to Solana CLI default
    pub fn resolve_keypair_path(&self) -> PathBuf {
        self.keypair_path
            .clone()
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .unwrap_or_default()
                    .join(".config/solana/id.json")
            })
    }
}

/// Load a Keypair from the configured path
pub fn load_keypair(cfg: &Config) -> Result<Keypair> {
    let path = cfg.resolve_keypair_path();

    warn_if_world_readable(&path);

    let data = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read keypair from: {}", path.display()))?;

    let bytes: Vec<u8> = serde_json::from_str(&data)
        .with_context(|| format!("Invalid keypair JSON at: {}", path.display()))?;

    Keypair::try_from(&bytes[..]).context("Invalid keypair bytes")
}

#[cfg(unix)]
fn warn_if_world_readable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;

    let Ok(meta) = std::fs::metadata(path) else { return };
    let mode = meta.permissions().mode();
    if mode & 0o077 != 0 {
        eprintln!(
            "{} keypair file `{}` has loose permissions (mode {:o}). Run `chmod 600 {}` to restrict access.",
            "⚠".yellow().bold(),
            path.display(),
            mode & 0o777,
            path.display(),
        );
    }
}

#[cfg(not(unix))]
fn warn_if_world_readable(_path: &std::path::Path) {
    // POSIX permission bits don't have a clean Windows analogue; rely on
    // user/ACL-level filesystem security there.
}
