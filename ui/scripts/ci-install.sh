#!/usr/bin/env bash
# `pnpm install --frozen-lockfile` for CI. Leo's prepare step builds it from source and is most
# of an install, so when the directory named by $1 holds a built @brave/leo, the install skips
# every script, copies that Leo in, and then runs this project's own postinstall, which
# fetches the Electron runtime and patches Leo's types. A dependency's build script is never
# run again here, which is what the skip is for: pnpm gates every dependency's scripts behind
# the allowBuilds list in pnpm-workspace.yaml, and --ignore-scripts turns the whole lot off,
# Leo's five-minute prepare included.
set -euo pipefail

built=$1
cd "$(dirname "$0")/.."

if [ -f "$built/package.json" ]; then
  pnpm install --frozen-lockfile --ignore-scripts
  rm -rf node_modules/@brave/leo
  cp -R "$built" node_modules/@brave/leo
  pnpm run postinstall
else
  pnpm install --frozen-lockfile
  mkdir -p "$built"
  cp -R node_modules/@brave/leo/. "$built"
fi
