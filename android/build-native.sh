#!/usr/bin/env bash
# Build what the app loads but Gradle does not build: the agent library from crates/android, and
# the renderer from ui/. Both land in gitignored folders under app/src/main.
#
#   ./build-native.sh            debug library, both ABIs
#   ./build-native.sh --release  release library
#
# Needs the NDK (ANDROID_NDK_HOME, or the newest under the SDK), cargo-ndk, and the Rust targets
# aarch64-linux-android and x86_64-linux-android.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(dirname "$here")"
sdk="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
if [[ -z "${ANDROID_NDK_HOME:-}" ]]; then
  ANDROID_NDK_HOME="$(ls -d "$sdk"/ndk/* 2>/dev/null | sort -V | tail -1)"
  export ANDROID_NDK_HOME
fi
[[ -d "$ANDROID_NDK_HOME" ]] || { echo "no NDK found; set ANDROID_NDK_HOME" >&2; exit 1; }

profile=()
[[ "${1:-}" == "--release" ]] && profile=(--release)

(cd "$root" && cargo ndk -t arm64-v8a -t x86_64 -o "$here/app/src/main/jniLibs" \
  build -p bravebot-android ${profile[@]+"${profile[@]}"})
(cd "$root/ui" && npm run build:android)
