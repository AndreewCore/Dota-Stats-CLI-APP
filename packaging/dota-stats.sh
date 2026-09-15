#!/usr/bin/env sh
# Launcher for the Dota 2 Stats desktop app.
#
# The real GUI binary lives in <prefix>/lib/dota-stats/ so this wrapper can own
# the `dota-stats` name on PATH. It detaches the app from the calling terminal
# (setsid) so running it from kitty returns the prompt immediately instead of
# blocking until the window closes.
set -eu

# Resolve <prefix> from this script's own location, so the same file works
# whether it was installed to /usr/bin or ~/.local/bin.
self=$(readlink -f "$0")
prefix=$(dirname "$(dirname "$self")")
real="$prefix/lib/dota-stats/dota-stats"

if [ ! -x "$real" ]; then
    echo "dota-stats: GUI binary not found at $real" >&2
    exit 1
fi

# --foreground keeps the app attached to this terminal (useful for debugging);
# without it the process is detached and the prompt returns at once.
if [ "${1-}" = "--foreground" ]; then
    shift
    exec "$real" "$@"
fi

setsid -f "$real" "$@" >/dev/null 2>&1
