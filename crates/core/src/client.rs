use crate::cache;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::models::*;
use serde::de::DeserializeOwned;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

const BASE: &str = "https://api.opendota.com/api";

/// A request that never returns would hang the blocking thread it runs on
/// forever, leaving the card that asked for it stuck on "Loading…".
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// One shared agent for the process: the dashboard fires a dozen requests per
/// render, and a pooled connection spares each of them a fresh TLS handshake.
fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::AgentBuilder::new()
            .timeout_connect(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .build()
    })
}

/// TTLs per endpoint — balances freshness against OpenDota's rate limits.
const TTL_PROFILE: Duration = Duration::from_secs(6 * 3600);
const TTL_WL: Duration = Duration::from_secs(30 * 60);
const TTL_HEROES: Duration = Duration::from_secs(60 * 60);
const TTL_MATCHES: Duration = Duration::from_secs(10 * 60);
const TTL_CONSTANTS: Duration = Duration::from_secs(7 * 24 * 3600);

pub struct OpenDota {
    account_id: u64,
    api_key: Option<String>,
}

/// OpenDota aggregations default to `significant=1`, which excludes Turbo (and
/// other non-standard modes). `significant=0` includes everything.
fn significant(include_turbo: bool) -> u8 {
    if include_turbo { 0 } else { 1 }
}

/// Cache-key suffix so Turbo and core stats never collide on disk.
fn turbo_key(include_turbo: bool) -> &'static str {
    if include_turbo { "_turbo" } else { "_core" }
}

impl OpenDota {
    pub fn new(cfg: &Config) -> Self {
        OpenDota {
            account_id: cfg.account_id,
            api_key: cfg.api_key.clone(),
        }
    }

    pub fn account_id(&self) -> u64 {
        self.account_id
    }

    /// Fetch `path` (relative to BASE) as raw JSON, honoring the cache.
    fn get_raw(&self, path: &str, cache_key: &str, ttl: Duration) -> Result<String> {
        if let Some(hit) = cache::get(cache_key, ttl)? {
            return Ok(hit);
        }
        let mut req = agent().get(&format!("{BASE}{path}"));
        if let Some(key) = &self.api_key {
            req = req.query("api_key", key);
        }
        let body = req.call()?.into_string()?;
        cache::put(cache_key, &body)?;
        Ok(body)
    }

    fn get_json<T: DeserializeOwned>(
        &self,
        path: &str,
        cache_key: &str,
        ttl: Duration,
    ) -> Result<T> {
        let raw = self.get_raw(path, cache_key, ttl)?;
        serde_json::from_str(&raw).map_err(Error::from)
    }

    pub fn player(&self) -> Result<PlayerResponse> {
        let id = self.account_id;
        self.get_json(&format!("/players/{id}"), &format!("player_{id}"), TTL_PROFILE)
    }

    pub fn win_loss(&self, include_turbo: bool) -> Result<WinLoss> {
        let id = self.account_id;
        let sig = significant(include_turbo);
        let key = format!("wl_{id}{}", turbo_key(include_turbo));
        self.get_json(&format!("/players/{id}/wl?significant={sig}"), &key, TTL_WL)
    }

    /// Heroes played, sorted by games desc (OpenDota already returns this order).
    pub fn heroes(&self, include_turbo: bool) -> Result<Vec<HeroStat>> {
        let id = self.account_id;
        let sig = significant(include_turbo);
        let key = format!("heroes_{id}{}", turbo_key(include_turbo));
        self.get_json(&format!("/players/{id}/heroes?significant={sig}"), &key, TTL_HEROES)
    }

    pub fn recent_matches(&self, limit: u32, include_turbo: bool) -> Result<Vec<MatchSummary>> {
        let id = self.account_id;
        let sig = significant(include_turbo);
        let path = format!("/players/{id}/matches?limit={limit}&significant={sig}");
        let key = format!("matches_{id}_{limit}{}", turbo_key(include_turbo));
        self.get_json(&path, &key, TTL_MATCHES)
    }

    /// Recent matches for a single hero, enriched with per-game economy fields
    /// (requested via `project=`), for the hero drill-down.
    pub fn hero_matches(&self, hero_id: u32, limit: u32, include_turbo: bool) -> Result<Vec<MatchSummary>> {
        let id = self.account_id;
        let sig = significant(include_turbo);
        const PROJ: &str = "&project=start_time&project=duration&project=kills&project=deaths&project=assists\
&project=gold_per_min&project=xp_per_min&project=last_hits&project=hero_damage&project=game_mode";
        let path =
            format!("/players/{id}/matches?hero_id={hero_id}&limit={limit}&significant={sig}{PROJ}");
        let key = format!("heromatches_{id}_{hero_id}_{limit}{}", turbo_key(include_turbo));
        self.get_json(&path, &key, TTL_MATCHES)
    }

