#!/usr/bin/env bash
# `npm ci` for CI. Leo's prepare step builds it from source and is most of an install, so when
# the directory named by $1 holds a built @brave/leo, the install skips every script, copies that
# Leo in, and then runs the other packages' scripts and our postinstall with `npm rebuild`, which
# does not run a dependency's prepare. Otherwise it installs normally and copies Leo out to $1.
set -euo pipefail

built=$1
cd "$(dirname "$0")/.."

if [ -f "$built/package.json" ]; then
  npm ci --ignore-scripts
  rm -rf node_modules/@brave/leo
  cp -R "$built" node_modules/@brave/leo
  npm rebuild
else
  npm ci
  mkdir -p "$built"
  cp -R node_modules/@brave/leo/. "$built"
fi
