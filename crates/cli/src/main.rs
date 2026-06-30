//! dota-stats CLI — human-readable stats plus `--json` / `widget` output for
//! waybar / eww / polybar. All data comes from the shared cached core.

use dota_stats_core::models::{medal_name, medal_stars};
use dota_stats_core::{OpenDota, Result, UsersStore};
use std::collections::HashMap;
use std::process::ExitCode;

const USAGE: &str = "\
dota-stats — Dota 2 stats from OpenDota

USAGE:
    dota-stats <COMMAND> [OPTIONS]

COMMANDS:
    profile              Name, rank/medal, and MMR estimate (if available)
    mmr                  Estimated MMR only
    rank                 Rank medal + stars
    winrate              Overall W/L and win %
    heroes [--n N]       Top N most-played heroes (default 5)
    top-hero             Single most-played hero
    recent [--limit N]   Recent matches (default 10)
    widget <METRIC>      One-line JSON for bars: mmr|rank|winrate|top-hero

PROFILES:
    users                List saved profiles (★ marks the active one)
    use <ACCOUNT_ID>     Make a saved profile the active one
    add <ACCOUNT_ID> [LABEL]   Save a profile (first one becomes active)
    remove <ACCOUNT_ID>  Delete a saved profile

OPTIONS:
    --turbo              Include Turbo matches in stats (default: core only)
    --json               Machine-readable JSON for the chosen command
    -h, --help           Show this help
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<()> {
    let cmd = args.first().map(String::as_str).unwrap_or("");
    if cmd.is_empty() || cmd == "-h" || cmd == "--help" || cmd == "help" {
        print!("{USAGE}");
        return Ok(());
    }
    let json = args.iter().any(|a| a == "--json");
    let turbo = args.iter().any(|a| a == "--turbo");

    // Profile management works regardless of whether one is selected yet.
    match cmd {
        "users" => return cmd_users(),
        "use" => return cmd_use(args.get(1).map(String::as_str)),
        "add" => return cmd_add(args.get(1).map(String::as_str), args.get(2).map(String::as_str)),
        "remove" => return cmd_remove(args.get(1).map(String::as_str)),
        _ => {}
    }

    let cfg = UsersStore::load()?.active_config()?;
    let api = OpenDota::new(&cfg);

    match cmd {
        "profile" => cmd_profile(&api, json),
        "mmr" => cmd_mmr(&api, json),
        "rank" => cmd_rank(&api, json),
        "winrate" => cmd_winrate(&api, turbo, json),
        "heroes" => cmd_heroes(&api, opt_value(args, "--n").unwrap_or(5), turbo, json),
        "top-hero" => cmd_top_hero(&api, turbo, json),
        "recent" => cmd_recent(&api, opt_value(args, "--limit").unwrap_or(10), turbo, json),
        "widget" => cmd_widget(&api, args.get(1).map(String::as_str).unwrap_or(""), turbo),
        other => {
            eprintln!("unknown command: {other}\n");
            print!("{USAGE}");
            Ok(())
        }
    }
}

/// Parse `--flag N` integer options.
fn opt_value(args: &[String], flag: &str) -> Option<u32> {
    let i = args.iter().position(|a| a == flag)?;
    args.get(i + 1)?.parse().ok()
}

fn parse_account_id(arg: Option<&str>) -> Result<u64> {
    use dota_stats_core::Error;
    arg.and_then(|s| s.parse::<u64>().ok())
        .filter(|id| *id != 0)
        .ok_or_else(|| Error::Config("expected a non-zero account_id".into()))
}

fn cmd_users() -> Result<()> {
    let store = UsersStore::load()?;
    if store.profiles.is_empty() {
        println!("No profiles yet. Add one with: dota-stats add <ACCOUNT_ID> [LABEL]");
        return Ok(());
    }
    for p in &store.profiles {
        let mark = if store.selected == Some(p.account_id) { "★" } else { " " };
        println!("{mark} {} ({})", p.label, p.account_id);
    }
    Ok(())
}

fn cmd_use(arg: Option<&str>) -> Result<()> {
    let id = parse_account_id(arg)?;
    let mut store = UsersStore::load()?;
    store.set_selected(id)?;
    store.save()?;
    println!("Active profile set to {id}");
    Ok(())
}

fn cmd_add(id_arg: Option<&str>, label_arg: Option<&str>) -> Result<()> {
    let id = parse_account_id(id_arg)?;
    let mut store = UsersStore::load()?;
    store.add(label_arg.unwrap_or(""), id)?;
    store.save()?;
    println!("Saved profile {id}");
    Ok(())
}

