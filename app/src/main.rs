// Hide the console window on Windows release builds.
#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

//! Tauri desktop shell. Each #[tauri::command] wraps the shared `core` and is
//! callable from the vanilla JS frontend via window.__TAURI__.core.invoke(...).
//! Network/cache work runs on a blocking thread so the UI never freezes.

use dota_stats_core::models::{game_mode_name, lane_role_name, medal_name, medal_stars};
use dota_stats_core::{OpenDota, UsersStore};
use serde_json::{json, Value};

fn client() -> Result<OpenDota, String> {
    let cfg = UsersStore::load()
        .and_then(|s| s.active_config())
        .map_err(|e| e.to_string())?;
    Ok(OpenDota::new(&cfg))
}

/// Shape the store into the `{ profiles, selected }` payload the UI expects.
fn users_payload(store: &UsersStore) -> Value {
    let profiles: Vec<Value> = store
        .profiles
        .iter()
        .map(|p| json!({ "label": p.label, "account_id": p.account_id }))
        .collect();
    json!({ "profiles": profiles, "selected": store.selected })
}

/// List saved profiles and which one is active.
#[tauri::command]
fn list_users() -> Result<Value, String> {
    let store = UsersStore::load().map_err(|e| e.to_string())?;
    Ok(users_payload(&store))
}

/// Add (or relabel) a profile, then return the refreshed list.
#[tauri::command]
fn add_user(label: String, account_id: u64) -> Result<Value, String> {
    let mut store = UsersStore::load().map_err(|e| e.to_string())?;
    store.add(&label, account_id).map_err(|e| e.to_string())?;
    store.save().map_err(|e| e.to_string())?;
    Ok(users_payload(&store))
}

/// Remove a profile, then return the refreshed list.
#[tauri::command]
fn remove_user(account_id: u64) -> Result<Value, String> {
    let mut store = UsersStore::load().map_err(|e| e.to_string())?;
    store.remove(account_id);
    store.save().map_err(|e| e.to_string())?;
    Ok(users_payload(&store))
}

