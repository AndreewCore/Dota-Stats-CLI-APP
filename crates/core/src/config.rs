use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Resolved runtime config for a single profile — what `OpenDota::new` consumes.
/// Produced on demand from the active profile in [`UsersStore`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// OpenDota account_id (Steam32 / Dota friend id).
    pub account_id: u64,
    /// Optional OpenDota API key (raises rate limits). Never required.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

/// One saved player profile shown in the UI dropdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    /// Friendly label shown in the dropdown (e.g. "Main", "Smurf").
    pub label: String,
    /// OpenDota account_id (Steam32 / Dota friend id).
    pub account_id: u64,
}

/// On-disk store of all saved profiles plus which one is active. Persisted as
/// JSON at [`users_path`]. The repo ships an empty template; the real file lives
/// in the OS config dir and is never committed.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsersStore {
    #[serde(default)]
    pub profiles: Vec<UserProfile>,
    /// account_id of the active profile, or `None` when empty / unselected.
    #[serde(default)]
    pub selected: Option<u64>,
    /// Optional global OpenDota API key (raises rate limits). Never required.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

/// Directory holding `users.json`.
///
/// `XDG_CONFIG_HOME` is honoured on every platform, Windows included, so a
/// portable copy can keep its profiles next to itself without a rebuild.
pub fn config_dir() -> Result<PathBuf> {
    if let Some(x) = std::env::var_os("XDG_CONFIG_HOME") {
        if !x.is_empty() {
            return Ok(PathBuf::from(x).join("dota-stats"));
        }
    }
    platform_config_dir()
}

/// `%APPDATA%\dota-stats`.
///
/// Roaming, not Local: the saved profiles are a handful of account ids the user
/// would expect to follow them to another machine on the same account.
#[cfg(windows)]
fn platform_config_dir() -> Result<PathBuf> {
    Ok(appdata_dir("APPDATA", "Roaming")?.join("dota-stats"))
}

/// `~/.config/dota-stats`.
#[cfg(not(windows))]
fn platform_config_dir() -> Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .ok_or_else(|| Error::Config("HOME is not set".into()))?;
    Ok(PathBuf::from(home).join(".config").join("dota-stats"))
}

/// `%LOCALAPPDATA%`, the root the cache module hangs its directory off.
#[cfg(windows)]
pub(crate) fn local_appdata_dir() -> Result<PathBuf> {
    appdata_dir("LOCALAPPDATA", "Local")
}

/// Resolve one of the AppData roots (`Roaming` or `Local`) from the environment.
///
/// Windows sets `var` for interactive sessions, but service and some CI shells
/// run with a stripped environment, hence the `%USERPROFILE%\AppData\<sub>`
/// fallback before giving up.
#[cfg(windows)]
fn appdata_dir(var: &str, sub: &str) -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os(var) {
        if !dir.is_empty() {
            return Ok(PathBuf::from(dir));
        }
    }
    let profile = std::env::var_os("USERPROFILE")
        .filter(|p| !p.is_empty())
        .ok_or_else(|| Error::Config(format!("neither {var} nor USERPROFILE is set")))?;
    Ok(PathBuf::from(profile).join("AppData").join(sub))
}

/// Full path of the saved-profiles file inside [`config_dir`].
pub fn users_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("users.json"))
}

impl UsersStore {
    /// Load the store from disk. A missing file is a valid empty state (the UI
    /// prompts the user to add an id), so it is **not** an error.
    pub fn load() -> Result<UsersStore> {
        let path = users_path()?;
        if !path.exists() {
            return Ok(UsersStore::default());
        }
        let text = std::fs::read_to_string(&path)?;
        if text.trim().is_empty() {
            return Ok(UsersStore::default());
        }
        serde_json::from_str(&text).map_err(Error::from)
    }

    /// Persist the store. Writes to a temp file and renames, because a partial
    /// write here loses every saved profile, not just the edit in flight.
    pub fn save(&self) -> Result<()> {
        let dir = config_dir()?;
        std::fs::create_dir_all(&dir)?;
        let text = serde_json::to_string_pretty(self).map_err(Error::from)?;
        let path = users_path()?;
        let tmp = path.with_extension(format!("tmp{}", std::process::id()));
        std::fs::write(&tmp, text)?;
        if let Err(e) = std::fs::rename(&tmp, &path) {
            std::fs::remove_file(&tmp).ok();
            return Err(e.into());
        }
        Ok(())
    }

    /// Add a profile (or update the label of an existing one with the same id).
    /// The first profile added becomes the selected one. `account_id` must be
    /// non-zero. A blank label defaults to `account {id}`.
    pub fn add(&mut self, label: &str, account_id: u64) -> Result<()> {
        if account_id == 0 {
            return Err(Error::Config("account_id must be a non-zero number".into()));
        }
        let label = if label.trim().is_empty() {
            format!("account {account_id}")
        } else {
            label.trim().to_string()
        };
        match self.profiles.iter_mut().find(|p| p.account_id == account_id) {
            Some(p) => p.label = label,
            None => self.profiles.push(UserProfile { label, account_id }),
        }
        if self.selected.is_none() {
            self.selected = Some(account_id);
        }
        Ok(())
    }

    /// Remove a profile. If it was the selected one, repoint `selected` to the
    /// first remaining profile (or `None` when the list becomes empty).
    pub fn remove(&mut self, account_id: u64) {
        self.profiles.retain(|p| p.account_id != account_id);
        if self.selected == Some(account_id) {
            self.selected = self.profiles.first().map(|p| p.account_id);
        }
    }

    /// Select a profile by id. No-op (error) if the id isn't in the list.
    pub fn set_selected(&mut self, account_id: u64) -> Result<()> {
        if self.profiles.iter().any(|p| p.account_id == account_id) {
            self.selected = Some(account_id);
            Ok(())
        } else {
            Err(Error::Config(format!("no profile with account_id {account_id}")))
        }
    }

    /// Resolve the active profile into a [`Config`] for `OpenDota::new`.
    pub fn active_config(&self) -> Result<Config> {
        let id = self
            .selected
            .ok_or_else(|| Error::Config("No profile selected — add a Dota ID".into()))?;
        Ok(Config {
            account_id: id,
            api_key: self.api_key.clone(),
        })
    }
}
