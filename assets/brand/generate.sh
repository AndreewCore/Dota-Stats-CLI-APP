#!/bin/sh
# Derive every shipped icon from the one brand master (assets/brand/logo.png).
#
#   sh assets/brand/generate.sh
#
# Run it after changing the master and commit whatever it rewrites; nothing
# builds these on the fly, so the derived files are checked in.
#
# Requires ImageMagick 7 (`magick`).
set -eu

here="$(cd "$(dirname "$0")" && pwd)"
root="$here/../.."
src="$here/logo.png"

command -v magick >/dev/null || { echo "magick (ImageMagick 7) not found" >&2; exit 1; }
[ -f "$src" ] || { echo "missing master: $src" >&2; exit 1; }

# The master is flat RGB: its white is background, not part of the artwork.
# Flood-filling from the four corners clears it while leaving the white *inside*
# the logo (the lens highlight, the wedges in the red square) alone, since the
# black ring and the red field seal those regions off.
flat="$(mktemp -t brandXXXXXX.png)"
sq="$(mktemp -t brandXXXXXX.png)"
trap 'rm -f "$flat" "$sq"' EXIT
w="$(magick identify -format '%[fx:w-1]' "$src")"
h="$(magick identify -format '%[fx:h-1]' "$src")"
magick "$src" -alpha set -fuzz 12% -fill none \
  -draw 'alpha 0,0 floodfill' \
  -draw "alpha $w,0 floodfill" \
  -draw "alpha 0,$h floodfill" \
  -draw "alpha $w,$h floodfill" \
  -trim +repage "$flat"

# Every size is cut from one 1024px square so they all share the same framing:
# the logo centred with a hair of breathing room, so neither the lens nor the
# handle butts against the edge at small sizes.
magick "$flat" -resize 944x944 -background none -gravity center -extent 1024x1024 "$sq"

# square <out> <size> — a transparent PNG at that size.
square() {
  magick "$sq" -resize "${2}x${2}" -strip "$1"
}

# opaque <out> <size> <inset%> — the logo inset and flattened onto the app
# background, for platforms that render icons without alpha (iOS home screen,
# Android's maskable crop).
opaque() {
  inner=$(( $2 - $2 * $3 / 100 * 2 ))
  magick "$flat" -resize "${inner}x${inner}" -background '#0c1014' -gravity center \
    -extent "${2}x${2}" -alpha remove -alpha off -strip "$1"
}

# ico <out> <size>... — one multi-resolution .ico holding each size given.
ico() {
  out="$1"; shift
  set -- $(for s in "$@"; do printf '( -clone 0 -resize %sx%s ) ' "$s" "$s"; done)
  magick "$sq" "$@" -delete 0 "$out"
}

# Tauri bundle (app/tauri.conf.json lists these four).
square "$root/app/icons/32x32.png" 32
square "$root/app/icons/128x128.png" 128
square "$root/app/icons/icon.png" 512
ico "$root/app/icons/icon.ico" 16 32 48 64 128 256

# Shared UI (app/ui), copied verbatim into both web builds by web/build.sh.
square "$root/app/ui/logo.png" 256
ico "$root/app/ui/favicon.ico" 16 32 48
opaque "$root/app/ui/apple-touch-icon.png" 180 8

# PWA manifest icons (web/pwa/icons). The maskable one keeps the logo inside the
# safe zone so Android's circular crop doesn't cut the magnifier off.
square "$root/web/pwa/icons/icon-192.png" 192
square "$root/web/pwa/icons/icon-512.png" 512
opaque "$root/web/pwa/icons/icon-maskable-512.png" 512 20

echo "regenerated icons from $src"