/// Set the active profile.
#[tauri::command]
fn select_user(account_id: u64) -> Result<(), String> {
    let mut store = UsersStore::load().map_err(|e| e.to_string())?;
    store.set_selected(account_id).map_err(|e| e.to_string())?;
    store.save().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn get_profile() -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let api = client()?;
        let p = api.player().map_err(|e| e.to_string())?;
        let name = p
            .profile
            .as_ref()
            .and_then(|x| x.personaname.clone())
            .unwrap_or_else(|| format!("account {}", api.account_id()));
        Ok(json!({
            "name": name,
            "avatar": p.profile.as_ref().and_then(|x| x.avatarfull.clone()),
            "medal": medal_name(p.rank_tier),
            "stars": medal_stars(p.rank_tier),
            "rank_tier": p.rank_tier,
            "leaderboard_rank": p.leaderboard_rank,
            "mmr_estimate": p.mmr_estimate.and_then(|m| m.estimate),
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_winrate(include_turbo: Option<bool>) -> Result<Value, String> {
    let turbo = include_turbo.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || {
        let api = client()?;
        let wl = api.win_loss(turbo).map_err(|e| e.to_string())?;
        Ok(json!({
            "win": wl.win, "lose": wl.lose,
            "total": wl.total(), "winrate": wl.winrate()
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_heroes(n: Option<u32>, include_turbo: Option<bool>) -> Result<Value, String> {
    let turbo = include_turbo.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || {
        let api = client()?;
        let heroes = api.heroes(turbo).map_err(|e| e.to_string())?;
        let names = api.hero_names().map_err(|e| e.to_string())?;
        let icons = api.hero_icons().map_err(|e| e.to_string())?;
        let take = n.unwrap_or(8) as usize;
        let arr: Vec<Value> = heroes
            .iter()
            .take(take)
            .map(|h| {
                json!({
                    "hero_id": h.hero_id,
                    "hero": names.get(&h.hero_id).cloned().unwrap_or_else(|| h.hero_id.to_string()),
                    "icon": icons.get(&h.hero_id),
                    "games": h.games, "win": h.win, "winrate": h.winrate(),
                })
            })
            .collect();
        Ok(Value::Array(arr))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_recent(
    limit: Option<u32>,
    include_turbo: Option<bool>,
    hero_id: Option<u32>,
) -> Result<Value, String> {
    let turbo = include_turbo.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || {
        let api = client()?;
        let n = limit.unwrap_or(12);
        // When a hero is given, pull that hero's match history instead.
        let matches = match hero_id {
            Some(hid) => api.hero_matches(hid, n, turbo),
            None => api.recent_matches(n, turbo),
        }
        .map_err(|e| e.to_string())?;
        let names = api.hero_names().map_err(|e| e.to_string())?;
        let icons = api.hero_icons().map_err(|e| e.to_string())?;
        let arr: Vec<Value> = matches
            .iter()
            .map(|m| {
                json!({
                    "match_id": m.match_id,
                    "hero": names.get(&m.hero_id).cloned().unwrap_or_else(|| m.hero_id.to_string()),
                    "icon": icons.get(&m.hero_id),
                    "won": m.won(),
                    "kills": m.kills, "deaths": m.deaths, "assists": m.assists,
                    "kda": m.kda(), "duration": m.duration, "start_time": m.start_time,
                    "is_turbo": m.is_turbo(),
                })
            })
            .collect();
        Ok(Value::Array(arr))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Heroes with the highest win rate, filtered to a minimum sample size.
#[tauri::command]
async fn get_top_winrate(
    n: Option<u32>,
    min_games: Option<u32>,
    include_turbo: Option<bool>,
) -> Result<Value, String> {
    let turbo = include_turbo.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || {
        let api = client()?;
        let names = api.hero_names().map_err(|e| e.to_string())?;
        let icons = api.hero_icons().map_err(|e| e.to_string())?;
        let mut heroes = api.heroes(turbo).map_err(|e| e.to_string())?;
        let min = min_games.unwrap_or(5);
        heroes.retain(|h| h.games >= min);
        heroes.sort_by(|a, b| {
            b.winrate()
                .partial_cmp(&a.winrate())
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.games.cmp(&a.games))
        });
        let arr: Vec<Value> = heroes
            .iter()
            .take(n.unwrap_or(6) as usize)
            .map(|h| {
                json!({
                    "hero_id": h.hero_id,
                    "hero": names.get(&h.hero_id).cloned().unwrap_or_else(|| h.hero_id.to_string()),
                    "icon": icons.get(&h.hero_id),
                    "games": h.games, "win": h.win, "winrate": h.winrate(),
                })
            })
            .collect();
        Ok(json!({ "min_games": min, "heroes": arr }))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Full hero list (id, name, icon) for the search box autocomplete.
#[tauri::command]
async fn get_hero_list() -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let api = client()?;
        let names = api.hero_names().map_err(|e| e.to_string())?;
        let icons = api.hero_icons().map_err(|e| e.to_string())?;
        let mut list: Vec<(u32, String)> = names.into_iter().collect();
        list.sort_by(|a, b| a.1.cmp(&b.1));
        let arr: Vec<Value> = list
            .iter()
            .map(|(id, name)| json!({ "hero_id": id, "hero": name, "icon": icons.get(id) }))
            .collect();
        Ok(Value::Array(arr))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Hero drill-down: career line for one hero plus its recent enriched matches.
#[tauri::command]
async fn get_hero_detail(hero_id: u32, include_turbo: Option<bool>) -> Result<Value, String> {
    let turbo = include_turbo.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || {
        let api = client()?;
        let names = api.hero_names().map_err(|e| e.to_string())?;
        let icons = api.hero_icons().map_err(|e| e.to_string())?;
        let heroes = api.heroes(turbo).map_err(|e| e.to_string())?;
        let stat = heroes.iter().find(|h| h.hero_id == hero_id);
        let matches = api.hero_matches(hero_id, 20, turbo).map_err(|e| e.to_string())?;

        // Averages over the recent enriched matches.
        let n = matches.len().max(1) as f64;
        let sum = |f: &dyn Fn(&_) -> f64| matches.iter().map(f).sum::<f64>();
        let avg_kills = sum(&|m: &dota_stats_core::models::MatchSummary| m.kills as f64) / n;
        let avg_deaths = sum(&|m| m.deaths as f64) / n;
        let avg_assists = sum(&|m| m.assists as f64) / n;
        let avg_gpm = sum(&|m| m.gold_per_min.unwrap_or(0) as f64) / n;
        let avg_xpm = sum(&|m| m.xp_per_min.unwrap_or(0) as f64) / n;
        let avg_lh = sum(&|m| m.last_hits.unwrap_or(0) as f64) / n;
        let avg_hd = sum(&|m| m.hero_damage.unwrap_or(0) as f64) / n;

        let match_list: Vec<Value> = matches
            .iter()
            .map(|m| {
                json!({
                    "match_id": m.match_id,
                    "won": m.won(),
                    "kills": m.kills, "deaths": m.deaths, "assists": m.assists,
                    "kda": m.kda(),
                    "gpm": m.gold_per_min, "xpm": m.xp_per_min,
                    "last_hits": m.last_hits, "hero_damage": m.hero_damage,
                    "duration": m.duration, "start_time": m.start_time,
                    "is_turbo": m.is_turbo(),
                })
            })
            .collect();

        Ok(json!({
            "hero": names.get(&hero_id).cloned().unwrap_or_else(|| hero_id.to_string()),
            "icon": icons.get(&hero_id),
            "games": stat.map(|s| s.games).unwrap_or(0),
            "win": stat.map(|s| s.win).unwrap_or(0),
            "winrate": stat.map(|s| s.winrate()).unwrap_or(0.0),
            "last_played": stat.and_then(|s| s.last_played),
            "avg_kills": avg_kills, "avg_deaths": avg_deaths, "avg_assists": avg_assists,
            "avg_gpm": avg_gpm, "avg_xpm": avg_xpm,
            "avg_last_hits": avg_lh, "avg_hero_damage": avg_hd,
            "sample": matches.len(),
            "matches": match_list,
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Match drill-down: full 10-player scoreboard plus gold/xp advantage curves.
#[tauri::command]
async fn get_match_detail(match_id: u64) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let api = client()?;
        let names = api.hero_names().map_err(|e| e.to_string())?;
        let icons = api.hero_icons().map_err(|e| e.to_string())?;
        let m = api.match_detail(match_id).map_err(|e| e.to_string())?;

        let players: Vec<Value> = m
            .players
            .iter()
            .map(|p| {
                json!({
                    "hero": names.get(&p.hero_id).cloned().unwrap_or_else(|| p.hero_id.to_string()),
                    "icon": icons.get(&p.hero_id),
                    "name": p.personaname,
                    "is_me": p.account_id == Some(api.account_id()),
                    "radiant": p.is_radiant(),
                    "kills": p.kills, "deaths": p.deaths, "assists": p.assists,
                    "gpm": p.gold_per_min, "xpm": p.xp_per_min,
                    "last_hits": p.last_hits, "denies": p.denies,
                    "hero_damage": p.hero_damage, "net_worth": p.net_worth, "level": p.level,
                })
            })
            .collect();

        Ok(json!({
            "match_id": m.match_id,
            "radiant_win": m.radiant_win,
            "duration": m.duration,
            "start_time": m.start_time,
            "radiant_score": m.radiant_score,
            "dire_score": m.dire_score,
            "gold_adv": m.radiant_gold_adv,
            "xp_adv": m.radiant_xp_adv,
            "players": players,
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Performance summary: career per-game averages from `/totals`.
#[tauri::command]
async fn get_performance(include_turbo: Option<bool>) -> Result<Value, String> {
    let turbo = include_turbo.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || {
        let api = client()?;
        let totals = api.totals(turbo).map_err(|e| e.to_string())?;
        let avg = |field: &str| {
            totals
                .iter()
                .find(|t| t.field == field)
                .map(|t| t.avg())
                .unwrap_or(0.0)
        };
        let k = avg("kills");
        let d = avg("deaths");
        let a = avg("assists");
        let kda = (k + a) / d.max(1.0);
        Ok(json!({
            "games": totals.iter().map(|t| t.n).max().unwrap_or(0),
            "kills": k, "deaths": d, "assists": a, "kda": kda,
            "gpm": avg("gold_per_min"), "xpm": avg("xp_per_min"),
            "last_hits": avg("last_hits"), "denies": avg("denies"),
            "hero_damage": avg("hero_damage"), "tower_damage": avg("tower_damage"),
            "duration": avg("duration"),
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Breakdowns: win/games by lane role and by game mode from `/counts`.
#[tauri::command]
async fn get_breakdowns(include_turbo: Option<bool>) -> Result<Value, String> {
    let turbo = include_turbo.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || {
        let api = client()?;
        let c = api.counts(turbo).map_err(|e| e.to_string())?;

        // role "0" is unknown; drop it. Sort by games desc.
        let mut roles: Vec<Value> = c
            .lane_role
            .iter()
            .filter(|(k, v)| *k != "0" && v.games > 0)
            .map(|(k, v)| {
                json!({ "label": lane_role_name(k), "games": v.games, "win": v.win, "winrate": v.winrate() })
            })
            .collect();
        roles.sort_by(|a, b| b["games"].as_u64().cmp(&a["games"].as_u64()));

        let mut modes: Vec<Value> = c
            .game_mode
            .iter()
            .filter(|(_, v)| v.games > 0)
            .map(|(k, v)| {
                json!({ "label": game_mode_name(k), "games": v.games, "win": v.win, "winrate": v.winrate() })
            })
            .collect();
        modes.sort_by(|a, b| b["games"].as_u64().cmp(&a["games"].as_u64()));

        Ok(json!({ "roles": roles, "modes": modes }))
    })
    .await
    .map_err(|e| e.to_string())?
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            list_users,
            add_user,
            remove_user,
            select_user,
            get_profile,
            get_winrate,
            get_heroes,
            get_recent,
            get_top_winrate,
            get_hero_list,
            get_hero_detail,
            get_match_detail,
            get_performance,
            get_breakdowns
        ])
        .run(tauri::generate_context!())
        .expect("error while running the dota-stats application");
}