    /// `/matches/{id}` — full match detail. Finished matches are immutable, so
    /// cache them for the long constants TTL.
    pub fn match_detail(&self, match_id: u64) -> Result<MatchDetail> {
        self.get_json(
            &format!("/matches/{match_id}"),
            &format!("match_{match_id}"),
            TTL_CONSTANTS,
        )
    }

    /// `/players/{id}/totals` — career sums per stat, for averages.
    pub fn totals(&self, include_turbo: bool) -> Result<Vec<TotalField>> {
        let id = self.account_id;
        let sig = significant(include_turbo);
        let key = format!("totals_{id}{}", turbo_key(include_turbo));
        self.get_json(&format!("/players/{id}/totals?significant={sig}"), &key, TTL_WL)
    }

    /// `/players/{id}/counts` — games/win grouped by lane, game mode, etc.
    pub fn counts(&self, include_turbo: bool) -> Result<Counts> {
        let id = self.account_id;
        let sig = significant(include_turbo);
        let key = format!("counts_{id}{}", turbo_key(include_turbo));
        self.get_json(&format!("/players/{id}/counts?significant={sig}"), &key, TTL_WL)
    }

    /// `/players/{id}/peers` — teammates, with games/wins played together.
    /// No `significant` param on this endpoint, so there's no turbo variant.
    pub fn peers(&self) -> Result<Vec<Peer>> {
        let id = self.account_id;
        self.get_json(&format!("/players/{id}/peers"), &format!("peers_{id}"), TTL_WL)
    }

    /// Name and icon slug for every hero, from the `/heroes` constant list.
    ///
    /// Memoized for the life of the process: `/heroes` is the same for every
    /// account, and each dashboard render used to re-read and re-parse the whole
    /// list twice per command — once for names, once for icons.
    pub fn hero_index(&self) -> Result<Arc<HeroIndex>> {
        static MEMO: OnceLock<Mutex<Option<Arc<HeroIndex>>>> = OnceLock::new();
        let slot = MEMO.get_or_init(|| Mutex::new(None));
        if let Some(idx) = slot.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            return Ok(idx);
        }
        let heroes: Vec<Hero> = self.get_json("/heroes", "const_heroes", TTL_CONSTANTS)?;
        let idx = Arc::new(HeroIndex::new(heroes));
        *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(idx.clone());
        Ok(idx)
    }
}

/// Hero constants keyed by id, so a lookup doesn't rebuild a map per call.
pub struct HeroIndex {
    by_id: std::collections::HashMap<u32, (String, String)>,
}

impl HeroIndex {
    fn new(heroes: Vec<Hero>) -> Self {
        let by_id = heroes
            .into_iter()
            .map(|h| {
                let slug = h.icon_slug().to_string();
                (h.id, (h.localized_name, slug))
            })
            .collect();
        HeroIndex { by_id }
    }

    /// Build an index directly, for tests and callers that already have the data.
    pub fn from_pairs(pairs: Vec<(u32, &str, &str)>) -> Self {
        HeroIndex {
            by_id: pairs
                .into_iter()
                .map(|(id, name, slug)| (id, (name.to_string(), slug.to_string())))
                .collect(),
        }
    }

    /// Localized hero name, or `None` for an id newer than the cached list.
    pub fn name(&self, hero_id: u32) -> Option<&str> {
        self.by_id.get(&hero_id).map(|(n, _)| n.as_str())
    }

    /// Icon slug (e.g. "antimage"), matching `app/ui/heroes/{slug}.png`.
    pub fn slug(&self, hero_id: u32) -> Option<&str> {
        self.by_id.get(&hero_id).map(|(_, s)| s.as_str())
    }

    /// Every hero as (id, name), sorted by name — backs the search autocomplete.
    pub fn sorted_by_name(&self) -> Vec<(u32, &str)> {
        let mut all: Vec<(u32, &str)> =
            self.by_id.iter().map(|(id, (n, _))| (*id, n.as_str())).collect();
        all.sort_by(|a, b| a.1.cmp(b.1));
        all
    }
}
