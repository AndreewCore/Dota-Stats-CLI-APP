//! Player aggregates computed locally from one full match history.
//!
//! OpenDota's `/wl`, `/heroes`, `/totals` and `/counts` each rescan a player's
//! whole history server-side, and concurrent calls from one IP queue behind each
//! other. Fetching `/players/{id}/matches` once and deriving them here replaces
//! five requests per profile with one, and makes the Turbo toggle free: the
//! `significant` filter is reproduced by [`is_significant`] instead of asking
//! the server for a second variant.
//!
//! Every function mirrors the endpoint it replaces, quirks included, so the
//! numbers match what OpenDota itself reports.

use crate::models::{Counts, HeroStat, MatchSummary, TotalField, WinGames, WinLoss};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Radiant occupies player slots 0-127, Dire 128-255.
const DIRE_SLOT_START: u32 = 128;
/// OpenDota drops games this short from significant stats (abandons, remakes).
const MIN_SIGNIFICANT_DURATION: u32 = 360;
/// `game_mode` ids flagged `balanced` in OpenDota's constants. Turbo (23) is not.
const BALANCED_GAME_MODES: [u32; 10] = [0, 1, 2, 3, 4, 5, 12, 16, 17, 22];
/// `lobby_type` ids flagged `balanced` in OpenDota's constants.
const BALANCED_LOBBY_TYPES: [u32; 10] = [0, 1, 2, 5, 6, 7, 9, 10, 11, 13];

/// Fields requested via `project=`. Anything a derived endpoint reads must be
/// listed, or it silently comes back absent and aggregates to zero.
pub const PROJECT_FIELDS: [&str; 18] = [
    "match_id", "player_slot", "radiant_win", "hero_id", "start_time", "duration",
    "game_mode", "lobby_type", "lane_role", "kills", "deaths", "assists",
    "gold_per_min", "xp_per_min", "last_hits", "denies", "hero_damage", "tower_damage",
];

/// Fields `/totals` reports that the dashboard reads, in `/totals` naming.
const TOTAL_FIELDS: [&str; 10] = [
    "kills", "deaths", "assists", "gold_per_min", "xp_per_min",
    "last_hits", "denies", "hero_damage", "tower_damage", "duration",
];

/// One row of the full history. Everything but the id is optional because
/// OpenDota returns rows with every stat `null` (unrecorded games), and a plain
/// `u32` would fail to deserialize the whole list over them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryMatch {
    pub match_id: u64,
    #[serde(default)]
    pub player_slot: Option<u32>,
    #[serde(default)]
    pub radiant_win: Option<bool>,
    #[serde(default)]
    pub hero_id: Option<u32>,
    #[serde(default)]
    pub start_time: Option<i64>,
    #[serde(default)]
    pub duration: Option<u32>,
    #[serde(default)]
    pub game_mode: Option<u32>,
    #[serde(default)]
    pub lobby_type: Option<u32>,
    #[serde(default)]
    pub lane_role: Option<u32>,
    #[serde(default)]
    pub kills: Option<u32>,
    #[serde(default)]
    pub deaths: Option<u32>,
    #[serde(default)]
    pub assists: Option<u32>,
    #[serde(default)]
    pub gold_per_min: Option<u32>,
    #[serde(default)]
    pub xp_per_min: Option<u32>,
    #[serde(default)]
    pub last_hits: Option<u32>,
    #[serde(default)]
    pub denies: Option<u32>,
    #[serde(default)]
    pub hero_damage: Option<u64>,
    #[serde(default)]
    pub tower_damage: Option<u64>,
}

impl HistoryMatch {
    /// True only for a known win. OpenDota counts an unknown result as a loss
    /// in `/wl`, so callers must not treat `false` as "decided".
    fn won(&self) -> bool {
        let radiant = self.player_slot.unwrap_or(0) < DIRE_SLOT_START;
        self.radiant_win == Some(radiant)
    }

