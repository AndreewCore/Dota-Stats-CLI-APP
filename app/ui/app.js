// Dashboard frontend. Loaded as a file, not inline, so the app's CSP can
// keep script-src at 'self' without relying on nonce injection.
const invoke = window.__TAURI__.core.invoke;
const $ = (id) => document.getElementById(id);
// Covers ' as well: these strings land in attributes, and a single-quoted
// one elsewhere in the file would otherwise be escapable.
const esc = (s) => String(s).replace(/[&<>"']/g, c =>
  ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const r0 = (n) => Math.round(n);
const r1 = (n) => (Math.round(n * 10) / 10).toFixed(1);
const kfmt = (n) => n >= 1000 ? (n / 1000).toFixed(1) + 'k' : r0(n);
const fmtWhen = (ts) => ts ? new Date(ts * 1000).toLocaleString([], {
  year: 'numeric', month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' }) : '—';
// Year dropped so two match tables fit side by side when comparing.
const fmtWhenShort = (ts) => ts ? new Date(ts * 1000).toLocaleString([], {
  month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' }) : '—';
const fmtDay = (ts) => ts ? new Date(ts * 1000).toLocaleDateString([], {
  year: 'numeric', month: 'short', day: 'numeric' }) : '—';
const heroIcon = (slug, cls) => slug
  ? `<img class="hicon${cls ? ' ' + cls : ''}" src="heroes/${esc(slug)}.png" alt="" loading="lazy" onerror="this.style.visibility='hidden'">`
  : `<span class="hicon${cls ? ' ' + cls : ''}"></span>`;
const resTag = (won) => won === true ? '<span class="res w">W</span>'
                      : won === false ? '<span class="res l">L</span>'
                      : '<span class="muted">?</span>';

// Turbo toggle: when on, every stat includes Turbo matches (significant=0).
let includeTurbo = false;
// Hero modal has its own Turbo switch, seeded from the global one on open.
let heroTurbo = false;
// Full hero list for the search box (id + name), loaded once.
let heroIndex = [];
// Active hero filter for the Recent matches table (null = all heroes).
let recentHeroFilter = null;
// account_id of the active profile (null = none configured yet).
let currentSelected = null;
// account_id of the compare-to profile (null = comparison off), and its label.
let compareSelected = null;
let compareLabel = '';
// Last users payload we rendered — lets the profile dropdown rebuild the
// compare list (which must exclude the active profile) without a refetch.
let lastUsers = { profiles: [], selected: null };

/* ---- tiny inline SVG charting (no deps) ---- */
const CW = 820, CH = 132, CP = 22;
function linePoints(values, lo, hi) {
  const n = values.length, span = (hi - lo) || 1;
  return values.map((v, i) => {
    const x = CP + (n === 1 ? 0 : i / (n - 1) * (CW - 2 * CP));
    const y = CH - CP - (v - lo) / span * (CH - 2 * CP);
    return `${x.toFixed(1)},${y.toFixed(1)}`;
  }).join(' ');
}
// Uniform scaling (default preserveAspectRatio) so text labels aren't stretched.
function chartFrame(inner) {
  return `<div class="chart"><svg viewBox="0 0 ${CW} ${CH}">${inner}</svg></div>`;
}
// Dot + value label at the maximum point of a series.
function peakMarker(values, lo, hi, color, fmt) {
  const n = values.length;
  let mi = 0;
  for (let i = 1; i < n; i++) if (values[i] > values[mi]) mi = i;
  const span = (hi - lo) || 1;
  const x = CP + (n === 1 ? 0 : mi / (n - 1) * (CW - 2 * CP));
  const y = CH - CP - (values[mi] - lo) / span * (CH - 2 * CP);
  const anchor = x < 70 ? 'start' : x > CW - 70 ? 'end' : 'middle';
  const ty = y - 9 < 13 ? y + 18 : y - 9;
  return `<circle cx="${x.toFixed(1)}" cy="${y.toFixed(1)}" r="3.5" fill="${color}"/>
    <text x="${x.toFixed(1)}" y="${ty.toFixed(1)}" text-anchor="${anchor}" fill="${color}"
      font-size="13" font-weight="700">${fmt(values[mi])}</text>`;
}
// Reusable winrate wheel (donut). scheme 'warm' (default, main user, red→gold)
// or 'cool' (compare user, blue→purple). Each call gets a unique gradient id.
let _donutSeq = 0;
function donutSVG(pct, cap, size, scheme) {
  size = size || 124;
  const c = size / 2, r = c - 10, C = 2 * Math.PI * r;
  const dash = (pct / 100 * C).toFixed(1);
  const gid = 'wrg' + (++_donutSeq);
  const stops = scheme === 'cool'
    ? '<stop offset="0%" stop-color="var(--blue)"/><stop offset="100%" stop-color="var(--purple)"/>'
    : '<stop offset="0%" stop-color="var(--red)"/><stop offset="100%" stop-color="var(--gold)"/>';
  return `<svg class="donut" width="${size}" height="${size}" viewBox="0 0 ${size} ${size}">
    <defs><linearGradient id="${gid}" x1="0" y1="0" x2="1" y2="1">${stops}</linearGradient></defs>
    <circle cx="${c}" cy="${c}" r="${r}" fill="none" stroke="var(--panel-3)" stroke-width="11"/>
    <circle cx="${c}" cy="${c}" r="${r}" fill="none" stroke="url(#${gid})" stroke-width="11"
      stroke-linecap="round" stroke-dasharray="${dash} ${C.toFixed(1)}" transform="rotate(-90 ${c} ${c})"/>
    <text class="val" x="${c}" y="${c - 1}" text-anchor="middle">${r1(pct)}%</text>
    <text class="cap" x="${c}" y="${c + 14}" text-anchor="middle">${cap}</text>
  </svg>`;
}
function formStrip(ms) {
  return `<div class="formstrip">${ms.map(m => {
    const c = m.won === true ? 'var(--green)' : m.won === false ? 'var(--red-soft)' : 'var(--panel-3)';
    const t = m.won === true ? 'W' : m.won === false ? 'L' : '?';
    return `<span class="formsq" style="background:${c}" title="${esc(fmtWhen(m.start_time))}">${t}</span>`;
  }).join('')}</div>`;
}
function heroGraphs(matches) {
  const ms = matches.slice().reverse(); // oldest -> newest, left -> right
  if (ms.length < 2) return '';
  const gpm = ms.map(m => m.gpm || 0), xpm = ms.map(m => m.xpm || 0);
  const eLo = Math.min(...gpm, ...xpm), eHi = Math.max(...gpm, ...xpm);
  const econ = chartFrame(
    `<polyline points="${linePoints(gpm, eLo, eHi)}" fill="none" stroke="var(--gold)" stroke-width="2"/>
     <polyline points="${linePoints(xpm, eLo, eHi)}" fill="none" stroke="#5aa9e6" stroke-width="2"/>
     ${peakMarker(gpm, eLo, eHi, 'var(--gold)', r0)}
     ${peakMarker(xpm, eLo, eHi, '#5aa9e6', r0)}`);
  const kda = ms.map(m => m.kda || 0);
  const kHi = Math.max(...kda, 1);
  const avg = kda.reduce((a, b) => a + b, 0) / kda.length;
  const aY = (CH - CP - avg / kHi * (CH - 2 * CP)).toFixed(1);
  const kchart = chartFrame(
    `<line x1="${CP}" y1="${aY}" x2="${CW - CP}" y2="${aY}" stroke="var(--line)" stroke-dasharray="4 4"/>
     <polyline points="${linePoints(kda, 0, kHi)}" fill="none" stroke="var(--green)" stroke-width="2"/>
     ${peakMarker(kda, 0, kHi, 'var(--green)', r1)}`);
  return `
    <div class="teamtitle">Recent form <span class="muted" style="font-weight:400;text-transform:none;letter-spacing:0">(oldest → newest)</span></div>
    ${formStrip(ms)}
    <div class="teamtitle">GPM / XPM trend</div>
    <div class="chart-legend"><span><i style="background:var(--gold)"></i>GPM</span><span><i style="background:#5aa9e6"></i>XPM</span></div>
    ${econ}
    <div class="teamtitle">KDA trend <span class="muted" style="font-weight:400;text-transform:none;letter-spacing:0">(dashed = avg ${r1(avg)})</span></div>
    ${kchart}`;
}

/* ---------------- main dashboard ---------------- */
/// Avatar + name + rank/MMR for one player. `cmp` picks the cool accent that
/// marks the compared user everywhere else in the dashboard.
function identityBlock(p, cmp) {
  const rank = p.medal === 'Immortal'
    ? (p.leaderboard_rank ? `Immortal #${p.leaderboard_rank}` : 'Immortal')
    : `${p.medal} ${p.stars}`;
  const mmr = p.mmr_estimate != null ? `${p.mmr_estimate} MMR` : 'MMR n/a';
  return `<div class="pident${cmp ? ' cmp' : ''}">
    ${p.avatar ? `<img class="avatar" src="${esc(p.avatar)}" alt="">` : `<div class="avatar"></div>`}
    <div>
      <div class="pname">${esc(p.name)}</div>
      <div class="psub"><span class="badge">${esc(rank)}</span><span>${esc(mmr)}</span></div>
    </div>
  </div>`;
}

async function loadProfile() {
  const el = $('profileMain');
  try {
    // The compared player's card is best-effort: a failure there must not
    // blank out the active profile's own identity.
    const [p, c] = await Promise.all([
      invoke('get_profile'),
      fetchCompare('get_profile', {}).catch(() => null),
    ]);
    el.classList.toggle('comparing', !!c);
    el.innerHTML = identityBlock(p, false)
      + (c ? `<span class="pvs">vs</span>${identityBlock(c, true)}` : '');
  } catch (e) {
    el.classList.remove('comparing');
    el.innerHTML = `<p class="err">profile: ${esc(e)}</p>`;
  }
}

// Text of the active profile in the dropdown — used for compare legends.
function mainLabelText() {
  const o = $('userSelect').selectedOptions[0];
  return o ? o.text : 'You';
}
// Fetch a command for the compare user, or null when comparison is off.
function fetchCompare(cmd, args) {
  return compareSelected != null
    ? invoke(cmd, Object.assign({ accountId: compareSelected }, args))
    : Promise.resolve(null);
}

async function loadWinrate() {
  try {
    const [w, wc] = await Promise.all([
      invoke('get_winrate', { includeTurbo }),
      fetchCompare('get_winrate', { includeTurbo }),
    ]);
    const wrStats = (x) => `<div class="wr-stats">
      <div><div class="num">${x.win}</div><div class="lbl">wins</div></div>
      <div><div class="num">${x.lose}</div><div class="lbl">losses</div></div>
      <div><div class="num">${x.total}</div><div class="lbl">games</div></div>
    </div>`;
    if (!wc) {
      $('winrate').innerHTML = `
        <div class="wr-wrap">
          ${donutSVG(w.winrate, 'WIN RATE', 124)}
          <div class="wr-stats">
            <div><div class="num" style="color:var(--green)">${w.win}</div><div class="lbl">wins</div></div>
            <div><div class="num" style="color:var(--red-soft)">${w.lose}</div><div class="lbl">losses</div></div>
            <div><div class="num">${w.total}</div><div class="lbl">games</div></div>
          </div>
        </div>`;
      return;
    }
    const row = (x, cmp, label) => `<div class="wr-row">
      <div class="who"><span class="dot ${cmp ? 'cool' : 'warm'}"></span>${esc(label)}</div>
      ${donutSVG(x.winrate, 'WIN RATE', 108, cmp ? 'cool' : 'warm')}
      ${wrStats(x)}
    </div>`;
    $('winrate').innerHTML = `<div class="wr-compare">
      ${row(w, false, mainLabelText())}
      ${row(wc, true, compareLabel)}
    </div>`;
  } catch (e) { $('winrate').innerHTML = `<p class="err">winrate: ${esc(e)}</p>`; }
}

// Ordered [label, value] tiles for the Career averages card.
function perfTiles(p) {
  return [
    ['KDA', r1(p.kda)],
    ['K / D / A', `${r1(p.kills)}/${r1(p.deaths)}/${r1(p.assists)}`],
    ['GPM', r0(p.gpm)],
    ['XPM', r0(p.xpm)],
    ['Last hits', r0(p.last_hits)],
    ['Denies', r0(p.denies)],
    ['Hero dmg', kfmt(p.hero_damage)],
    ['Avg length', r0(p.duration / 60) + 'm'],
  ];
}

async function loadPerformance() {
  try {
    const [p, pc] = await Promise.all([
      invoke('get_performance', { includeTurbo }),
      fetchCompare('get_performance', { includeTurbo }),
    ]);
    const tiles = perfTiles(p);
    const cmp = pc ? perfTiles(pc) : null;
    $('performance').innerHTML = `<div class="avgs">${tiles.map(([l, v], i) =>
      `<div class="avg">
        <div class="num${cmp ? ' main-cmp' : ''}">${v}</div>
        ${cmp ? `<div class="num cmp">${cmp[i][1]}</div>` : ''}
        <div class="lbl">${l}</div>
      </div>`).join('')}</div>`;
  } catch (e) { $('performance').innerHTML = `<p class="err">averages: ${esc(e)}</p>`; }
}

// One hero row. `cmp` rows use the cool palette and carry the compared
// account so their drill-down shows that player's numbers, not the active one's.
function heroRow(h, rank, widthPct, cmp) {
  const rankCell = cmp
    ? `<span class="rank">•</span>`
    : `<span class="rank">${rank}</span>`;
  const acct = cmp && compareSelected != null ? ` data-account="${compareSelected}"` : '';
  return `<div class="hero${cmp ? ' cmp' : ''}" data-hero="${h.hero_id}"${acct}>
    ${rankCell}
    ${heroIcon(h.icon)}
    <div>
      <div>${esc(h.hero)} <span class="muted">· ${h.games} games</span></div>
      <div class="meter"><span class="${cmp ? 'cmp' : ''}" style="width:${widthPct.toFixed(0)}%"></span></div>
    </div>
    <span class="pct${cmp ? ' cmp' : ''}">${r0(h.winrate)}% <span class="chev">›</span></span>
  </div>`;
}
// Placeholder for a hero the other profile doesn't have in its top list.
// That is not the same as never having played it — only that it fell outside
// the N rows this card asked for — so it reads "—", not 0%.
function heroMissing(h, cmp) {
  return `<div class="hero${cmp ? ' cmp' : ''}">
    <span class="rank">•</span>
    ${heroIcon(h.icon)}
    <div>
      <div class="muted">${esc(h.hero)} <span class="muted">· not in top list</span></div>
      <div class="meter"><span style="width:0%"></span></div>
    </div>
    <span class="pct muted">—</span>
  </div>`;
}
// Pair the two lists by hero, not by position. Aligning main #1 against
// compare #1 put two different heroes side by side and invited a comparison
// that wasn't one; only the colour said whose row was whose.
// `width` maps a hero to its meter width (games-relative or winrate).
function heroList(main, cmp, width) {
  if (!cmp) return main.map((h, i) => heroRow(h, i + 1, width(h), false)).join('');
  const byId = new Map(cmp.map(h => [h.hero_id, h]));
  const seen = new Set();
  let out = '';
  main.forEach((h, i) => {
    out += heroRow(h, i + 1, width(h), false);
    const c = byId.get(h.hero_id);
    if (c) { out += heroRow(c, i + 1, width(c), true); seen.add(h.hero_id); }
    else out += heroMissing(h, true);
  });
  // Heroes the compared profile has that the active one doesn't.
  for (const c of cmp) if (!seen.has(c.hero_id)) {
    out += heroMissing(c, false);
    out += heroRow(c, 0, width(c), true);
  }
  return out;
}

async function loadHeroes() {
  try {
    const [hs, hc] = await Promise.all([
      invoke('get_heroes', { n: 8, includeTurbo }),
      fetchCompare('get_heroes', { n: 8, includeTurbo }),
    ]);
    const max = Math.max(1, ...hs.map(h => h.games), ...(hc ? hc.map(h => h.games) : []));
    const width = (h) => h.games / max * 100;
    $('heroes').innerHTML = heroList(hs, hc, width) || '<p class="muted">No hero data.</p>';
  } catch (e) { $('heroes').innerHTML = `<p class="err">heroes: ${esc(e)}</p>`; }
}

function barRow(r, max, cmp) {
  return `<div class="bar${cmp ? ' cmp' : ''}">
    <span>${esc(r.label)}</span>
    <div class="track2"><div class="fill${cmp ? ' cmp' : ''}" style="width:${(r.games / max * 100).toFixed(0)}%"></div></div>
    <span class="meta">${r0(r.winrate)}% · ${r.games}g</span>
  </div>`;
}
function barMissing(label, cmp) {
  return `<div class="bar${cmp ? ' cmp' : ''}">
    <span>${esc(label)}</span>
    <div class="track2"><div class="fill${cmp ? ' cmp' : ''}" style="width:0%"></div></div>
    <span class="meta muted">— · 0g</span>
  </div>`;
}
function barRows(rows) {
  const max = Math.max(1, ...rows.map(r => r.games));
  return rows.map(r => barRow(r, max, false)).join('');
}
// Compare bars, aligned by label: for each label show the main bar then the
// compare bar (interleaved), e.g. Safelane(main), Safelane(compare), Offlane…
function barRowsCompare(main, cmp) {
  const byLabel = new Map(cmp.map(r => [r.label, r]));
  const max = Math.max(1, ...main.map(r => r.games), ...cmp.map(r => r.games));
  const seen = new Set();
  let out = '';
  for (const r of main) {
    out += barRow(r, max, false);
    const cr = byLabel.get(r.label);
    if (cr) { out += barRow(cr, max, true); seen.add(r.label); }
    else out += barMissing(r.label, true);
  }
  // Labels the compare user has but the main user doesn't.
  for (const cr of cmp) if (!seen.has(cr.label)) {
    out += barMissing(cr.label, false);
    out += barRow(cr, max, true);
  }
  return out;
}
function bkGroup(title, main, cmp) {
  if (!main.length && !(cmp && cmp.length)) return '';
  const body = cmp ? barRowsCompare(main, cmp) : barRows(main);
  return `<div class="bk-group"><div class="bk-title">${title}</div>${body}</div>`;
}

async function loadBreakdowns() {
  try {
    const [b, bc] = await Promise.all([
      invoke('get_breakdowns', { includeTurbo }),
      fetchCompare('get_breakdowns', { includeTurbo }),
    ]);
    const role = bkGroup('By lane role', b.roles, bc && bc.roles);
    const mode = bkGroup('By game mode', b.modes, bc && bc.modes);
    $('breakdowns').innerHTML = (role + mode) || '<p class="muted">No breakdown data.</p>';
  } catch (e) { $('breakdowns').innerHTML = `<p class="err">breakdowns: ${esc(e)}</p>`; }
}

async function loadTopWinrate() {
  try {
    const [d, dc] = await Promise.all([
      invoke('get_top_winrate', { n: 6, minGames: 5, includeTurbo }),
      fetchCompare('get_top_winrate', { n: 6, minGames: 5, includeTurbo }),
    ]);
    $('topwrTitle').innerHTML = `Highest win rate <span class="muted" style="font-weight:400;text-transform:none;letter-spacing:0">(≥${d.min_games} games)</span>`;
    const width = (h) => h.winrate;
    $('topwr').innerHTML = heroList(d.heroes, dc && dc.heroes, width)
      || '<p class="muted">Not enough games yet.</p>';
  } catch (e) { $('topwr').innerHTML = `<p class="err">top winrate: ${esc(e)}</p>`; }
}

/// One match table. In compare mode the dates lose the year and the KDA and
/// Length columns are hidden by CSS, so two tables fit the card's width.
function recentTable(ms, compact, accountId) {
  const when = compact ? fmtWhenShort : fmtWhen;
  const acct = accountId != null ? ` data-account="${accountId}"` : '';
  const rows = ms.map(m => {
    const tag = m.is_turbo ? '<span class="turbo-tag">Turbo</span>' : '';
    return `<tr data-match="${m.match_id}"${acct}>
      <td>${resTag(m.won)}</td>
      <td class="muted">${esc(when(m.start_time))}</td>
      <td><span class="hcell">${heroIcon(m.icon)}${esc(m.hero)}${tag}</span></td>
      <td>${m.kills}/${m.deaths}/${m.assists}</td>
      <td class="col-kda">${r1(m.kda)}</td>
      <td class="muted col-len">${r0(m.duration / 60)}m</td>
    </tr>`;
  }).join('');
  return `<table>
    <thead><tr><th>Res</th><th>Date / Time</th><th>Hero</th><th>K/D/A</th>
      <th class="col-kda">KDA</th><th class="col-len">Length</th></tr></thead>
    <tbody>${rows}</tbody></table>`;
}

async function loadRecent() {
  try {
    const args = { limit: 12, includeTurbo, heroId: recentHeroFilter };
    const [ms, mc] = await Promise.all([
      invoke('get_recent', args),
      fetchCompare('get_recent', args),
    ]);
    if (!mc) { $('recent').innerHTML = recentTable(ms, false); return; }
    const col = (list, cmp, label) => `<div class="recent-col">
      <div class="who"><span class="dot ${cmp ? 'cool' : 'warm'}"></span>${esc(label)}</div>
      ${list.length ? recentTable(list, true, cmp ? compareSelected : null) : '<p class="muted">No matches.</p>'}
    </div>`;
    $('recent').innerHTML = `<div class="recent-compare">
      ${col(ms, false, mainLabelText())}
      ${col(mc, true, compareLabel)}
    </div>`;
  } catch (e) { $('recent').innerHTML = `<p class="err">recent: ${esc(e)}</p>`; }
}

// One teammate row. `cmp` rows use the cool accent (compared profile).
function peerRow(p, cmp) {
  const avatar = p.avatar
    ? `<img class="pavatar" src="${esc(p.avatar)}" alt="" loading="lazy" onerror="this.style.visibility='hidden'">`
    : `<span class="pavatar"></span>`;
  return `<div class="peer${cmp ? ' cmp' : ''}">
    ${avatar}
    <div class="pname">${esc(p.name)} <span class="muted">· ${p.games}g</span></div>
    <span class="ppct">${r0(p.winrate)}%</span>
  </div>`;
}
// Render a peer list, or an empty-state note.
function peerList(list, cmp) {
  return list.peers.length
    ? list.peers.map(p => peerRow(p, cmp)).join('')
    : '<p class="muted">No teammate data.</p>';
}

async function loadPeers() {
  try {
    // No turbo dimension: /peers isn't mode-filterable.
    const [ps, pc] = await Promise.all([
      invoke('get_peers', { n: 10 }),
      fetchCompare('get_peers', { n: 10 }),
    ]);
    if (!pc) { $('peers').innerHTML = peerList(ps, false); return; }
    const col = (list, cmp, label) => `<div>
      <div class="who"><span class="dot ${cmp ? 'cool' : 'warm'}"></span>${esc(label)}</div>
      ${peerList(list, cmp)}
    </div>`;
    $('peers').innerHTML = `<div class="recent-compare peers-compare">
      ${col(ps, false, mainLabelText())}
      ${col(pc, true, compareLabel)}
    </div>`;
  } catch (e) { $('peers').innerHTML = `<p class="err">peers: ${esc(e)}</p>`; }
}

async function loadHeroList() {
  try { heroIndex = await invoke('get_hero_list'); }
  catch (e) {
    // The dashboard still works without it, but a silent failure here looks
    // like a broken search box, so leave a trace and a visible placeholder.
    console.error('hero list:', e);
    heroIndex = [];
    for (const id of ['heroSearch', 'heroPageSearch']) $(id).placeholder = 'Hero list unavailable';
  }
}
// Best name match: exact → prefix → substring (case-insensitive).
function resolveHero(q) {
  q = q.trim().toLowerCase();
  if (!q) return null;
  return heroIndex.find(h => h.hero.toLowerCase() === q)
      || heroIndex.find(h => h.hero.toLowerCase().startsWith(q))
      || heroIndex.find(h => h.hero.toLowerCase().includes(q)) || null;
}
function openHero(heroId, accountId) {
  heroTurbo = includeTurbo;
  pushView(() => renderHero(heroId, accountId));
}

// Profile/rank are mode-independent; the Turbo toggle only reloads stats.
function loadStats() { loadWinrate(); loadPerformance(); loadHeroes(); loadBreakdowns(); loadPeers(); loadTopWinrate(); loadRecent(); }
function loadAll() { loadProfile(); loadHeroList(); loadStats(); }

/* ---------------- modal view stack ---------------- */
const overlay = $('overlay'), modal = $('modal');
const stack = [];
function closeModal() { overlay.classList.remove('open'); stack.length = 0; }
async function pushView(fn) { stack.push(fn); await renderTop(); }
async function popView() { stack.pop(); stack.length ? renderTop() : closeModal(); }
async function renderTop() {
  overlay.classList.add('open');
  modal.innerHTML = '<div class="loading">Loading…</div>';
  try { modal.innerHTML = await stack[stack.length - 1](); }
  catch (e) { modal.innerHTML = `<div class="mbody err">${esc(e)}</div>`; }
}
function mHead(iconHtml, title, sub, extra) {
  const back = stack.length > 1 ? `<button class="iconbtn" data-act="back" title="Back">‹</button>` : '';
  return `<div class="mhead">${iconHtml}
    <div><h3>${title}</h3><div class="sub">${sub}</div></div>
    <div class="spacer"></div>${extra || ''}${back}
    <button class="iconbtn" data-act="close" title="Close">✕</button></div>`;
}

/* ---- hero drill-down ---- */
async function renderHero(heroId, accountId) {
  const d = await invoke('get_hero_detail', { heroId, includeTurbo: heroTurbo, accountId });
  const sw = `<label class="switch${heroTurbo ? ' on' : ''}" title="Include Turbo matches for this hero">
    <input type="checkbox" id="heroTurbo"${heroTurbo ? ' checked' : ''}/><span class="track"></span><span>Turbo</span></label>`;
  const head = mHead(
    heroIcon(d.icon, '').replace('hicon', 'hicon big'),
    esc(d.hero),
    `${d.games} games · ${r0(d.winrate)}% win · last played ${fmtDay(d.last_played)}`,
    sw);
  const avgNote = `last ${d.sample} games`;
  // GPM/XPM/LH ride on fields OpenDota omits for unparsed games, so they can
  // rest on a smaller sample than the K/D/A tiles beside them.
  const econNote = d.economy_sample && d.economy_sample !== d.sample
    ? ` · economy from ${d.economy_sample}` : '';
  const tiles = [
    ['KDA', r1(d.avg_kda)],
    ['K / D / A', `${r1(d.avg_kills)}/${r1(d.avg_deaths)}/${r1(d.avg_assists)}`],
    ['GPM', r0(d.avg_gpm)],
    ['XPM', r0(d.avg_xpm)],
    ['Last hits', r0(d.avg_last_hits)],
    ['Hero dmg', kfmt(d.avg_hero_damage)],
  ];
  const grid = `<div class="avgs">${tiles.map(([l, v]) =>
    `<div class="avg"><div class="num">${v}</div><div class="lbl">${l}</div></div>`).join('')}</div>`;
  const matchAcct = accountId != null ? ` data-account="${accountId}"` : '';
  const rows = d.matches.map(m => `<tr data-match="${m.match_id}"${matchAcct}>
    <td>${resTag(m.won)}</td>
    <td class="muted">${esc(fmtWhen(m.start_time))}</td>
    <td>${m.kills}/${m.deaths}/${m.assists}</td>
    <td>${r1(m.kda)}</td>
    <td>${m.gpm ?? '—'}</td>
    <td>${m.xpm ?? '—'}</td>
    <td class="muted">${r0(m.duration / 60)}m${m.is_turbo ? ' <span class="turbo-tag">T</span>' : ''}</td>
  </tr>`).join('');
  const table = d.matches.length ? `
    <div class="teamtitle">Recent matches on this hero</div>
    <table><thead><tr><th>Res</th><th>Date / Time</th><th>K/D/A</th><th>KDA</th><th>GPM</th><th>XPM</th><th>Length</th></tr></thead>
    <tbody>${rows}</tbody></table>` : '<p class="muted">No recent matches.</p>';
  const overview = `<div class="ov">
    <div class="wheel">${donutSVG(d.winrate, 'WIN RATE', 116)}
      <div class="muted" style="font-size:12px;margin-top:2px">${d.win} / ${d.games} won</div></div>
    <div class="grow">${grid}</div>
  </div>`;
  return head + `<div class="mbody">
    <div class="teamtitle">Averages <span class="muted" style="font-weight:400;text-transform:none;letter-spacing:0">(${avgNote}${econNote})</span></div>
    ${overview}
    ${heroGraphs(d.matches)}
    ${table}</div>`;
}

/* ---- match drill-down ---- */
function advChart(gold) {
  if (!Array.isArray(gold) || gold.length < 2) {
    return '<div class="advnote">Gold/XP graph unavailable — this match was not parsed by OpenDota.</div>';
  }
  const w = 820, h = 150, pad = 14, mid = h / 2;
  const maxAbs = Math.max(1, ...gold.map(v => Math.abs(v)));
  const pts = gold.map((v, i) => {
    const x = pad + i / (gold.length - 1) * (w - 2 * pad);
    const y = mid - (v / maxAbs) * (mid - pad);
    return `${x.toFixed(1)},${y.toFixed(1)}`;
  }).join(' ');
  const area = `${pad},${mid} ${pts} ${(w - pad)},${mid}`;
  return `
    <div class="adv-leg"><span><b style="color:var(--radiant)">▲</b> Radiant lead</span>
      <span><b style="color:var(--dire)">▼</b> Dire lead</span>
      <span>peak ${kfmt(maxAbs)} gold</span></div>
    <svg width="100%" viewBox="0 0 ${w} ${h}" preserveAspectRatio="none" style="display:block">
      <defs><linearGradient id="advg" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0%" stop-color="var(--radiant)" stop-opacity=".30"/>
        <stop offset="50%" stop-color="var(--radiant)" stop-opacity=".04"/>
        <stop offset="100%" stop-color="var(--dire)" stop-opacity=".22"/>
      </linearGradient></defs>
      <polygon points="${area}" fill="url(#advg)"/>
      <line x1="${pad}" y1="${mid}" x2="${w - pad}" y2="${mid}" stroke="var(--line)" stroke-dasharray="4 4"/>
      <polyline points="${pts}" fill="none" stroke="var(--gold)" stroke-width="2"/>
    </svg>`;
}

function scoreRow(p) {
  return `<tr class="${p.is_me ? 'me' : ''}">
    <td><span class="hcell">${heroIcon(p.icon)}${esc(p.hero)}</span></td>
    <td class="muted">${esc(p.name || '—')}</td>
    <td>${p.level ?? '—'}</td>
    <td>${p.kills}/${p.deaths}/${p.assists}</td>
    <td>${p.net_worth != null ? kfmt(p.net_worth) : '—'}</td>
    <td>${p.gpm ?? '—'}</td>
    <td>${p.xpm ?? '—'}</td>
    <td class="muted">${p.last_hits ?? '—'}/${p.denies ?? '—'}</td>
    <td>${p.hero_damage != null ? kfmt(p.hero_damage) : '—'}</td>
  </tr>`;
}
function teamTable(players, side, label, win) {
  const rows = players.filter(p => p.radiant === side).map(scoreRow).join('');
  const tag = win === true ? ' · Victory' : win === false ? ' · Defeat' : '';
  return `<div class="teamtitle ${side ? 'radiant' : 'dire'}">${label}${tag}</div>
    <table><thead><tr><th>Hero</th><th>Player</th><th>Lv</th><th>K/D/A</th><th>Net</th><th>GPM</th><th>XPM</th><th>LH/DN</th><th>Dmg</th></tr></thead>
    <tbody>${rows}</tbody></table>`;
}
async function renderMatch(matchId, accountId) {
  const m = await invoke('get_match_detail', { matchId, accountId });
  const rWin = m.radiant_win;
  const winner = rWin === true ? '<span style="color:var(--radiant)">Radiant Victory</span>'
               : rWin === false ? '<span style="color:var(--dire)">Dire Victory</span>' : 'Result unknown';
  const score = (m.radiant_score != null && m.dire_score != null)
    ? `${m.radiant_score} – ${m.dire_score}` : '';
  const head = mHead(
    `<div class="big" style="display:flex;align-items:center;justify-content:center;font-size:22px">⚔</div>`,
    winner,
    `${fmtWhen(m.start_time)} · ${r0(m.duration / 60)}m${score ? ' · ' + score : ''} · #${m.match_id}`);
  return head + `<div class="mbody">
    ${advChart(m.gold_adv)}
    ${teamTable(m.players, true, 'Radiant', rWin === true)}
    ${teamTable(m.players, false, 'Dire', rWin === false)}
  </div>`;
}

/* ---------------- profiles (user IDs) ---------------- */
const STAT_IDS = ['winrate', 'performance', 'heroes', 'breakdowns', 'topwr', 'recent'];

function populateUserSelect(payload) {
  lastUsers = payload;
  const sel = $('userSelect');
  sel.innerHTML = payload.profiles.map(p =>
    `<option value="${p.account_id}">${esc(p.label)}</option>`).join('');
  if (payload.selected != null) sel.value = String(payload.selected);
  sel.style.display = payload.profiles.length ? '' : 'none';
  populateCompareSelect(payload);
}

// Compare dropdown lists every saved profile except the active one. Hidden
// when there's no second profile to compare against.
function populateCompareSelect(payload) {
  const sel = $('compareSelect');
  const others = payload.profiles.filter(p => p.account_id !== payload.selected);
  sel.innerHTML = `<option value="">Compare: none</option>` +
    others.map(p => `<option value="${p.account_id}">vs ${esc(p.label)}</option>`).join('');
  // Keep the current comparison if it's still a valid non-active profile.
  const keep = others.find(p => p.account_id === compareSelected);
  if (keep) { sel.value = String(compareSelected); compareLabel = keep.label; }
  else { compareSelected = null; compareLabel = ''; sel.value = ''; }
  sel.style.display = others.length ? '' : 'none';
  updateCompareLegend();
}

function updateCompareLegend() {
  const el = $('compareLegend');
  if (compareSelected == null) { el.classList.remove('on'); el.innerHTML = ''; return; }
  el.classList.add('on');
  el.innerHTML = `<span class="dot warm"></span>${esc(mainLabelText())} <span class="muted">(you)</span>
    <span class="dot cool"></span>${esc(compareLabel)}`;
}

// No profile configured: prompt to add one instead of firing failing stats.
function showEmptyState() {
  $('profileMain').innerHTML = `<div>
    <div class="pname">No profiles yet</div>
    <div class="psub">Add your Steam32 / Dota friend id to get started.
      <button id="emptyAdd">+ Add a Dota ID</button></div></div>`;
  STAT_IDS.forEach(id => $(id).innerHTML = '<p class="muted">No profile selected.</p>');
}

// Reflect a fresh users payload everywhere; reload stats if the active id moved.
function applyUsers(payload) {
  populateUserSelect(payload);
  if (payload.selected !== currentSelected) {
    currentSelected = payload.selected;
    if (currentSelected != null) loadAll(); else showEmptyState();
  }
}

function openUsersEditor() { pushView(renderUsersEditor); }

async function renderUsersEditor() {
  const data = await invoke('list_users');
  const rows = data.profiles.map(p => `
    <div class="urow">
      <span class="ulabel">${esc(p.label)}</span>
      <span class="uid muted">${p.account_id}</span>
      <button class="iconbtn" data-remove="${p.account_id}" title="Remove">✕</button>
    </div>`).join('') || '<p class="muted">No profiles saved yet.</p>';
  const head = mHead(
    `<div class="big" style="display:flex;align-items:center;justify-content:center;font-size:22px">⚙</div>`,
    'Edit Dota IDs',
    'Profiles shown in the dropdown — stored locally, never committed');
  return head + `<div class="mbody">
    <div class="teamtitle">Add a profile</div>
    <div class="addform">
      <input id="userLabel" class="search" placeholder="Label (e.g. Main)" autocomplete="off" />
      <input id="userAccountId" class="search" placeholder="account_id (Steam32)" inputmode="numeric" autocomplete="off" />
      <button id="userAddBtn">+ Add</button>
    </div>
    <p class="err" id="userAddErr" style="display:none;margin-top:8px"></p>
    <div class="teamtitle" style="margin-top:16px">Saved profiles</div>
    <div class="ulist">${rows}</div>
  </div>`;
}

async function submitAddUser() {
  const labelEl = $('userLabel'), idEl = $('userAccountId'), errEl = $('userAddErr');
  const account_id = parseInt((idEl.value || '').trim(), 10);
  if (!Number.isInteger(account_id) || account_id <= 0) {
    errEl.textContent = 'Enter a valid numeric account_id (Steam32 / Dota friend id).';
    errEl.style.display = 'block';
    idEl.classList.add('miss'); setTimeout(() => idEl.classList.remove('miss'), 800);
    return;
  }
  try {
    const payload = await invoke('add_user', { label: labelEl.value.trim(), accountId: account_id });
    applyUsers(payload);
    await renderTop();
  } catch (e) {
    errEl.textContent = String(e); errEl.style.display = 'block';
  }
}

async function doRemoveUser(account_id) {
  try {
    const payload = await invoke('remove_user', { accountId: account_id });
    applyUsers(payload);
    await renderTop();
  } catch (e) {
    // Re-rendering would wipe the message, so write it into the open editor.
    const err = $('userAddErr');
    if (err) { err.textContent = `Could not remove profile: ${e}`; err.style.display = 'block'; }
  }
}

/* ---------------- events ---------------- */
// A row carries data-account only when it belongs to the compared profile.
const rowAccount = (el) => el.dataset.account ? Number(el.dataset.account) : undefined;
// Every click inside the modal is dispatched here, most specific first.
// Two separate listeners used to split this; their relative order was load-
// bearing but invisible, so an early `return` in one silently shadowed the other.
overlay.addEventListener('click', (e) => {
  if (e.target === overlay) return closeModal();
  const act = e.target.closest('[data-act]');
  if (act) { act.dataset.act === 'back' ? popView() : closeModal(); return; }
  if (e.target.closest('#userAddBtn')) return submitAddUser();
  const rm = e.target.closest('[data-remove]');
  if (rm) return doRemoveUser(Number(rm.dataset.remove));
  const mrow = e.target.closest('[data-match]');
  if (mrow) return pushView(() => renderMatch(Number(mrow.dataset.match), rowAccount(mrow)));
});
document.addEventListener('keydown', (e) => { if (e.key === 'Escape' && overlay.classList.contains('open')) popView(); });

const heroClick = (e) => {
  const h = e.target.closest('[data-hero]');
  if (h) openHero(Number(h.dataset.hero), rowAccount(h));
};
$('heroes').addEventListener('click', heroClick);
$('topwr').addEventListener('click', heroClick);
/* ---- reusable icon-dropdown hero search ---- */
function setupHeroSearch(input, box, { onSelect, onClear }) {
  let items = [], active = -1;
  const hide = () => { box.classList.remove('open'); active = -1; };
  function render(q) {
    q = q.trim().toLowerCase();
    if (!q) { hide(); return; }
    items = heroIndex
      .filter(h => h.hero.toLowerCase().includes(q))
      .sort((a, b) => {
        const ap = a.hero.toLowerCase().startsWith(q) ? 0 : 1;
        const bp = b.hero.toLowerCase().startsWith(q) ? 0 : 1;
        return ap - bp || a.hero.localeCompare(b.hero);
      })
      .slice(0, 8);
    if (!items.length) { hide(); return; }
    active = 0;
    box.innerHTML = items.map((h, i) =>
      `<div class="opt${i === 0 ? ' active' : ''}" data-i="${i}">${heroIcon(h.icon)}<span>${esc(h.hero)}</span></div>`
    ).join('');
    box.classList.add('open');
  }
  function setActive(i) {
    const opts = [...box.querySelectorAll('.opt')];
    if (!opts.length) return;
    active = (i + opts.length) % opts.length;
    opts.forEach((o, k) => o.classList.toggle('active', k === active));
    opts[active].scrollIntoView({ block: 'nearest' });
  }
  function pick() {
    const h = (active >= 0 && items[active]) ? items[active] : resolveHero(input.value);
    if (h) { hide(); onSelect(h); }
    else { input.classList.add('miss'); setTimeout(() => input.classList.remove('miss'), 800); }
  }
  input.addEventListener('input', (e) => {
    if (!e.target.value.trim()) { hide(); onClear && onClear(); return; }
    render(e.target.value);
  });
  input.addEventListener('keydown', (e) => {
    if (e.key === 'ArrowDown') { e.preventDefault(); setActive(active + 1); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); setActive(active - 1); }
    else if (e.key === 'Escape') { hide(); }
    else if (e.key === 'Enter') { e.preventDefault(); pick(); }
  });
  // mousedown (not click) so it fires before the input's blur hides the box.
  box.addEventListener('mousedown', (e) => {
    const opt = e.target.closest('.opt');
    if (opt) { e.preventDefault(); hide(); onSelect(items[Number(opt.dataset.i)]); }
  });
  input.addEventListener('blur', () => setTimeout(hide, 120));
}

// Recent-matches search → filters the table in place.
function selectRecentHero(h) {
  recentHeroFilter = h.hero_id;
  $('heroSearch').value = h.hero;
  $('recentTitle').innerHTML =
    `Recent matches <span class="muted" style="font-weight:400;text-transform:none;letter-spacing:0">· ${esc(h.hero)}</span>` +
    `<span class="filter-clear" id="clearFilter" title="Show all heroes">✕</span>`;
  loadRecent();
}
function clearFilter() {
  recentHeroFilter = null;
  $('heroSearch').value = '';
  $('recentTitle').textContent = 'Recent matches';
  loadRecent();
}
setupHeroSearch($('heroSearch'), $('heroSuggest'), {
  onSelect: selectRecentHero,
  onClear: () => { if (recentHeroFilter !== null) clearFilter(); },
});
$('recentTitle').addEventListener('click', (e) => { if (e.target.id === 'clearFilter') clearFilter(); });

// Most-played-heroes search → opens the hero stats modal.
const heroPageInput = $('heroPageSearch');
setupHeroSearch(heroPageInput, $('heroPageSuggest'), {
  onSelect: (h) => { heroPageInput.value = ''; openHero(h.hero_id); },
});
// In-view Turbo switch re-fetches the hero detail in place.
overlay.addEventListener('change', (e) => {
  if (e.target.id === 'heroTurbo') { heroTurbo = e.target.checked; renderTop(); }
});
$('recent').addEventListener('click', (e) => {
  const r = e.target.closest('[data-match]');
  if (r) pushView(() => renderMatch(Number(r.dataset.match), rowAccount(r)));
});

// Refresh must drop the cache first: within the TTL, reloading alone would
// re-read the same JSON from disk and appear to do nothing.
async function hardRefresh() {
  const btn = $('refresh');
  btn.disabled = true;
  const label = btn.textContent;
  btn.textContent = '↻ Refreshing…';
  try { await invoke('clear_cache'); }
  catch (e) { /* a stale cache still renders; loadAll reports real failures */ }
  finally { btn.disabled = false; btn.textContent = label; }
  loadAll();
}
$('refresh').addEventListener('click', hardRefresh);
$('turbo').addEventListener('change', (e) => {
  includeTurbo = e.target.checked;
  $('turboSwitch').classList.toggle('on', includeTurbo);
  loadStats();
});

// Profile selector + IDs editor wiring.
$('userSelect').addEventListener('change', async (e) => {
  const account_id = Number(e.target.value);
  try {
    await invoke('select_user', { accountId: account_id });
    currentSelected = account_id;
    lastUsers.selected = account_id;
    // Rebuild the compare list so it can't offer the now-active profile.
    populateCompareSelect(lastUsers);
    loadAll();
  } catch (err) {
    // The store rejected the switch, so snap the dropdown back to the profile
    // that is actually active instead of leaving it showing a lie.
    if (currentSelected != null) e.target.value = String(currentSelected);
    $('profileMain').innerHTML = `<p class="err">could not switch profile: ${esc(err)}</p>`;
  }
});
// Compare-to selector: pick a second profile (or "none" to turn it off).
$('compareSelect').addEventListener('change', (e) => {
  const v = e.target.value;
  compareSelected = v ? Number(v) : null;
  compareLabel = compareSelected != null ? e.target.selectedOptions[0].text.replace(/^vs\s+/, '') : '';
  updateCompareLegend();
  loadProfile();
  loadStats();
});
$('editUsers').addEventListener('click', openUsersEditor);
$('profile').addEventListener('click', (e) => {
  if (e.target.id === 'emptyAdd') openUsersEditor();
});
overlay.addEventListener('keydown', (e) => {
  if (e.key === 'Enter' && (e.target.id === 'userAccountId' || e.target.id === 'userLabel')) {
    e.preventDefault(); submitAddUser();
  }
});

// Startup: load profiles first; show the dashboard only once one is selected.
async function init() {
  try {
    const payload = await invoke('list_users');
    populateUserSelect(payload);
    currentSelected = payload.selected;
    if (currentSelected != null) { loadAll(); }
    else { showEmptyState(); openUsersEditor(); }
  } catch (e) {
    $('profileMain').innerHTML = `<p class="err">profiles: ${esc(e)}</p>`;
  }
}

init();
