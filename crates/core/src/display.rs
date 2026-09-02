//! Presentation rules shared by the CLI and the desktop app.
//!
//! These used to live twice — once in each binary — which is how the hero-name
//! fallback drifted into two spellings ("42" in JSON output, "hero 42" in the
//! human one). Keeping them here means a naming rule changes in one place.

use crate::client::HeroIndex;
use crate::models::{Peer, PlayerResponse};

/// Display name for a player. Private profiles expose no persona, so fall back
/// to the account id the caller already knows.
pub fn player_name(p: &PlayerResponse, account_id: u64) -> String {
    p.profile
        .as_ref()
        .and_then(|x| x.personaname.clone())
        .unwrap_or_else(|| format!("account {account_id}"))
}

/// Display name for a hero id. A miss means the hero shipped after the cached
/// `/heroes` snapshot, so name it by id rather than dropping the row.
pub fn hero_name(heroes: &HeroIndex, hero_id: u32) -> String {
    heroes
        .name(hero_id)
        .map(str::to_string)
        .unwrap_or_else(|| format!("hero {hero_id}"))
}

/// Display name for a teammate; private profiles have no persona.
pub fn peer_name(p: &Peer) -> String {
    p.personaname
        .clone()
        .unwrap_or_else(|| p.account_id.to_string())
}

/// Rank label for a player. Immortal is ranked by leaderboard position, not by
/// stars, so it never renders the star count the other medals use.
pub fn rank_label(medal: &str, stars: u32, leaderboard_rank: Option<u32>) -> String {
    if medal != "Immortal" {
        return format!("{medal} {stars}");
    }
    match leaderboard_rank {
        Some(r) => format!("Immortal #{r}"),
        None => "Immortal".to_string(),
    }
}

/// Teammates ranked by games played together, capped at `n`.
///
/// `/peers` also returns players only ever faced as opponents (`with_games` 0);
/// those are not teammates and would otherwise pad the list with zeros.
pub fn top_peers(mut peers: Vec<Peer>, n: usize) -> Vec<Peer> {
    peers.retain(|p| p.with_games > 0);
    peers.sort_by_key(|p| std::cmp::Reverse(p.with_games));
    peers.truncate(n);
    peers
}

/// Aggregate KDA from per-game averages or career sums.
///
/// The guard is on deaths, not on the ratio: a deathless sample would otherwise
/// divide by zero, and clamping to one death matches how Dota itself reports it.
pub fn kda_ratio(kills: f64, deaths: f64, assists: f64) -> f64 {
    (kills + assists) / deaths.max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer(account_id: u64, with_games: u32) -> Peer {
        Peer {
            account_id,
            personaname: None,
            avatarfull: None,
            with_games,
            with_win: 0,
            last_played: None,
        }
    }

    #[test]
    fn hero_name_falls_back_to_the_id() {
        let heroes = HeroIndex::from_pairs(vec![(1, "Anti-Mage", "antimage")]);
        assert_eq!(hero_name(&heroes, 1), "Anti-Mage");
        assert_eq!(hero_name(&heroes, 999), "hero 999");
    }

    #[test]
    fn immortal_uses_leaderboard_rank_instead_of_stars() {
        assert_eq!(rank_label("Legend", 4, None), "Legend 4");
        // Stars are meaningless at Immortal, so they must not leak through.
        assert_eq!(rank_label("Immortal", 3, Some(42)), "Immortal #42");
        assert_eq!(rank_label("Immortal", 3, None), "Immortal");
    }

    #[test]
    fn top_peers_drops_opponents_and_ranks_by_games() {
        let got = top_peers(vec![peer(1, 5), peer(2, 0), peer(3, 12)], 10);
        assert_eq!(
            got.iter().map(|p| p.account_id).collect::<Vec<_>>(),
            vec![3, 1]
        );
    }

    #[test]
    fn top_peers_truncates() {
        let got = top_peers(vec![peer(1, 5), peer(2, 9), peer(3, 12)], 2);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].account_id, 3);
    }

    #[test]
    fn kda_ratio_survives_a_deathless_sample() {
        assert_eq!(kda_ratio(6.0, 0.0, 4.0), 10.0);
        assert_eq!(kda_ratio(6.0, 2.0, 4.0), 5.0);
    }
}
