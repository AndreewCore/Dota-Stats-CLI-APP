use crate::error::Result;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

/// Directory holding the on-disk TTL cache.
///
/// `XDG_CACHE_HOME` is honoured on every platform, Windows included, so a
/// portable copy can keep its cache next to itself without a rebuild.
pub fn cache_dir() -> Result<PathBuf> {
    if let Some(x) = std::env::var_os("XDG_CACHE_HOME") {
        if !x.is_empty() {
            return Ok(PathBuf::from(x).join("dota-stats"));
        }
    }
    platform_cache_dir()
}

/// `%LOCALAPPDATA%\dota-stats\cache`.
///
/// Local, never Roaming: cache entries are bulky, machine-specific and
/// re-downloadable, so a roaming profile must not sync them across machines.
#[cfg(windows)]
fn platform_cache_dir() -> Result<PathBuf> {
    Ok(crate::config::local_appdata_dir()?
        .join("dota-stats")
        .join("cache"))
}

/// `~/.cache/dota-stats`.
#[cfg(not(windows))]
fn platform_cache_dir() -> Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .ok_or_else(|| crate::error::Error::Config("HOME is not set".into()))?;
    Ok(PathBuf::from(home).join(".cache").join("dota-stats"))
}

/// Absolute path of the cache entry for `key`.
fn entry_path(key: &str) -> Result<PathBuf> {
    // keys are simple slugs like "profile" or "matches_20"; keep filenames safe.
    let safe: String = key
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect();
    Ok(cache_dir()?.join(format!("{safe}.json")))
}

/// Return cached raw JSON if it exists and is younger than `ttl`.
pub fn get(key: &str, ttl: Duration) -> Result<Option<String>> {
    let path = entry_path(key)?;
    let meta = match std::fs::metadata(&path) {
        Ok(m) => m,
        Err(_) => return Ok(None),
    };
    let age = meta
        .modified()
        .ok()
        .and_then(|m| SystemTime::now().duration_since(m).ok())
        .unwrap_or(Duration::MAX);
    if age <= ttl {
        Ok(Some(std::fs::read_to_string(&path)?))
    } else {
        Ok(None)
    }
}

/// Write raw JSON to the cache under `key`.
pub fn put(key: &str, data: &str) -> Result<()> {
    let path = entry_path(key)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, data)?;
    Ok(())
}
