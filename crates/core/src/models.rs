use serde::{Deserialize, Serialize};

/// `/players/{id}` — profile + rank.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerResponse {
    #[serde(default)]
    pub profile: Option<Profile>,
    /// Medal*10 + stars, e.g. 54 = Legend 4. None if uncalibrated/private.
    #[serde(default)]
    pub rank_tier: Option<u32>,
    #[serde(default)]
    pub leaderboard_rank: Option<u32>,
    /// Legacy estimated MMR; often absent on modern OpenDota.
    #[serde(default)]
    pub mmr_estimate: Option<MmrEstimate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    #[serde(default)]
    pub account_id: u64,
    #[serde(default)]
    pub personaname: Option<String>,
    #[serde(default)]
    pub avatarfull: Option<String>,
    #[serde(default)]
    pub country_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MmrEstimate {
    #[serde(default)]
    pub estimate: Option<i64>,
}

/// `/players/{id}/wl`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct WinLoss {
    #[serde(default)]
    pub win: u32,
    #[serde(default)]
    pub lose: u32,
}

impl WinLoss {
    pub fn total(&self) -> u32 {
        self.win + self.lose
    }
    pub fn winrate(&self) -> f64 {
        let t = self.total();
        if t == 0 { 0.0 } else { self.win as f64 / t as f64 * 100.0 }
    }
}

/// One element of `/players/{id}/heroes`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeroStat {
    pub hero_id: u32,
    #[serde(default)]
    pub games: u32,
    #[serde(default)]
    pub win: u32,
    #[serde(default)]
    pub last_played: Option<i64>,
}

impl HeroStat {
    pub fn winrate(&self) -> f64 {
        if self.games == 0 { 0.0 } else { self.win as f64 / self.games as f64 * 100.0 }
    }
}

/// One element of `/players/{id}/matches`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchSummary {
    pub match_id: u64,
    #[serde(default)]
    pub player_slot: u32,
    #[serde(default)]
    pub radiant_win: Option<bool>,
    #[serde(default)]
    pub hero_id: u32,
    #[serde(default)]
    pub kills: u32,
    #[serde(default)]
    pub deaths: u32,
    #[serde(default)]
    pub assists: u32,
    #[serde(default)]
    pub duration: u32,
    #[serde(default)]
    pub start_time: i64,
    /// OpenDota game_mode id; 23 = Turbo.
    #[serde(default)]
    pub game_mode: Option<u32>,
    // Present only when requested via `project=` (per-hero match list).
    #[serde(default)]
    pub gold_per_min: Option<u32>,
    #[serde(default)]
    pub xp_per_min: Option<u32>,
    #[serde(default)]
    pub last_hits: Option<u32>,
    #[serde(default)]
    pub hero_damage: Option<u64>,
}

impl MatchSummary {
    /// Radiant occupies slots 0-127; Dire 128-255.
    pub fn is_radiant(&self) -> bool {
        self.player_slot < 128
    }
    /// Turbo is game_mode 23.
    pub fn is_turbo(&self) -> bool {
        self.game_mode == Some(23)
    }
    /// Whether the player won; None when result unknown.
    pub fn won(&self) -> Option<bool> {
        self.radiant_win.map(|rw| rw == self.is_radiant())
    }
    pub fn kda(&self) -> f64 {
        let d = self.deaths.max(1);
        (self.kills + self.assists) as f64 / d as f64
    }
}

/// One element of `/heroes` (constants). Used to map hero_id -> name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hero {
    pub id: u32,
    /// Internal name, e.g. "npc_dota_hero_antimage".
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub localized_name: String,
}

impl Hero {
    /// Icon slug = internal name without the `npc_dota_hero_` prefix
    /// (e.g. "antimage"), matching the files in `dota2_icons/`.
    pub fn icon_slug(&self) -> &str {
        self.name.strip_prefix("npc_dota_hero_").unwrap_or(&self.name)
    }
}

