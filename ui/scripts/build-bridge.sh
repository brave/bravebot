#!/usr/bin/env bash
# Build bravebot-rpc, with the agent's credentials baked in where they can be found.
#
# bravebot captures its backend credentials at COMPILE time (see
# crates/config/build.rs upstream). That is deliberate: a release binary is built where
# the secrets are and used anywhere, so it does not demand them again from every
# directory it starts in.
#
# It matters more for a GUI than for the CLI. `bravebot` is run from a terminal, and that
# terminal usually has direnv loaded, so an unconfigured binary still finds what it needs
# in the environment. An app launched from Finder or `npm run dev` has no such
# environment, so an unconfigured build fails at the first inference request with
# "SERVICES_KEY_AICHAT is not set and was not built in".
#
# So: build through direnv when the agent checkout has an allowed .envrc, and say plainly
# what will happen when it does not.

set -uo pipefail

# The default has to agree with the path dependencies in
# crates/bravebot-bridge/Cargo.toml, which are what actually decide whose sources get
# compiled. Reading credentials from one checkout while cargo builds another produces a
# binary nobody asked for, so the default is the submodule those paths point at rather
# than a second spelling of the same location.
#
# BRAVEBOT_DIR (and BUA_AGENT_DIR, which may be set in a shell profile from before the
# agent was renamed) still win, for the case where the .envrc lives in an older sibling
# checkout. That is credentials only: cargo compiles the submodule either way.
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
AGENT="${BRAVEBOT_DIR:-${BUA_AGENT_DIR:-$REPO/vendor/bravebot}}"

# An un-initialised submodule is an empty directory, and cargo's error for it names a
# missing Cargo.toml rather than the one command that fixes it.
if [ ! -f "$REPO/vendor/bravebot/Cargo.toml" ]; then
  echo "error: vendor/bravebot is empty. Run: git submodule update --init" >&2
  exit 1
fi

# A `git pull` that moves the pin leaves the submodule where it was, and the build that
# follows compiles an agent nobody chose. `git submodule status` marks that with a leading
# `+`, which is the only warning anyone gets otherwise.
if git -C "$REPO" submodule status vendor/bravebot 2>/dev/null | grep -q '^+'; then
  echo "warning: vendor/bravebot is not at the pinned revision." >&2
  echo "         Run \`git submodule update\` to build what this branch pins." >&2
fi

# direnv's allow list is keyed on the physical path of the .envrc, so a checkout reached
# through a symlink reads as un-allowed however many times you run `direnv allow` on it.
if [ -d "$AGENT" ]; then
  AGENT="$(cd "$AGENT" && pwd -P)"
fi

build() { cargo build -p bravebot-bridge -p bravebot-ui-files "$@"; }

if [ ! -d "$AGENT" ]; then
  echo "warning: no credential checkout at $AGENT (set BRAVEBOT_DIR)." >&2
  echo "         Building without credentials; inference will fail at run time." >&2
  build "$@"
  exit $?
fi

if [ -n "${SERVICES_KEY_AICHAT:-}" ]; then
  # Already in a configured shell. Nothing to add.
  build "$@"
  exit $?
fi

if command -v direnv >/dev/null 2>&1 && [ -f "$AGENT/.envrc" ]; then
  if direnv exec "$AGENT" true 2>/dev/null; then
    echo "building with credentials from $AGENT/.envrc" >&2
    direnv exec "$AGENT" cargo build -p bravebot-bridge -p bravebot-ui-files "$@"
    exit $?
  fi
  echo "warning: $AGENT/.envrc is not allowed. Run: direnv allow $AGENT" >&2
fi

cat >&2 <<'MSG'
warning: building bravebot-rpc WITHOUT backend credentials.

  The app will start, list sessions, and open them. The first inference request will
  fail with "SERVICES_KEY_AICHAT is not set and was not built in".

  To fix: copy .envrc.example to .envrc in vendor/bravebot, run `direnv allow` there,
  then build again. (.envrc is ignored by the agent's own .gitignore, so it does not
  show up as a dirty submodule.) If you already have one in an older sibling checkout,
  BRAVEBOT_DIR pointed at that checkout is read for credentials instead.
MSG
build "$@"
