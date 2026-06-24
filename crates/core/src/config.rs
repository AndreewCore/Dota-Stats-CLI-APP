use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Placeholder account_id written into a freshly-created config file.
/// The app refuses to run until it is replaced with a real
/// Steam32 / Dota friend id.
pub const PLACEHOLDER_ACCOUNT_ID: u64 = 0;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// OpenDota account_id (Steam32 / Dota friend id).
    pub account_id: u64,
    /// Optional OpenDota API key (raises rate limits). Never required.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            account_id: PLACEHOLDER_ACCOUNT_ID,
            api_key: None,
        }
    }
}

/// `$XDG_CONFIG_HOME/dota-stats` or `~/.config/dota-stats`.
pub fn config_dir() -> Result<PathBuf> {
    if let Some(x) = std::env::var_os("XDG_CONFIG_HOME") {
        if !x.is_empty() {
            return Ok(PathBuf::from(x).join("dota-stats"));
        }
    }
    let home = std::env::var_os("HOME")
        .ok_or_else(|| Error::Config("HOME is not set".into()))?;
    Ok(PathBuf::from(home).join(".config").join("dota-stats"))
}

pub fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

impl Config {
    /// Load config from disk. If absent, write a template file and ask the
    /// user to fill in their account_id. Also rejects an unset placeholder id.
    pub fn load_or_init() -> Result<Config> {
        let path = config_path()?;
        if !path.exists() {
            Config::default().write()?;
            return Err(Error::Config(format!(
                "Created a config file at {}. Set your account_id \
                 (Steam32 / Dota friend id) and run again.",
                path.display()
            )));
        }
        let text = std::fs::read_to_string(&path)?;
        let cfg: Config =
            toml::from_str(&text).map_err(|e| Error::Config(e.to_string()))?;
        if cfg.account_id == PLACEHOLDER_ACCOUNT_ID {
            return Err(Error::Config(format!(
                "account_id is not set. Edit {} and set your \
                 Steam32 / Dota friend id.",
                path.display()
            )));
        }
        Ok(cfg)
    }

    pub fn write(&self) -> Result<()> {
        let dir = config_dir()?;
        std::fs::create_dir_all(&dir)?;
        let text = toml::to_string_pretty(self)
            .map_err(|e| Error::Config(e.to_string()))?;
        std::fs::write(config_path()?, text)?;
        Ok(())
    }
}
