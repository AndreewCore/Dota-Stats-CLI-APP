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
   dota-stats-cli        dota-stats (native window)
   (waybar/eww/polybar)
```

One cached core feeds both the GUI and the CLI, so widgets refreshing every few
minutes never hammer OpenDota. Both live in the OS' own per-user directories:

| | Linux | Windows |
|---|---|---|
| Saved profiles | `~/.config/dota-stats/users.json` | `%APPDATA%\dota-stats\users.json` |
| Cache | `~/.cache/dota-stats/` | `%LOCALAPPDATA%\dota-stats\cache\` |

`XDG_CONFIG_HOME` / `XDG_CACHE_HOME` override both on every platform, Windows
included — handy for keeping a portable copy self-contained.

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

### Linux

```bash
# Toolchain + Tauri deps (official Arch repos, no AUR):
sudo pacman -S --needed rustup webkit2gtk-4.1 gtk3 libsoup3 librsvg base-devel
rustup default stable

cargo build --release --locked
# GUI binary: target/release/dota-stats
# CLI binary: target/release/dota-stats-cli
```

### Windows

Tauri renders through the system WebView2, so there is no gtk/webkit stack to
install — just the Rust toolchain and the MSVC linker:

1. [Visual Studio Build Tools](https://visualstudio.microsoft.com/downloads/)
   with the **Desktop development with C++** workload (supplies `link.exe`).
2. [rustup](https://rustup.rs) → `rustup default stable`.
3. **WebView2 runtime** — preinstalled on Windows 11 and up-to-date Windows 10;
   otherwise grab the Evergreen bootstrapper from Microsoft.

```powershell
cargo build --release --locked
# GUI binary: target\release\dota-stats.exe
# CLI binary: target\release\dota-stats-cli.exe
```

## Run the desktop app

The frontend is a static folder (no dev server), so the GUI runs straight from
cargo — no `npm`, no `tauri-cli`:

```bash
cargo run -p dota-stats-app           # quick run (debug)
# or, after a release build:
./target/release/dota-stats           # target\release\dota-stats.exe on Windows
```

A 1000×780 window titled **Dota 2 Stats** opens. On first launch it has no
account yet — see [Profiles](#profiles) below.

## Install

### Linux (make)

Nothing here downloads external tooling: no `tauri-cli`, no AppImage helpers.
`make` and `cargo` are enough, plus `makepkg` for the Arch package.

```bash
make install          # both binaries into ~/.local (no root needed)
make install-gui      # only the desktop app
make install-cli      # only the terminal client
make uninstall        # remove everything again
```

Afterwards `dota-stats` opens the window and `dota-stats-cli` prints stats.
Requires `~/.local/bin` on your `PATH`; override the location with
`make install PREFIX=/somewhere`.

`dota-stats` on `PATH` is a small wrapper that detaches the app from the
terminal, so launching it from a shell returns the prompt immediately instead of
blocking until you close the window. Pass `--foreground` to keep it attached
(handy for reading panics). The real binary sits in `<prefix>/lib/dota-stats/`.

Installing also registers a **Dota 2 Stats** entry in your application menu.

### Arch package

```bash
make package          # builds a .pkg.tar.zst in packaging/
cd packaging && makepkg -si
```

This gives you a pacman-managed install (`pacman -R dota-stats` to remove it)
under `/usr` instead of `~/.local`.

### Windows (portable zip)

There is no installer. Grab `dota-stats-windows-x86_64.zip` from the
[releases page](https://github.com/AndreewCore/Dota-Stats-CLI-APP/releases),
unzip it anywhere you like and run `dota-stats.exe`. The zip holds both
binaries; `dota-stats-cli.exe` is used from a terminal opened in that folder,
or from anywhere once you add the folder to your `PATH`.

The executables are unsigned, so the first launch shows a SmartScreen warning —
*More info → Run anyway* clears it for good. Uninstalling is deleting the
folder; the profiles under `%APPDATA%\dota-stats\` and the cache under
`%LOCALAPPDATA%\dota-stats\` are left behind and can be removed by hand.

If no release carries the zip yet, run the **windows** workflow from the repo's
Actions tab (`Run workflow`) and download the artifact it leaves behind — or
build it yourself with the steps under [Build → Windows](#windows).

## Profiles

There is **no account baked into the repo** — it ships empty. On first launch
the app opens the **Edit IDs** editor: add your Steam32 / Dota friend id (with a
label like "Main"), and it's saved to `~/.config/dota-stats/users.json`
(`%APPDATA%\dota-stats\users.json` on Windows). Add as
many profiles as you like and switch between them with the dropdown in the top
bar. The file lives in your OS config dir and is never committed — the repo only
ships the empty `users.example.json` template.

From the CLI you can manage the same list:

```bash
dota-stats-cli add 123456 Main    # save a profile (first one becomes active)
dota-stats-cli users              # list profiles (★ = active)
dota-stats-cli use 123456         # switch the active profile
dota-stats-cli remove 123456      # delete a profile
```

## CLI usage

```bash
dota-stats-cli profile            # name, rank/medal, MMR estimate (if any)
dota-stats-cli rank               # rank medal + stars
dota-stats-cli winrate            # overall W/L and win %
dota-stats-cli heroes --n 5       # top N most-played heroes
dota-stats-cli top-hero           # single most-played hero
dota-stats-cli recent --limit 10  # recent matches with KDA
dota-stats-cli widget <metric>    # one-line JSON for bars: mmr|rank|winrate|top-hero
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
    "exec": "dota-stats-cli widget winrate",
    "return-type": "json",
    "interval": 600,
    "tooltip": true
}
```

### eww

```lisp
(defpoll dota-mmr :interval "10m" "dota-stats-cli rank")
(label :text dota-mmr)
```

### polybar

```ini
[module/dota]
type = custom/script
exec = dota-stats-cli widget winrate | jq -r .text
interval = 600
```

---
