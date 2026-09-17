// In-browser stand-in for the Tauri commands in app/src/main.rs, so the shared
// dashboard (app/ui) runs as a static site. Every command returns the same JSON
// shape as its Rust twin; keep the two in step when either changes.
//
// Ported from crates/core: client.rs (endpoints, TTLs, cache keys), history.rs
// (stats derived from the match history), models.rs and display.rs (derived
// fields), config.rs (profile store).
(function () {
  'use strict';

  const BASE = 'https://api.opendota.com/api';
  const MIN = 60e3, HOUR = 60 * MIN;
  const TTL = { profile: 6 * HOUR, wl: 30 * MIN, matches: 10 * MIN, constants: 7 * 24 * HOUR };
  // Mirrors cache::prune: long-TTL entries would otherwise pile up forever.
  const MAX_ENTRY_AGE = 30 * 24 * HOUR;
  const CACHE_PREFIX = 'dota-stats.cache.';
  const USERS_KEY = 'dota-stats.users';
  // Per-endpoint keys from before stats were derived from one history. They
  // would squat the storage quota the history needs until their 30-day prune.
  const LEGACY_KEYS = /^(wl|heroes|matches|heromatches|totals|counts)_/;
  const DIRE_SLOT_START = 128;
  const GAME_MODE_TURBO = 23;

  /* ---------------- cache ---------------- */

  // Holds entries localStorage can't: match detail (large, and would eat the
  // ~5 MB quota) and anything a full or blocked storage rejects.
  const memory = new Map();
  // Cold dashboards ask several commands for one endpoint at once (heroes,
  // the hero index); sharing the in-flight request spends one API call, not N.
  const inflight = new Map();

  /** Cached body for `key` if younger than `ttl`, else null. */
  function cacheGet(key, ttl) {
    let entry = memory.get(key);
    if (!entry) {
      try { entry = JSON.parse(localStorage.getItem(CACHE_PREFIX + key)); } catch { entry = null; }
      // Keep the parsed copy: a dashboard render reads the same history from
      // six commands, and re-parsing it from storage each time is the slow part.
      if (entry) memory.set(key, entry);
    }
    return entry && Date.now() - entry.t <= ttl ? entry.body : null;
  }

  /** Store `body` under `key`; `persist` false keeps it for this tab only. */
  function cachePut(key, body, persist) {
    const entry = { t: Date.now(), body };
    if (persist) {
      try { localStorage.setItem(CACHE_PREFIX + key, JSON.stringify(entry)); } catch { /* quota: memory only */ }
    }
    memory.set(key, entry);
  }

  /** Every persisted cache key (without prefix). Collected first: removing while indexing skips keys. */
  function storedKeys() {
    const keys = [];
    try {
      for (let i = 0; i < localStorage.length; i++) {
        const k = localStorage.key(i);
        if (k && k.startsWith(CACHE_PREFIX)) keys.push(k.slice(CACHE_PREFIX.length));
      }
    } catch { /* storage blocked: nothing persisted */ }
    return keys;
  }

  /** Drop entries past MAX_ENTRY_AGE. Best-effort, like cache::prune. */
  function prune() {
    const now = Date.now();
    for (const k of storedKeys()) {
      try {
        const e = JSON.parse(localStorage.getItem(CACHE_PREFIX + k));
        if (!e || now - e.t > MAX_ENTRY_AGE || LEGACY_KEYS.test(k)) localStorage.removeItem(CACHE_PREFIX + k);
      } catch { localStorage.removeItem(CACHE_PREFIX + k); }
    }
  }

  /** Drop player data but keep `const_*` (hero list): it changes on patch days, not on Refresh. */
  function clearCache() {
    for (const k of storedKeys()) if (!k.startsWith('const_')) localStorage.removeItem(CACHE_PREFIX + k);
    for (const k of [...memory.keys()]) if (!k.startsWith('const_')) memory.delete(k);
  }

  /**
   * GET `path` as JSON through the cache. Errors are strings, as Tauri delivers them.
   * `pack` reshapes the body before it is cached, for payloads too big to store as-is.
   */
  function getJson(path, key, ttl, persist = true, pack = (body) => body) {
    const hit = cacheGet(key, ttl);
    if (hit !== null) return Promise.resolve(hit);
    if (inflight.has(key)) return inflight.get(key);
    const req = (async () => {
      let res;
      try { res = await fetch(BASE + path); }
      catch (e) { throw `http error: ${e.message || e}`; }
      if (res.status === 429) throw 'OpenDota rate limit reached — wait a minute and try again';
      if (res.status === 404) throw 'not found on OpenDota — check the account id, or the profile may be private';
      if (!res.ok) throw `http error: ${res.status} ${res.statusText}`;
      let body;
      try { body = pack(await res.json()); } catch (e) { throw `parse error: ${e.message || e}`; }
      cachePut(key, body, persist);
      return body;
    })();
    inflight.set(key, req);
    const forget = () => inflight.delete(key);
    req.then(forget, forget);
    return req;
  }

  /* ---------------- OpenDota client (client.rs) ---------------- */

  // Everything history.rs reads. A field missing here comes back absent and
  // silently aggregates to zero.
  const HISTORY_FIELDS = ['match_id', 'player_slot', 'radiant_win', 'hero_id', 'start_time', 'duration',
    'game_mode', 'lobby_type', 'lane_role', 'kills', 'deaths', 'assists',
    'gold_per_min', 'xp_per_min', 'last_hits', 'denies', 'hero_damage', 'tower_damage'];
  const HISTORY_PATH = (id) =>
    `/players/${id}/matches?significant=0${HISTORY_FIELDS.map((f) => `&project=${f}`).join('')}`;

  /**
   * History rows as positional arrays in HISTORY_FIELDS order. A long career is
   * ~3 MB as objects, which two compared profiles would push past the ~5 MB
   * localStorage quota; without the repeated keys it is about a quarter of that.
   */
  const packHistory = (rows) => rows.map((m) => HISTORY_FIELDS.map((f) => m[f] ?? null));

  // Unpacked rows per packed array. The memory cache hands back the same array
  // until it expires, so each download is unpacked once, not once per command.
  const unpacked = new WeakMap();
  /** Rows back as objects, newest first (the order OpenDota sends). */
  function unpackHistory(packed) {
    let rows = unpacked.get(packed);
    if (!rows) {
      rows = packed.map((r) => Object.fromEntries(HISTORY_FIELDS.map((f, i) => [f, r[i]])));
      unpacked.set(packed, rows);
    }
    return rows;
  }

  const api = {
    player: (id) => getJson(`/players/${id}`, `player_${id}`, TTL.profile),
    // Unfiltered (significant=0): the Turbo toggle is applied by selectGames,
    // so one download serves both states of the switch.
    history: (id) => getJson(HISTORY_PATH(id), `history_${id}`, TTL.matches, true, packHistory).then(unpackHistory),
    matchDetail: (matchId) => getJson(`/matches/${matchId}`, `match_${matchId}`, TTL.constants, false),
    peers: (id) => getJson(`/players/${id}/peers`, `peers_${id}`, TTL.wl),
  };

  /* ---------------- stats from the match history (history.rs) ---------------- */

  const MIN_SIGNIFICANT_DURATION = 360;
  // game_mode / lobby_type ids flagged `balanced` in OpenDota's constants.
  const BALANCED_GAME_MODES = new Set([0, 1, 2, 3, 4, 5, 12, 16, 17, 22]);
  const BALANCED_LOBBY_TYPES = new Set([0, 1, 2, 5, 6, 7, 9, 10, 11, 13]);
  const TOTAL_FIELDS = ['kills', 'deaths', 'assists', 'gold_per_min', 'xp_per_min',
    'last_hits', 'denies', 'hero_damage', 'tower_damage', 'duration'];

  /** OpenDota's significant=1 rule: balanced mode and lobby, known result, not an abandon. */
  const isSignificant = (m) => BALANCED_GAME_MODES.has(m.game_mode) && BALANCED_LOBBY_TYPES.has(m.lobby_type)
    && m.radiant_win != null && n0(m.duration) > MIN_SIGNIFICANT_DURATION;

  /** Games counted under the Turbo toggle, newest first. */
  const selectGames = (rows, turbo) => rows.filter((m) => turbo || isSignificant(m))
    .sort((a, b) => b.match_id - a.match_id);

  /** A known win; /wl files unknown results under losses, so this is not `!lost`. */
  const wonGame = (m) => won(m) === true;

  /** Selected games for an account, as a promise. */
  const gamesFor = async (id, turbo) => selectGames(await api.history(id), turbo);

  /** /wl: unknown results count as losses, as OpenDota does. */
  function winLossOf(rows) {
    const win = rows.filter(wonGame).length;
    return { win, lose: rows.length - win };
  }

  /** /heroes, games desc. OpenDota's tie order is arbitrary; hero id keeps it stable. */
  function heroStatsOf(rows) {
    const byHero = new Map();
    for (const m of rows) {
      if (!m.hero_id) continue; // unrecorded rows carry no hero; /heroes skips them
      const h = byHero.get(m.hero_id) || { hero_id: m.hero_id, games: 0, win: 0, last_played: null };
      h.games++;
      if (wonGame(m)) h.win++;
      if (m.start_time != null && (h.last_played == null || m.start_time > h.last_played)) h.last_played = m.start_time;
      byHero.set(m.hero_id, h);
    }
    return [...byHero.values()].sort((a, b) => b.games - a.games || a.hero_id - b.hero_id);
  }

  /** /totals: `n` counts only games that recorded the field. */
  const totalsOf = (rows) => TOTAL_FIELDS.map((field) => {
    const vals = rows.map((m) => m[field]).filter((v) => v != null);
    return { field, n: vals.length, sum: vals.reduce((a, b) => a + b, 0) };
  });

  /** /counts lane_role and game_mode groups; a missing id is filed under "0". */
  function countsOf(rows) {
    const c = { lane_role: {}, game_mode: {} };
    for (const m of rows) {
      for (const group of ['lane_role', 'game_mode']) {
        const slot = (c[group][String(m[group] ?? 0)] ??= { games: 0, win: 0 });
        slot.games++;
        if (wonGame(m)) slot.win++;
      }
    }
    return c;
  }

  let heroIndexMemo = null;
  /** id -> { name, slug } from the /heroes constants, memoized for the page's life. */
  function heroIndex() {
    if (!heroIndexMemo) {
      heroIndexMemo = getJson('/heroes', 'const_heroes', TTL.constants).then((list) => {
        const byId = new Map();
        for (const h of list) {
          const name = h.name || '';
          byId.set(h.id, {
            name: h.localized_name || '',
            slug: name.startsWith('npc_dota_hero_') ? name.slice('npc_dota_hero_'.length) : name,
          });
        }
        return byId;
      });
      // A failed fetch must not poison every later call for the page's lifetime.
      heroIndexMemo.catch(() => { heroIndexMemo = null; });
    }
    return heroIndexMemo;
  }

  /* ---------------- derived fields (models.rs / display.rs) ---------------- */

  const n0 = (v) => v ?? 0;
  const pct = (win, games) => (games === 0 ? 0 : (win / games) * 100);
  const kdaRatio = (k, d, a) => (k + a) / Math.max(d, 1);
  const heroName = (idx, id) => idx.get(id)?.name ?? `hero ${id}`;
  const heroSlug = (idx, id) => idx.get(id)?.slug ?? null;
  const isRadiant = (slot) => n0(slot) < DIRE_SLOT_START;
  const won = (m) => (m.radiant_win == null ? null : m.radiant_win === isRadiant(m.player_slot));
  const matchKda = (m) => (n0(m.kills) + n0(m.assists)) / Math.max(n0(m.deaths), 1);
  const MEDALS = [null, 'Herald', 'Guardian', 'Crusader', 'Archon', 'Legend', 'Ancient', 'Divine', 'Immortal'];
  const medalName = (tier) => (tier == null ? null : MEDALS[Math.floor(tier / 10)]) || 'Uncalibrated';
  const medalStars = (tier) => (tier == null ? 0 : tier % 10);
  const LANE_ROLES = { 1: 'Safe lane', 2: 'Mid lane', 3: 'Off lane', 4: 'Jungle' };
  const GAME_MODES = { 1: 'All Pick', 2: 'Captains Mode', 3: 'Random Draft', 4: 'Single Draft', 5: 'All Random',
    16: 'Captains Draft', 18: 'Ability Draft', 22: 'Ranked All Pick', 23: 'Turbo' };

  /** One hero stat row as get_heroes / get_top_winrate emit it. */
  const heroStatOut = (idx, h) => ({
    hero_id: h.hero_id, hero: heroName(idx, h.hero_id), icon: heroSlug(idx, h.hero_id),
    games: n0(h.games), win: n0(h.win), winrate: pct(n0(h.win), n0(h.games)),
  });

  /* ---------------- profile store (config.rs UsersStore) ---------------- */

  /** Saved profiles; a missing or unreadable store is the valid empty state. */
  function loadUsers() {
    try {
      const s = JSON.parse(localStorage.getItem(USERS_KEY));
      if (s && Array.isArray(s.profiles)) return { profiles: s.profiles, selected: s.selected ?? null };
    } catch { /* fall through to empty */ }
    return { profiles: [], selected: null };
  }

  function saveUsers(store) {
    try { localStorage.setItem(USERS_KEY, JSON.stringify(store)); }
    catch (e) { throw `config error: could not save profiles in this browser (${e.message || e})`; }
  }

  const usersPayload = (s) => ({
    profiles: s.profiles.map((p) => ({ label: p.label, account_id: p.account_id })),
    selected: s.selected,
  });

  /** Account a command acts on: the compare override, else the active profile (client_for). */
  function accountFor(accountId) {
    if (accountId != null) return accountId;
    const { selected } = loadUsers();
    if (selected == null) throw 'config error: No profile selected — add a Dota ID';
    return selected;
  }

  /* ---------------- commands (main.rs) ---------------- */

  const commands = {
    list_users: () => usersPayload(loadUsers()),

    add_user: ({ label, accountId }) => {
      if (!Number.isInteger(accountId) || accountId <= 0) throw 'config error: account_id must be a non-zero number';
      const s = loadUsers();
      const clean = String(label ?? '').trim() || `account ${accountId}`;
      const existing = s.profiles.find((p) => p.account_id === accountId);
      if (existing) existing.label = clean;
      else s.profiles.push({ label: clean, account_id: accountId });
      if (s.selected == null) s.selected = accountId;
      saveUsers(s);
      return usersPayload(s);
    },

    remove_user: ({ accountId }) => {
      const s = loadUsers();
      s.profiles = s.profiles.filter((p) => p.account_id !== accountId);
      if (s.selected === accountId) s.selected = s.profiles[0]?.account_id ?? null;
      saveUsers(s);
      return usersPayload(s);
    },

    select_user: ({ accountId }) => {
      const s = loadUsers();
      if (!s.profiles.some((p) => p.account_id === accountId)) throw `config error: no profile with account_id ${accountId}`;
      s.selected = accountId;
      saveUsers(s);
      return null;
    },

    clear_cache: () => { clearCache(); return null; },

    get_profile: async ({ accountId }) => {
      const id = accountFor(accountId);
      const p = await api.player(id);
      const tier = p.rank_tier ?? null;
      return {
        name: p.profile?.personaname ?? `account ${id}`,
        avatar: p.profile?.avatarfull ?? null,
        medal: medalName(tier),
        stars: medalStars(tier),
        rank_tier: tier,
        leaderboard_rank: p.leaderboard_rank ?? null,
        mmr_estimate: p.mmr_estimate?.estimate ?? null,
      };
    },

    get_winrate: async ({ includeTurbo, accountId }) => {
      const { win, lose } = winLossOf(await gamesFor(accountFor(accountId), !!includeTurbo));
      return { win, lose, total: win + lose, winrate: pct(win, win + lose) };
    },

    get_heroes: async ({ n, includeTurbo, accountId }) => {
      const id = accountFor(accountId);
      const [rows, idx] = await Promise.all([gamesFor(id, !!includeTurbo), heroIndex()]);
      return heroStatsOf(rows).slice(0, n ?? 8).map((h) => heroStatOut(idx, h));
    },

    get_recent: async ({ limit, includeTurbo, heroId, accountId }) => {
      const id = accountFor(accountId);
      const count = limit ?? 12;
      const [rows, idx] = await Promise.all([gamesFor(id, !!includeTurbo), heroIndex()]);
      const matches = (heroId != null ? rows.filter((m) => m.hero_id === heroId) : rows).slice(0, count);
      return matches.map((m) => ({
        match_id: m.match_id, hero: heroName(idx, n0(m.hero_id)), icon: heroSlug(idx, n0(m.hero_id)),
        won: won(m), kills: n0(m.kills), deaths: n0(m.deaths), assists: n0(m.assists),
        kda: matchKda(m), duration: n0(m.duration), start_time: n0(m.start_time),
        is_turbo: m.game_mode === GAME_MODE_TURBO,
      }));
    },

    get_top_winrate: async ({ n, minGames, includeTurbo, accountId }) => {
      const id = accountFor(accountId);
      const [rows, idx] = await Promise.all([gamesFor(id, !!includeTurbo), heroIndex()]);
      const min = minGames ?? 5;
      const ranked = heroStatsOf(rows).map((h) => heroStatOut(idx, h))
        .filter((h) => h.games >= min)
        .sort((a, b) => b.winrate - a.winrate || b.games - a.games);
      return { min_games: min, heroes: ranked.slice(0, n ?? 6) };
    },

    // The one command with no account: /heroes is a global constant.
    get_hero_list: async () => {
      const idx = await heroIndex();
      return [...idx.entries()]
        .sort((a, b) => (a[1].name < b[1].name ? -1 : a[1].name > b[1].name ? 1 : 0))
        .map(([id, h]) => ({ hero_id: id, hero: h.name, icon: h.slug }));
    },

    get_hero_detail: async ({ heroId, includeTurbo, accountId }) => {
      const id = accountFor(accountId);
      const turbo = !!includeTurbo;
      const [idx, rows] = await Promise.all([heroIndex(), gamesFor(id, turbo)]);
      const heroes = heroStatsOf(rows);
      const matches = rows.filter((m) => m.hero_id === heroId).slice(0, 20);
      const stat = heroes.find((h) => h.hero_id === heroId);
      const n = Math.max(matches.length, 1);
      const sum = (f) => matches.reduce((acc, m) => acc + f(m), 0);
      const avgKills = sum((m) => n0(m.kills)) / n;
      const avgDeaths = sum((m) => n0(m.deaths)) / n;
      const avgAssists = sum((m) => n0(m.assists)) / n;
      // Economy fields are absent on unparsed games; averaging them in as zero
      // would understate the hero, so mean only the games that carry a value.
      const avgPresent = (field) => {
        const vals = matches.map((m) => m[field]).filter((v) => v != null);
        return vals.length ? vals.reduce((a, b) => a + b, 0) / vals.length : 0;
      };
      const games = n0(stat?.games), win = n0(stat?.win);
      return {
        hero: heroName(idx, heroId),
        icon: heroSlug(idx, heroId),
        games, win, winrate: pct(win, games),
        last_played: stat?.last_played ?? null,
        avg_kills: avgKills, avg_deaths: avgDeaths, avg_assists: avgAssists,
        avg_kda: kdaRatio(avgKills, avgDeaths, avgAssists),
        avg_gpm: avgPresent('gold_per_min'), avg_xpm: avgPresent('xp_per_min'),
        avg_last_hits: avgPresent('last_hits'), avg_hero_damage: avgPresent('hero_damage'),
        sample: matches.length,
        economy_sample: matches.filter((m) => m.gold_per_min != null).length,
        matches: matches.map((m) => ({
          match_id: m.match_id, won: won(m),
          kills: n0(m.kills), deaths: n0(m.deaths), assists: n0(m.assists), kda: matchKda(m),
          gpm: m.gold_per_min ?? null, xpm: m.xp_per_min ?? null,
          last_hits: m.last_hits ?? null, hero_damage: m.hero_damage ?? null,
          duration: n0(m.duration), start_time: n0(m.start_time),
          is_turbo: m.game_mode === GAME_MODE_TURBO,
        })),
      };
    },

    get_match_detail: async ({ matchId, accountId }) => {
      const id = accountFor(accountId);
      const [idx, m] = await Promise.all([heroIndex(), api.matchDetail(matchId)]);
      return {
        match_id: m.match_id,
        radiant_win: m.radiant_win ?? null,
        duration: n0(m.duration),
        start_time: n0(m.start_time),
        radiant_score: m.radiant_score ?? null,
        dire_score: m.dire_score ?? null,
        gold_adv: m.radiant_gold_adv ?? null,
        xp_adv: m.radiant_xp_adv ?? null,
        players: (m.players || []).map((p) => ({
          hero: heroName(idx, n0(p.hero_id)), icon: heroSlug(idx, n0(p.hero_id)),
          name: p.personaname ?? null,
          is_me: p.account_id === id,
          radiant: isRadiant(p.player_slot),
          kills: n0(p.kills), deaths: n0(p.deaths), assists: n0(p.assists),
          gpm: p.gold_per_min ?? null, xpm: p.xp_per_min ?? null,
          last_hits: p.last_hits ?? null, denies: p.denies ?? null,
          hero_damage: p.hero_damage ?? null, net_worth: p.net_worth ?? null, level: p.level ?? null,
        })),
      };
    },

    get_performance: async ({ includeTurbo, accountId }) => {
      const totals = totalsOf(await gamesFor(accountFor(accountId), !!includeTurbo));
      const avg = (field) => {
        const t = totals.find((x) => x.field === field);
        return t && n0(t.n) !== 0 ? n0(t.sum) / t.n : 0;
      };
      const k = avg('kills'), d = avg('deaths'), a = avg('assists');
      const kills = totals.find((t) => t.field === 'kills');
      return {
        // "kills" is recorded for every game, so its n is the true game count.
        games: kills ? n0(kills.n) : Math.max(0, ...totals.map((t) => n0(t.n))),
        kills: k, deaths: d, assists: a, kda: kdaRatio(k, d, a),
        gpm: avg('gold_per_min'), xpm: avg('xp_per_min'),
        last_hits: avg('last_hits'), denies: avg('denies'),
        hero_damage: avg('hero_damage'), tower_damage: avg('tower_damage'),
        duration: avg('duration'),
      };
    },

    get_breakdowns: async ({ includeTurbo, accountId }) => {
      const c = countsOf(await gamesFor(accountFor(accountId), !!includeTurbo));
      const row = (label, v) => ({
        label, games: n0(v.games), win: n0(v.win), winrate: pct(n0(v.win), n0(v.games)),
      });
      const byGames = (a, b) => b.games - a.games;
      // Lane role "0" is "unknown"; the desktop app drops it too.
      const roles = Object.entries(c.lane_role || {})
        .filter(([k, v]) => k !== '0' && n0(v.games) > 0)
        .map(([k, v]) => row(LANE_ROLES[k] || 'Unknown', v))
        .sort(byGames);
      // Modes with no name of their own (event, arcade, long-retired) sum into
      // one "Other" row rather than one row each. Mirrors get_breakdowns in
      // app/src/main.rs.
      const other = { games: 0, win: 0 };
      const modes = Object.entries(c.game_mode || {})
        .filter(([, v]) => n0(v.games) > 0)
        .flatMap(([k, v]) => {
          if (GAME_MODES[k]) return [row(GAME_MODES[k], v)];
          other.games += n0(v.games);
          other.win += n0(v.win);
          return [];
        });
      if (other.games > 0) modes.push(row('Other', other));
      return { roles, modes: modes.sort(byGames) };
    },

    get_peers: async ({ n, accountId }) => {
      const peers = await api.peers(accountFor(accountId));
      // Opponents-only entries (with_games 0) aren't teammates (display::top_peers).
      const top = peers.filter((p) => n0(p.with_games) > 0)
        .sort((a, b) => n0(b.with_games) - n0(a.with_games))
        .slice(0, n ?? 10);
      return {
        peers: top.map((p) => ({
          account_id: n0(p.account_id),
          name: p.personaname ?? String(n0(p.account_id)),
          avatar: p.avatarfull ?? null,
          games: n0(p.with_games), win: n0(p.with_win),
          winrate: pct(n0(p.with_win), n0(p.with_games)),
        })),
      };
    },
  };

  /** Tauri-compatible entry point: always a Promise, rejecting with a string. */
  function invoke(cmd, args) {
    const handler = commands[cmd];
    if (!handler) return Promise.reject(`unknown command: ${cmd}`);
    return new Promise((resolve) => resolve(handler(args || {})));
  }

  prune();
  window.DotaBackend = { invoke };
})();
