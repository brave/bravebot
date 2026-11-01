#!/bin/sh
# Builds the review-ready menu bar app and installs it for the current user.
#
#   contrib/review-ready-menubar/install.sh [--interval SECONDS] [--make-args "REVIEWER=login"]
#                                           [--login] [--dir DIRECTORY]
#   contrib/review-ready-menubar/install.sh --uninstall
#
# The app is built from ReviewReadyMenu.swift with swiftc (Xcode or the Command Line Tools) into
# ~/Applications/ReviewReadyMenu.app and runs `make review-ready` in the checkout this script is in.
# --login also installs a LaunchAgent so it starts at login. --dir changes where the app is put.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
dir="$HOME/Applications"
interval=300
make_args=""
login=0
uninstall=0
label="com.brave.bravebot.review-ready-menu"
agent="$HOME/Library/LaunchAgents/$label.plist"

while [ $# -gt 0 ]; do
    case "$1" in
        --interval) interval=$2; shift 2 ;;
        --make-args) make_args=$2; shift 2 ;;
        --dir) dir=$2; shift 2 ;;
        --login) login=1; shift ;;
        --uninstall) uninstall=1; shift ;;
        *) echo "install.sh: unknown argument: $1" >&2; exit 2 ;;
    esac
done

app="$dir/ReviewReadyMenu.app"

if [ "$uninstall" -eq 1 ]; then
    launchctl bootout "gui/$(id -u)/$label" 2>/dev/null || true
    pkill -f "$app/Contents/MacOS/ReviewReadyMenu" 2>/dev/null || true
    rm -f "$agent"
    rm -rf "$app"
    rm -rf "$HOME/Library/Caches/bravebot-review-ready"
    echo "Removed $app"
    exit 0
fi

if [ "$(uname -s)" != Darwin ]; then
    echo "install.sh: this is a macOS menu bar app" >&2
    exit 1
fi
if ! command -v swiftc >/dev/null 2>&1; then
    echo "install.sh: swiftc not found; run xcode-select --install" >&2
    exit 1
fi
if ! command -v gh >/dev/null 2>&1 && [ ! -x /opt/homebrew/bin/gh ] && [ ! -x /usr/local/bin/gh ]; then
    echo "install.sh: warning: gh is not installed; make review-ready needs it" >&2
fi

pkill -f "$app/Contents/MacOS/ReviewReadyMenu" 2>/dev/null || true
rm -rf "$app"
mkdir -p "$app/Contents/MacOS"

swiftc -O -o "$app/Contents/MacOS/ReviewReadyMenu" "$here/ReviewReadyMenu.swift"

plist="$app/Contents/Info.plist"
cat >"$plist" <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key><string>com.brave.bravebot.review-ready-menu</string>
    <key>CFBundleName</key><string>ReviewReadyMenu</string>
    <key>CFBundleExecutable</key><string>ReviewReadyMenu</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleVersion</key><string>1</string>
    <key>LSUIElement</key><true/>
</dict>
</plist>
EOF
plutil -insert ReviewReadyRepoPath -string "$repo" "$plist"
plutil -insert ReviewReadyIntervalSeconds -string "$interval" "$plist"
plutil -insert ReviewReadyMakeArguments -string "$make_args" "$plist"
codesign --force --sign - "$app" >/dev/null 2>&1 || true

if [ "$login" -eq 1 ]; then
    mkdir -p "$(dirname "$agent")"
    cat >"$agent" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key><string>$label</string>
    <key>ProgramArguments</key><array><string>/usr/bin/open</string><string>$app</string></array>
    <key>RunAtLoad</key><true/>
</dict>
</plist>
EOF
    launchctl bootout "gui/$(id -u)/$label" 2>/dev/null || true
    launchctl bootstrap "gui/$(id -u)" "$agent"
fi

open "$app"
echo "Installed $app (checkout: $repo, refresh every ${interval}s)"
