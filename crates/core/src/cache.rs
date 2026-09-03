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
    // Keys are internal slugs like "player_123" or "matches_20_core", but the
    // escape is injective (`%` + hex) rather than mapping every odd character to
    // `_`: that collapsed "a/b" and "a_b" onto the same file, which would have
    // silently served one endpoint's payload for another.
    let mut safe = String::with_capacity(key.len());
    for c in key.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            safe.push(c);
        } else {
            for b in c.to_string().as_bytes() {
                safe.push_str(&format!("%{b:02x}"));
            }
        }
    }
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
///
/// Writes to a sibling temp file and renames, because `rename` is atomic within
/// a filesystem: a crash or a second instance mid-write can then only leave the
/// old entry or the new one, never a truncated file that reads as fresh JSON and
/// fails to parse until its TTL runs out.
pub fn put(key: &str, data: &str) -> Result<()> {
    let path = entry_path(key)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(&tmp, data)?;
    if let Err(e) = std::fs::rename(&tmp, &path) {
        std::fs::remove_file(&tmp).ok();
        return Err(e.into());
    }
    Ok(())
}

/// Entries older than this are dropped on startup. Match detail is cached under
/// the constants TTL and would otherwise accumulate one file per match forever.
const MAX_ENTRY_AGE: Duration = Duration::from_secs(30 * 24 * 3600);

/// Delete cache entries older than [`MAX_ENTRY_AGE`], including orphans left by
/// earlier key schemes. Best-effort: a cache we cannot prune is not a failure
/// worth refusing to start over.
pub fn prune() {
    let Ok(dir) = cache_dir() else { return };
    let Ok(entries) = std::fs::read_dir(&dir) else { return };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let stale = entry
            .metadata()
            .and_then(|m| m.modified())
            .map(|m| now.duration_since(m).unwrap_or_default() > MAX_ENTRY_AGE)
            .unwrap_or(false);
        if stale {
            std::fs::remove_file(entry.path()).ok();
        }
    }
}

/// Drop every cached player response, forcing the next fetch to hit the network.
/// Backs the UI's Refresh button, which is otherwise a no-op inside the TTL.
///
/// Entries keyed `const_*` (the hero list) are kept: they change on patch days,
/// not on demand, and re-fetching them would spend a request per view for
/// nothing. A missing cache dir is already the desired state, not an error.
pub fn clear() -> Result<()> {
    let dir = cache_dir()?;
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("const_") || !name.ends_with(".json") {
            continue;
        }
        // A file we lose the race on is one someone else already removed.
        let _ = std::fs::remove_file(entry.path());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `cache_dir` reads a process-global env var, so these tests cannot run
    /// concurrently: one would redirect the dir out from under another.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Point the cache at an empty scratch dir for the duration of a test.
    fn scratch(tag: &str) -> (std::sync::MutexGuard<'static, ()>, PathBuf) {
        let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("dota-stats-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::env::set_var("XDG_CACHE_HOME", &dir);
        (guard, dir)
    }

    /// The old sanitizer mapped every odd character to `_`, so these two keys
    /// shared one file and one endpoint's payload could be served for another.
    #[test]
    fn distinct_keys_never_share_a_file() {
        let (_guard, tmp) = scratch("collide");
        put("a/b", "\"slash\"").unwrap();
        put("a_b", "\"underscore\"").unwrap();
        let forever = Duration::from_secs(u32::MAX as u64);
        assert_eq!(get("a/b", forever).unwrap().as_deref(), Some("\"slash\""));
        assert_eq!(get("a_b", forever).unwrap().as_deref(), Some("\"underscore\""));
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn clear_drops_player_entries_and_keeps_constants() {
        let (_guard, tmp) = scratch("clear");

        // A cache dir that was never created is already the post-clear state.
        clear().unwrap();

        put("wl_123_core", "{\"win\":1}").unwrap();
        put("match_999", "{}").unwrap();
        put("const_heroes", "[]").unwrap();

        clear().unwrap();

        let forever = Duration::from_secs(u32::MAX as u64);
        assert!(get("wl_123_core", forever).unwrap().is_none());
        assert!(get("match_999", forever).unwrap().is_none());
        assert_eq!(get("const_heroes", forever).unwrap().as_deref(), Some("[]"));

        std::fs::remove_dir_all(&tmp).ok();
    }

    /// `put` must leave no temp file behind, or `clear`/`prune` would trip on it.
    #[test]
    fn put_leaves_no_temp_files() {
        let (_guard, tmp) = scratch("atomic");

        put("player_1", "{}").unwrap();
        let names: Vec<String> = std::fs::read_dir(cache_dir().unwrap())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["player_1.json".to_string()]);

        std::fs::remove_dir_all(&tmp).ok();
    }

    /// Prune goes by age, so a just-written entry has to survive it.
    #[test]
    fn prune_keeps_fresh_entries() {
        let (_guard, tmp) = scratch("prune");

        put("player_1", "{}").unwrap();
        prune();
        assert!(get("player_1", Duration::from_secs(60)).unwrap().is_some());

        std::fs::remove_dir_all(&tmp).ok();
    }
}
