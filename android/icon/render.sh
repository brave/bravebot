#!/usr/bin/env bash
# Rasterise the launcher icon's foreground layer at each density Android asks for (108dp).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
res="$here/../app/src/main/res"
for pair in mdpi:108 hdpi:162 xhdpi:216 xxhdpi:324 xxxhdpi:432; do
  mkdir -p "$res/mipmap-${pair%%:*}"
  rsvg-convert -w "${pair##*:}" -h "${pair##*:}" "$here/foreground.svg" -o "$res/mipmap-${pair%%:*}/ic_launcher_foreground.png"
done
