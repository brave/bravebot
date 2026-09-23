#!/usr/bin/env bash
# Build bravebot-rpc, with the agent's credentials baked in where they can be found.
#
# bravebot captures its backend credentials at COMPILE time (see crates/config/build.rs).
# That is deliberate: a release binary is built where the secrets are and used anywhere, so
# it does not demand them again from every directory it starts in.
#
# It matters more for a GUI than for the CLI. `bravebot` is run from a terminal, and that
# terminal usually has direnv loaded, so an unconfigured binary still finds what it needs
# in the environment. An app launched from Finder or `npm run dev` has no such
# environment, so an unconfigured build fails at the first inference request with
# "SERVICES_KEY_AICHAT is not set and was not built in".
#
# So: build through direnv when the workspace has an allowed .envrc, and say plainly what
# will happen when it does not.

set -uo pipefail

# The workspace root, two levels up: this script lives in ui/scripts/. The bridge crates
# are members of that workspace and the .envrc sits at its root, so the sources being
# compiled and the credentials being read now come from one location rather than from a
# pair that had to be kept agreeing. BRAVEBOT_DIR still wins, for a .envrc kept in a
# sibling checkout; that is credentials only, since cargo compiles this workspace either
# way.
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
AGENT="${BRAVEBOT_DIR:-$REPO}"

# direnv's allow list is keyed on the physical path of the .envrc, so a checkout reached
# through a symlink reads as un-allowed however many times you run `direnv allow` on it.
if [ -d "$AGENT" ]; then
  AGENT="$(cd "$AGENT" && pwd -P)"
fi

# The unconfigured build is opted into here, for these two packages, rather than in a
# `.cargo/config.toml`. Cargo reads that file from the invocation directory upwards, and
# since the front end became a subdirectory of the workspace one under `ui/` applied to
# every member: `cd ui && cargo build -p bravebot-cli` produced the shipping binary with no
# credentials baked in and nothing said so, which is the failure the config build script
# exists to refuse. A packaged release must NOT rely on this; it is built the way the
# agent's own releases are, with the credentials present.
build() {
  BRAVEBOT_ALLOW_UNCONFIGURED_BUILD="${BRAVEBOT_ALLOW_UNCONFIGURED_BUILD:-1}" \
    cargo build -p bravebot-ui-bridge -p bravebot-ui-files "$@"
}

if [ "${BRAVEBOT_BUILD_UNCONFIGURED:-0}" = 1 ]; then
  # Check builds must not inherit a developer's account or load it through direnv.
  unset SERVICES_KEY_AICHAT BRAVE_SERVICES_KEY_ID BRAVE_AI_CHAT_ENDPOINT \
    BRAVE_AI_CHAT_PREMIUM_ENDPOINT BRAVE_AI_CHAT_DEFAULT_MODEL
  export BRAVEBOT_ALLOW_UNCONFIGURED_BUILD=1
  build "$@"
  exit $?
fi

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
    direnv exec "$AGENT" cargo build -p bravebot-ui-bridge -p bravebot-ui-files "$@"
    exit $?
  fi
  echo "warning: $AGENT/.envrc is not allowed. Run: direnv allow $AGENT" >&2
fi

cat >&2 <<'MSG'
warning: building bravebot-rpc WITHOUT backend credentials.

  The app will start, list sessions, and open them. The first inference request will
  fail with "SERVICES_KEY_AICHAT is not set and was not built in".

  To fix: copy .envrc.example to .envrc at the root of this repository, run
  `direnv allow` there, then build again. If you keep one in a sibling checkout,
  BRAVEBOT_DIR pointed at that checkout is read for credentials instead.
MSG
build "$@"
