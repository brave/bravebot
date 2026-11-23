#!/bin/sh
# Builds the alien wallpaper app and installs it for the current user.
#
#   contrib/alien-wallpaper/install.sh [--fps N] [--login] [--dir DIRECTORY]
#   contrib/alien-wallpaper/install.sh --uninstall
#
# The app is built from AlienWallpaper.swift with swiftc (Xcode or the Command Line Tools) into
# ~/Applications/AlienWallpaper.app. --login also installs a LaunchAgent so it starts at login.
# --dir changes where the app is put. --fps sets the frame rate the app is started with.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
dir="$HOME/Applications"
fps=30
login=0
uninstall=0
label="com.brave.bravebot.alien-wallpaper"
agent="$HOME/Library/LaunchAgents/$label.plist"

while [ $# -gt 0 ]; do
    case "$1" in
        --fps) fps=$2; shift 2 ;;
        --dir) dir=$2; shift 2 ;;
        --login) login=1; shift ;;
        --uninstall) uninstall=1; shift ;;
        *) echo "install.sh: unknown argument: $1" >&2; exit 2 ;;
    esac
done

app="$dir/AlienWallpaper.app"

if [ "$uninstall" -eq 1 ]; then
    launchctl bootout "gui/$(id -u)/$label" 2>/dev/null || true
    pkill -f "$app/Contents/MacOS/AlienWallpaper" 2>/dev/null || true
    rm -f "$agent"
    rm -rf "$app"
    echo "Removed $app"
    exit 0
fi

if [ "$(uname -s)" != Darwin ]; then
    echo "install.sh: this is a macOS app" >&2
    exit 1
fi
if ! command -v swiftc >/dev/null 2>&1; then
    echo "install.sh: swiftc not found; run xcode-select --install" >&2
    exit 1
fi

pkill -f "$app/Contents/MacOS/AlienWallpaper" 2>/dev/null || true
rm -rf "$app"
mkdir -p "$app/Contents/MacOS"

swiftc -O -o "$app/Contents/MacOS/AlienWallpaper" "$here/AlienWallpaper.swift"

cat >"$app/Contents/Info.plist" <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key><string>com.brave.bravebot.alien-wallpaper</string>
    <key>CFBundleName</key><string>AlienWallpaper</string>
    <key>CFBundleExecutable</key><string>AlienWallpaper</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleVersion</key><string>1</string>
    <key>LSUIElement</key><true/>
</dict>
</plist>
EOF
codesign --force --sign - "$app" >/dev/null 2>&1 || true

if [ "$login" -eq 1 ]; then
    mkdir -p "$(dirname "$agent")"
    cat >"$agent" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key><string>$label</string>
    <key>ProgramArguments</key><array><string>$app/Contents/MacOS/AlienWallpaper</string><string>--fps</string><string>$fps</string></array>
    <key>RunAtLoad</key><true/>
</dict>
</plist>
EOF
    launchctl bootout "gui/$(id -u)/$label" 2>/dev/null || true
    launchctl bootstrap "gui/$(id -u)" "$agent"
else
    open "$app" --args --fps "$fps"
fi

echo "Installed $app (${fps} fps)"
