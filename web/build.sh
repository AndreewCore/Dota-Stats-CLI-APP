#!/bin/sh
# Assemble a static web build from the shared desktop UI (app/ui).
#
#   sh web/build.sh site   ->  web/site/dist   plain site, deliberately NOT installable
#   sh web/build.sh pwa    ->  web/pwa/dist    same dashboard + manifest + service worker
#
# No Node toolchain: the UI is vanilla JS, so a build is copying files and
# filling two markers in index.html. Vercel runs this from web/<target>/.
set -eu

target="${1:-}"
case "$target" in
  site|pwa) ;;
  *) echo "usage: sh web/build.sh site|pwa" >&2; exit 2 ;;
esac

web="$(cd "$(dirname "$0")" && pwd)"
ui="$web/../app/ui"
out="$web/$target/dist"

# A marker that silently stopped matching would ship a page with no backend.
for marker in '<!-- web:head -->' '<!-- web:scripts -->' '<html lang="en">'; do
  grep -qF "$marker" "$ui/index.html" || { echo "index.html lost marker: $marker" >&2; exit 1; }
done

rm -rf "$out"
mkdir -p "$out"
cp -R "$ui/." "$out/"
cp "$web/src/backend.js" "$web/src/web.js" "$out/"

head=''
if [ "$target" = pwa ]; then
  cp "$web/pwa/manifest.webmanifest" "$out/"
  cp -R "$web/pwa/icons" "$out/icons"
  head='<link rel="manifest" href="manifest.webmanifest" /><meta name="theme-color" content="#0c1014" /><link rel="apple-touch-icon" href="icons/icon-192.png" />'
fi

sed -e "s|<html lang=\"en\">|<html lang=\"en\" data-target=\"$target\">|" \
    -e "s|<!-- web:head -->|$head|" \
    -e 's|<!-- web:scripts -->|<script src="backend.js"></script><script src="web.js"></script>|' \
    "$ui/index.html" > "$out/index.html"

if [ "$target" = pwa ]; then
  # Precache every shipped file, and version the cache by their content so a
  # deploy that changes any asset replaces the old offline copy.
  files="$(cd "$out" && find . -type f | sed 's|^\./||' | LC_ALL=C sort)"
  version="$(cd "$out" && printf '%s\n' "$files" | while IFS= read -r f; do cat "$f"; done | cksum | cut -d' ' -f1)"
  list="\"./\",$(printf '%s\n' "$files" | sed 's/.*/"&"/' | paste -sd, -)"
  sed -e "s|__VERSION__|$version|" -e "s|__PRECACHE__|$list|" "$web/pwa/sw.js" > "$out/sw.js"
fi

echo "built $target -> $out"
