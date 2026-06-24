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
minutes never hammer OpenDota. Cache lives in `~/.cache/dota-stats/`, config in
`~/.config/dota-stats/config.toml`.

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

## Config

First run writes a template `~/.config/dota-stats/config.toml` and exits,
asking you to set your `account_id` (Steam32 / Dota friend id). Edit the file,
then run again. See `config.example.toml`.

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