    /// Value of a `/totals` field on this game, `None` when unrecorded.
    fn total_field(&self, field: &str) -> Option<f64> {
        match field {
            "kills" => self.kills.map(f64::from),
            "deaths" => self.deaths.map(f64::from),
            "assists" => self.assists.map(f64::from),
            "gold_per_min" => self.gold_per_min.map(f64::from),
            "xp_per_min" => self.xp_per_min.map(f64::from),
            "last_hits" => self.last_hits.map(f64::from),
            "denies" => self.denies.map(f64::from),
            "hero_damage" => self.hero_damage.map(|v| v as f64),
            "tower_damage" => self.tower_damage.map(|v| v as f64),
            "duration" => self.duration.map(f64::from),
            _ => None,
        }
    }

    /// The row in the shape the recent-match and hero drill-down views use.
    pub fn summary(&self) -> MatchSummary {
        MatchSummary {
            match_id: self.match_id,
            player_slot: self.player_slot.unwrap_or(0),
            radiant_win: self.radiant_win,
            hero_id: self.hero_id.unwrap_or(0),
            kills: self.kills.unwrap_or(0),
            deaths: self.deaths.unwrap_or(0),
            assists: self.assists.unwrap_or(0),
            duration: self.duration.unwrap_or(0),
            start_time: self.start_time.unwrap_or(0),
            game_mode: self.game_mode,
            gold_per_min: self.gold_per_min,
            xp_per_min: self.xp_per_min,
            last_hits: self.last_hits,
            hero_damage: self.hero_damage,
        }
    }
}

/// OpenDota's `significant=1` rule: a balanced mode and lobby, a known result,
/// and long enough not to be an abandon. Verified against the server's own
/// split on a 6k-game account with no mismatches.
pub fn is_significant(m: &HistoryMatch) -> bool {
    let balanced = |id: Option<u32>, set: &[u32]| id.is_some_and(|v| set.contains(&v));
    balanced(m.game_mode, &BALANCED_GAME_MODES)
        && balanced(m.lobby_type, &BALANCED_LOBBY_TYPES)
        && m.radiant_win.is_some()
        && m.duration.unwrap_or(0) > MIN_SIGNIFICANT_DURATION
}

/// Games counted under the Turbo toggle, newest first: everything with it on
/// (`significant=0`), only significant games with it off.
pub fn select(history: &[HistoryMatch], include_turbo: bool) -> Vec<&HistoryMatch> {
    let mut games: Vec<&HistoryMatch> = history
        .iter()
        .filter(|m| include_turbo || is_significant(m))
        .collect();
    games.sort_by_key(|m| std::cmp::Reverse(m.match_id));
    games
}

/// `/wl`. Unknown results land in `lose`, as OpenDota does.
pub fn win_loss(games: &[&HistoryMatch]) -> WinLoss {
    let win = games.iter().filter(|m| m.won()).count() as u32;
    WinLoss { win, lose: games.len() as u32 - win }
}

/// `/heroes`, sorted by games desc. OpenDota breaks ties in arbitrary database
/// order; hero id is used here so the list at least stays stable.
pub fn hero_stats(games: &[&HistoryMatch]) -> Vec<HeroStat> {
    let mut by_hero: HashMap<u32, HeroStat> = HashMap::new();
    // Unrecorded rows carry no hero; /heroes leaves them out too.
    for m in games.iter().filter(|m| m.hero_id.unwrap_or(0) != 0) {
        let id = m.hero_id.unwrap_or(0);
        let h = by_hero.entry(id).or_insert(HeroStat { hero_id: id, games: 0, win: 0, last_played: None });
        h.games += 1;
        h.win += u32::from(m.won());
        h.last_played = h.last_played.max(m.start_time);
    }
    let mut heroes: Vec<HeroStat> = by_hero.into_values().collect();
    heroes.sort_by(|a, b| b.games.cmp(&a.games).then(a.hero_id.cmp(&b.hero_id)));
    heroes
}

/// `/totals` for the fields the dashboard reads. `n` counts only games that
/// recorded the field, so unparsed games don't drag averages toward zero.
pub fn totals(games: &[&HistoryMatch]) -> Vec<TotalField> {
    TOTAL_FIELDS
        .iter()
        .map(|&field| {
            let vals: Vec<f64> = games.iter().filter_map(|m| m.total_field(field)).collect();
            TotalField { field: field.to_string(), n: vals.len() as u32, sum: vals.iter().sum() }
        })
        .collect()
}