/// `/matches/{id}` — full match detail for the drill-down view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchDetail {
    pub match_id: u64,
    #[serde(default)]
    pub radiant_win: Option<bool>,
    #[serde(default)]
    pub duration: u32,
    #[serde(default)]
    pub start_time: i64,
    #[serde(default)]
    pub game_mode: Option<u32>,
    #[serde(default)]
    pub radiant_score: Option<u32>,
    #[serde(default)]
    pub dire_score: Option<u32>,
    /// Per-minute gold/xp lead for Radiant; present only on parsed replays.
    #[serde(default)]
    pub radiant_gold_adv: Option<Vec<i64>>,
    #[serde(default)]
    pub radiant_xp_adv: Option<Vec<i64>>,
    #[serde(default)]
    pub players: Vec<MatchPlayer>,
}

/// One of the 10 players in a `MatchDetail`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchPlayer {
    #[serde(default)]
    pub account_id: Option<u64>,
    #[serde(default)]
    pub player_slot: u32,
    #[serde(default)]
    pub hero_id: u32,
    #[serde(default)]
    pub personaname: Option<String>,
    #[serde(default)]
    pub kills: u32,
    #[serde(default)]
    pub deaths: u32,
    #[serde(default)]
    pub assists: u32,
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
    pub net_worth: Option<u64>,
    #[serde(default)]
    pub level: Option<u32>,
}

impl MatchPlayer {
    pub fn is_radiant(&self) -> bool {
        self.player_slot < 128
    }
}

/// One element of `/players/{id}/totals` (sum of `field` over `n` games).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TotalField {
    pub field: String,
    #[serde(default)]
    pub n: u32,
    #[serde(default)]
    pub sum: f64,
}

impl TotalField {
    pub fn avg(&self) -> f64 {
        if self.n == 0 { 0.0 } else { self.sum / self.n as f64 }
    }
}

/// games/win pair used throughout `/players/{id}/counts`.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct WinGames {
    #[serde(default)]
    pub games: u32,
    #[serde(default)]
    pub win: u32,
}

impl WinGames {
    pub fn winrate(&self) -> f64 {
        if self.games == 0 { 0.0 } else { self.win as f64 / self.games as f64 * 100.0 }
    }
}

/// `/players/{id}/counts` — only the groups we surface.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Counts {
    #[serde(default)]
    pub lane_role: std::collections::HashMap<String, WinGames>,
    #[serde(default)]
    pub game_mode: std::collections::HashMap<String, WinGames>,
}

/// Human label for an OpenDota lane_role id.
pub fn lane_role_name(id: &str) -> &'static str {
    match id {
        "1" => "Safe lane",
        "2" => "Mid lane",
        "3" => "Off lane",
        "4" => "Jungle",
        _ => "Unknown",
    }
}

/// Human label for the common OpenDota game_mode ids.
pub fn game_mode_name(id: &str) -> &'static str {
    match id {
        "1" => "All Pick",
        "2" => "Captains Mode",
        "3" => "Random Draft",
        "4" => "Single Draft",
        "5" => "All Random",
        "16" => "Captains Draft",
        "18" => "Ability Draft",
        "22" => "Ranked All Pick",
        "23" => "Turbo",
        _ => "Other",
    }
}

/// Medal name for a rank_tier value (tens digit).
pub fn medal_name(rank_tier: Option<u32>) -> &'static str {
    match rank_tier.map(|t| t / 10) {
        Some(1) => "Herald",
        Some(2) => "Guardian",
        Some(3) => "Crusader",
        Some(4) => "Archon",
        Some(5) => "Legend",
        Some(6) => "Ancient",
        Some(7) => "Divine",
        Some(8) => "Immortal",
        _ => "Uncalibrated",
    }
}

/// Star count (ones digit); not meaningful for Immortal.
pub fn medal_stars(rank_tier: Option<u32>) -> u32 {
    rank_tier.map(|t| t % 10).unwrap_or(0)
}