fn cmd_remove(arg: Option<&str>) -> Result<()> {
    let id = parse_account_id(arg)?;
    let mut store = UsersStore::load()?;
    store.remove(id);
    store.save()?;
    println!("Removed profile {id}");
    Ok(())
}

fn cmd_profile(api: &OpenDota, json: bool) -> Result<()> {
    let p = api.player()?;
    let name = p
        .profile
        .as_ref()
        .and_then(|x| x.personaname.clone())
        .unwrap_or_else(|| format!("account {}", api.account_id()));
    let medal = medal_name(p.rank_tier);
    let stars = medal_stars(p.rank_tier);
    let mmr = p.mmr_estimate.as_ref().and_then(|m| m.estimate);

    if json {
        let v = serde_json::json!({
            "name": name,
            "rank_tier": p.rank_tier,
            "medal": medal,
            "stars": stars,
            "mmr_estimate": mmr,
            "leaderboard_rank": p.leaderboard_rank,
        });
        println!("{v}");
    } else {
        println!("{name}");
        if medal == "Immortal" {
            match p.leaderboard_rank {
                Some(r) => println!("Rank: Immortal (#{r})"),
                None => println!("Rank: Immortal"),
            }
        } else {
            println!("Rank: {medal} {stars}");
        }
        match mmr {
            Some(m) => println!("MMR estimate: {m}"),
            None => println!("MMR estimate: n/a"),
        }
    }
    Ok(())
}

fn cmd_mmr(api: &OpenDota, json: bool) -> Result<()> {
    let p = api.player()?;
    let mmr = p.mmr_estimate.as_ref().and_then(|m| m.estimate);
    if json {
        println!("{}", serde_json::json!({ "mmr_estimate": mmr }));
    } else {
        match mmr {
            Some(m) => println!("{m}"),
            None => println!("n/a"),
        }
    }
    Ok(())
}

fn cmd_rank(api: &OpenDota, json: bool) -> Result<()> {
    let p = api.player()?;
    let medal = medal_name(p.rank_tier);
    let stars = medal_stars(p.rank_tier);
    if json {
        println!(
            "{}",
            serde_json::json!({ "medal": medal, "stars": stars, "rank_tier": p.rank_tier })
        );
    } else if medal == "Immortal" {
        println!("Immortal");
    } else {
        println!("{medal} {stars}");
    }
    Ok(())
}

fn cmd_winrate(api: &OpenDota, turbo: bool, json: bool) -> Result<()> {
    let wl = api.win_loss(turbo)?;
    if json {
        println!(
            "{}",
            serde_json::json!({
                "win": wl.win, "lose": wl.lose,
                "total": wl.total(), "winrate": wl.winrate()
            })
        );
    } else {
        println!(
            "{}W {}L  ({:.1}% over {})",
            wl.win,
            wl.lose,
            wl.winrate(),
            wl.total()
        );
    }
    Ok(())
}

fn cmd_heroes(api: &OpenDota, n: u32, turbo: bool, json: bool) -> Result<()> {
    let heroes = api.heroes(turbo)?;
    let names = api.hero_names()?;
    let top: Vec<_> = heroes.iter().take(n as usize).collect();
    if json {
        let arr: Vec<_> = top
            .iter()
            .map(|h| {
                serde_json::json!({
                    "hero": names.get(&h.hero_id).cloned().unwrap_or_else(|| h.hero_id.to_string()),
                    "games": h.games,
                    "win": h.win,
                    "winrate": h.winrate(),
                })
            })
            .collect();
        println!("{}", serde_json::Value::Array(arr));
    } else {
        for (i, h) in top.iter().enumerate() {
            let name = names
                .get(&h.hero_id)
                .cloned()
                .unwrap_or_else(|| format!("hero {}", h.hero_id));
            println!(
                "{:>2}. {:<20} {:>4} games  {:>5.1}% WR",
                i + 1,
                name,
                h.games,
                h.winrate()
            );
        }
    }
    Ok(())
}

