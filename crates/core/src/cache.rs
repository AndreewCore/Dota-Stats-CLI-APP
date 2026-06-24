use crate::error::{Error, Result};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

/// `$XDG_CACHE_HOME/dota-stats` or `~/.cache/dota-stats`.
pub fn cache_dir() -> Result<PathBuf> {
    if let Some(x) = std::env::var_os("XDG_CACHE_HOME") {
        if !x.is_empty() {
            return Ok(PathBuf::from(x).join("dota-stats"));
        }
    }
    let home = std::env::var_os("HOME")
        .ok_or_else(|| Error::Config("HOME is not set".into()))?;
    Ok(PathBuf::from(home).join(".cache").join("dota-stats"))
}

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
