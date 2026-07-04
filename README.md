# Dota 2 Stats

Personal Dota 2 dashboard built on the [OpenDota API](https://docs.opendota.com):
MMR/rank estimate, winrate, most-played heroes, KDA, and recent matches — as a
native desktop app **and** a CLI you can wire into your bar/widgets.

## Architecture

```
        OpenDota API  (remote, rate-limited)
              |
   crates/core  ──  OpenDota client + on-disk TTL cache + models   (source of truth)
        |                      |
   crates/cli            app/ (Tauri)
   dota-stats binary     native window dashboard
   (waybar/eww/polybar)
```

One cached core feeds both the GUI and the CLI, so widgets refreshing every few
minutes never hammer OpenDota. Cache lives in `~/.cache/dota-stats/`, saved
profiles in `~/.config/dota-stats/users.json`.

### When a real API request is made

The app does **not** call OpenDota on every click. Every request goes through
the on-disk TTL cache in `~/.cache/dota-stats/`: a network `GET` to
`api.opendota.com` happens **only on a cache miss** — i.e. the first time an
endpoint is needed, or after its cached entry has expired. A fresh cache entry
is served locally with no network traffic.

Cache entries are keyed per `account_id` (and separately for Turbo vs. core
stats), so navigating tabs, opening hero/match modals, and re-rendering views
reuse the cache and hit the network zero times while entries are still fresh.

Per-endpoint TTLs:

| Data | TTL |
|------|-----|
| Profile / MMR / rank | 6 hours |
| Win-loss, totals, counts (breakdowns) | 30 min |
| Heroes | 1 hour |
| Recent / hero matches | 10 min |
| Match detail + hero constants | ~7 days |

A real request set is therefore triggered by:
- **First load** after the cache is cold — fans out to the ~7–8 endpoints the
  dashboard needs (only those whose entries are missing/expired).
- **A cache entry aging past its TTL**, on the next view that needs it.
- **Switching profiles** — a different `account_id` means new cache keys, so
  that player's data is fetched.
- **Toggling Turbo** — Turbo and core stats use separate cache keys, so the
  first toggle refetches the affected endpoints.

> **Note:** the **↻ Refresh** button re-reads through the cache; it does *not*
> bypass it, so it only produces network calls for entries whose TTL has already
> expired. It will not force fresh data while entries are still within their TTL.

### Supply-chain notes
- HTTP via **`ureq`** (rustls, no OpenSSL; no tokio/hyper tree).
- CLI arg parsing is hand-rolled — no `clap`.
- The Tauri frontend is **vanilla HTML/CSS/JS — no npm/Node build step**.
- Build with `cargo build --locked`; `Cargo.lock` is committed.

## Build

```bash
# Toolchain + Tauri deps (official Arch repos, no AUR):
sudo pacman -S --needed rustup webkit2gtk-4.1 gtk3 libsoup3 librsvg base-devel
rustup default stable

cargo build --release --locked
# CLI binary: target/release/dota-stats
```

## Run the desktop app

The frontend is a static folder (no dev server), so the GUI runs straight from
cargo — no `npm`, no `tauri-cli`:

```bash
cargo run -p dota-stats-app           # quick run (debug)
# or, after a release build:
./target/release/dota-stats-app
```

A 1000×780 window titled **Dota 2 Stats** opens. On first launch it has no
account yet — see [Profiles](#profiles) below.

## Profiles

There is **no account baked into the repo** — it ships empty. On first launch
the app opens the **Edit IDs** editor: add your Steam32 / Dota friend id (with a
label like "Main"), and it's saved to `~/.config/dota-stats/users.json`. Add as
many profiles as you like and switch between them with the dropdown in the top
bar. The file lives in your OS config dir and is never committed — the repo only
ships the empty `users.example.json` template.

From the CLI you can manage the same list:

```bash
dota-stats add 123456 Main    # save a profile (first one becomes active)
dota-stats users              # list profiles (★ = active)
dota-stats use 123456         # switch the active profile
dota-stats remove 123456      # delete a profile
```

## CLI usage

```bash
dota-stats profile            # name, rank/medal, MMR estimate (if any)
dota-stats rank               # rank medal + stars
dota-stats winrate            # overall W/L and win %
dota-stats heroes --n 5       # top N most-played heroes
dota-stats top-hero           # single most-played hero
dota-stats recent --limit 10  # recent matches with KDA
dota-stats widget <metric>    # one-line JSON for bars: mmr|rank|winrate|top-hero
```

Add `--json` to most commands for machine-readable output.

> **Note:** OpenDota has deprecated numeric `mmr_estimate` for most accounts, so
> `mmr`/`profile` may show `n/a` for the number. The rank **medal** is the
> reliable signal; `widget mmr` falls back to showing the medal.

## Widget integration

All three bar tools are command-driven. The `widget` subcommand emits
`{"text": "...", "tooltip": "..."}` for waybar; plain commands suit eww/polybar.

### waybar

```jsonc
"custom/dota": {
    "exec": "dota-stats widget winrate",
    "return-type": "json",
    "interval": 600,
    "tooltip": true
}
```

### eww

```lisp
(defpoll dota-mmr :interval "10m" "dota-stats rank")
(label :text dota-mmr)
```

### polybar

```ini
[module/dota]
type = custom/script
exec = dota-stats widget winrate | jq -r .text
interval = 600
```

---