fn cmd_top_hero(api: &OpenDota, turbo: bool, json: bool) -> Result<()> {
    let heroes = api.heroes(turbo)?;
    let names = api.hero_names()?;
    let top = heroes.first();
    match top {
        Some(h) => {
            let name = names
                .get(&h.hero_id)
                .cloned()
                .unwrap_or_else(|| format!("hero {}", h.hero_id));
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "hero": name, "games": h.games,
                        "win": h.win, "winrate": h.winrate()
                    })
                );
            } else {
                println!("{name} — {} games, {:.1}% WR", h.games, h.winrate());
            }
        }
        None => println!("no hero data"),
    }
    Ok(())
}

fn cmd_recent(api: &OpenDota, limit: u32, turbo: bool, json: bool) -> Result<()> {
    let matches = api.recent_matches(limit, turbo)?;
    let names = api.hero_names()?;
    if json {
        let arr: Vec<_> = matches
            .iter()
            .map(|m| {
                serde_json::json!({
                    "match_id": m.match_id,
                    "hero": names.get(&m.hero_id).cloned().unwrap_or_else(|| m.hero_id.to_string()),
                    "won": m.won(),
                    "kills": m.kills, "deaths": m.deaths, "assists": m.assists,
                    "kda": m.kda(),
                    "duration": m.duration,
                    "start_time": m.start_time,
                    "game_mode": m.game_mode,
                    "is_turbo": m.is_turbo(),
                })
            })
            .collect();
        println!("{}", serde_json::Value::Array(arr));
    } else {
        for m in &matches {
            let name = names
                .get(&m.hero_id)
                .cloned()
                .unwrap_or_else(|| format!("hero {}", m.hero_id));
            let res = match m.won() {
                Some(true) => "W",
                Some(false) => "L",
                None => "?",
            };
            let when = fmt_when(m.start_time);
            let tag = if m.is_turbo() { " [Turbo]" } else { "" };
            println!(
                "{res}  {when}  {:<20} {}/{}/{}  ({:.1} KDA)  {}m{tag}",
                name,
                m.kills,
                m.deaths,
                m.assists,
                m.kda(),
                m.duration / 60
            );
        }
    }
    Ok(())
}

/// Format a unix timestamp as local-ish `YYYY-MM-DD HH:MM` (UTC, no deps).
fn fmt_when(ts: i64) -> String {
    if ts <= 0 {
        return "—".into();
    }
    // Civil-from-days (Howard Hinnant's algorithm); UTC.
    let days = ts.div_euclid(86_400);
    let secs = ts.rem_euclid(86_400);
    let (hh, mm) = (secs / 3600, (secs % 3600) / 60);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02} {hh:02}:{mm:02}")
}

/// waybar-style one-line `{"text":..,"tooltip":..}` for the named metric.
fn cmd_widget(api: &OpenDota, metric: &str, turbo: bool) -> Result<()> {
    let (text, tooltip) = match metric {
        "mmr" => {
            let p = api.player()?;
            let medal = medal_name(p.rank_tier);
            let stars = medal_stars(p.rank_tier);
            // OpenDota deprecated mmr_estimate for most accounts; fall back to
            // the rank medal so the bar shows something useful.
            let text = match p.mmr_estimate.as_ref().and_then(|m| m.estimate) {
                Some(m) => m.to_string(),
                None => format!("{medal} {stars}"),
            };
            (text, format!("{medal} {stars}"))
        }
        "rank" => {
            let p = api.player()?;
            let medal = medal_name(p.rank_tier);
            let stars = medal_stars(p.rank_tier);
            (
                if medal == "Immortal" { "Immortal".to_string() } else { format!("{medal} {stars}") },
                format!("rank_tier {:?}", p.rank_tier),
            )
        }
        "winrate" => {
            let wl = api.win_loss(turbo)?;
            (
                format!("{:.0}%", wl.winrate()),
                format!("{}W {}L over {}", wl.win, wl.lose, wl.total()),
            )
        }
        "top-hero" => {
            let heroes = api.heroes(turbo)?;
            let names: HashMap<u32, String> = api.hero_names()?;
            match heroes.first() {
                Some(h) => {
                    let name = names
                        .get(&h.hero_id)
                        .cloned()
                        .unwrap_or_else(|| format!("hero {}", h.hero_id));
                    (name.clone(), format!("{name}: {} games, {:.1}% WR", h.games, h.winrate()))
                }
                None => ("n/a".into(), "no hero data".into()),
            }
        }
        other => {
            eprintln!("unknown widget metric: {other} (use mmr|rank|winrate|top-hero)");
            ("?".into(), String::new())
        }
    };
    println!("{}", serde_json::json!({ "text": text, "tooltip": tooltip }));
    Ok(())
}