/// `/counts` lane role and game mode groups. A missing id is bucketed as "0",
/// which is where OpenDota files it.
pub fn counts(games: &[&HistoryMatch]) -> Counts {
    let mut c = Counts::default();
    for m in games {
        let won = u32::from(m.won());
        for (group, id) in [(&mut c.lane_role, m.lane_role), (&mut c.game_mode, m.game_mode)] {
            let slot: &mut WinGames = group.entry(id.unwrap_or(0).to_string()).or_default();
            slot.games += 1;
            slot.win += won;
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A significant Ranked All Pick game: Radiant slot, given result.
    fn game(match_id: u64, hero_id: u32, radiant_win: Option<bool>) -> HistoryMatch {
        HistoryMatch {
            match_id,
            player_slot: Some(0),
            radiant_win,
            hero_id: Some(hero_id),
            start_time: Some(match_id as i64),
            duration: Some(2000),
            game_mode: Some(22),
            lobby_type: Some(7),
            lane_role: Some(2),
            kills: Some(10),
            deaths: Some(2),
            assists: Some(5),
            gold_per_min: Some(500),
            xp_per_min: Some(600),
            last_hits: Some(200),
            denies: Some(10),
            hero_damage: None,
            tower_damage: None,
        }
    }

    /// The all-null rows OpenDota returns for unrecorded games.
    fn blank(match_id: u64) -> HistoryMatch {
        serde_json::from_value(serde_json::json!({ "match_id": match_id, "hero_id": null, "kills": null }))
            .unwrap()
    }

    #[test]
    fn significance_excludes_turbo_short_and_unfinished_games() {
        let mut turbo = game(1, 1, Some(true));
        turbo.game_mode = Some(23);
        let mut short = game(2, 1, Some(true));
        short.duration = Some(360);
        let mut unbalanced_lobby = game(3, 1, Some(true));
        unbalanced_lobby.lobby_type = Some(4);
        let history = vec![turbo, short, unbalanced_lobby, game(4, 1, None), game(5, 1, Some(true)), blank(6)];
        let core: Vec<u64> = select(&history, false).iter().map(|m| m.match_id).collect();
        assert_eq!(core, vec![5]);
        assert_eq!(select(&history, true).len(), 6);
    }

    #[test]
    fn select_orders_newest_first() {
        let history = vec![game(1, 1, Some(true)), game(3, 1, Some(true)), game(2, 1, Some(true))];
        let ids: Vec<u64> = select(&history, true).iter().map(|m| m.match_id).collect();
        assert_eq!(ids, vec![3, 2, 1]);
    }

    #[test]
    fn unknown_results_count_as_losses() {
        let history = vec![game(1, 1, Some(true)), game(2, 1, None), blank(3)];
        let wl = win_loss(&select(&history, true));
        assert_eq!((wl.win, wl.lose), (1, 2));
    }

    #[test]
    fn hero_stats_skip_blank_rows_and_rank_by_games() {
        let mut dire_loss = game(3, 7, Some(true));
        dire_loss.player_slot = Some(130);
        let history = vec![game(1, 7, Some(true)), game(2, 9, Some(true)), dire_loss, blank(4)];
        let heroes = hero_stats(&select(&history, true));
        assert_eq!(heroes.iter().map(|h| (h.hero_id, h.games, h.win)).collect::<Vec<_>>(), vec![(7, 2, 1), (9, 1, 1)]);
        assert_eq!(heroes[0].last_played, Some(3));
    }

    #[test]
    fn totals_count_only_recorded_values() {
        let history = vec![game(1, 1, Some(true)), game(2, 1, Some(false)), blank(3)];
        let t = totals(&select(&history, true));
        let kills = t.iter().find(|f| f.field == "kills").unwrap();
        assert_eq!((kills.n, kills.sum), (2, 20.0));
        let dmg = t.iter().find(|f| f.field == "hero_damage").unwrap();
        assert_eq!(dmg.n, 0);
    }

    #[test]
    fn counts_bucket_missing_ids_under_zero() {
        let history = vec![game(1, 1, Some(true)), blank(2)];
        let c = counts(&select(&history, true));
        assert_eq!(c.lane_role["2"].games, 1);
        assert_eq!(c.lane_role["0"].games, 1);
        assert_eq!(c.game_mode["0"].win, 0);
    }
}
