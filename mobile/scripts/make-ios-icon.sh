#!/bin/bash
# Copies the desktop app's Icon Composer document, ui/build/icon.icon, into the iOS project as its
# app icon. Xcode 26 compiles it into Assets.car for iOS 26 and later, and writes flat fallback
# images for earlier versions. The project's ASSETCATALOG_COMPILER_APPICON_NAME is AppIcon, which
# is why the copy is named AppIcon.icon.
set -euo pipefail

mobile="$(cd "$(dirname "$0")/.." && pwd)"
target="$mobile/ios/Bravebot/AppIcon.icon"
rm -rf "$target"
cp -R "$mobile/../ui/build/icon.icon" "$target"
echo "icon written to $target"
