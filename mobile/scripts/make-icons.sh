#!/bin/bash
# Remakes both apps' icons from the desktop app's drawing, ui/build/icon.svg.
# Needs macOS: it renders with AppKit through swiftc, and flattens with sips.
# Run it from anywhere; it rewrites the icon files under android/ and ios/.
set -euo pipefail

mobile="$(cd "$(dirname "$0")/.." && pwd)"
drawing="$mobile/../ui/build/icon.svg"
res="$mobile/android/app/src/main/res"
catalog="$mobile/ios/Bravebot/Images.xcassets/AppIcon.appiconset"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

cat > "$work/render.swift" <<'SWIFT'
import AppKit
let args = CommandLine.arguments
guard args.count == 4, let image = NSImage(contentsOfFile: args[1]) else { print("cannot load \(CommandLine.arguments)"); exit(1) }
let size = Int(args[3])!
guard let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: size, pixelsHigh: size, bitsPerSample: 8, samplesPerPixel: 4,
                                 hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0) else { exit(2) }
rep.size = NSSize(width: size, height: size)
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
NSColor.clear.set()
NSRect(x: 0, y: 0, width: size, height: size).fill(using: .copy)
image.draw(in: NSRect(x: 0, y: 0, width: size, height: size), from: .zero, operation: .sourceOver, fraction: 1)
NSGraphicsContext.restoreGraphicsState()
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: args[2]))
SWIFT
swiftc -O "$work/render.swift" -o "$work/render"

# The drawing is a rounded tile with a margin, as a macOS icon is. Each platform wants it cut differently.
python3 - "$drawing" "$work" <<'PY'
import sys

source = open(sys.argv[1], encoding="utf-8").read()
out = sys.argv[2]
margin = 'viewBox="0 0 1024 1024"'
tile_rect = '<rect x="100" y="100" width="824" height="824" rx="185" fill="url(#tile)"/>'
clip_rect = '<rect x="100" y="100" width="824" height="824" rx="185"/>'
for needed in (margin, tile_rect, clip_rect):
    assert source.count(needed) == 1, f"the drawing no longer contains {needed}"
tile_only = source.replace(margin, 'viewBox="100 100 824 824"')


def write(name, text):
    open(f"{out}/{name}.svg", "w", encoding="utf-8").write(text)


# Android before 8: the rounded tile, margin cut off.
write("rounded", tile_only)
# Android before 8, round: a circle instead of the rounded tile.
write("round", tile_only.replace(tile_rect, '<circle cx="512" cy="512" r="412" fill="url(#tile)"/>')
      .replace(clip_rect, '<circle cx="512" cy="512" r="412"/>'))
# iOS applies its own mask, so it wants the square with no rounded corners.
square = tile_only.replace(" rx=\"185\"", "")
write("square", square)
# Android 8 and later: the system draws the mask. The visible part is the middle 72 of 108, so the
# square sits at two thirds of the canvas, over a background of the tile's own gradient.
scale = 72 / 108 * 1024 / 824
offset = (1024 - 824 * scale) / 2
inner = square.split("</defs>", 1)[1].rsplit("</svg>", 1)[0]
head = square.split("</defs>", 1)[0] + "</defs>"
head = head.replace('viewBox="100 100 824 824"', 'viewBox="0 0 1024 1024"')
write("foreground", f'{head}<g transform="translate({offset:.3f} {offset:.3f}) scale({scale:.5f}) translate(-100 -100)">{inner}</g></svg>')
PY

render() { "$work/render" "$work/$1.svg" "$2" "$3"; }

for density in mdpi:48:108 hdpi:72:162 xhdpi:96:216 xxhdpi:144:324 xxxhdpi:192:432; do
  IFS=: read -r name legacy adaptive <<< "$density"
  folder="$res/mipmap-$name"
  mkdir -p "$folder"
  render rounded "$folder/ic_launcher.png" "$legacy"
  render round "$folder/ic_launcher_round.png" "$legacy"
  render foreground "$folder/ic_launcher_foreground.png" "$adaptive"
done

mkdir -p "$res/mipmap-anydpi-v26" "$res/drawable"
for name in ic_launcher ic_launcher_round; do
  cat > "$res/mipmap-anydpi-v26/$name.xml" <<'XML'
<?xml version="1.0" encoding="utf-8"?>
<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">
    <background android:drawable="@drawable/ic_launcher_background" />
    <foreground android:drawable="@mipmap/ic_launcher_foreground" />
</adaptive-icon>
XML
done
cat > "$res/drawable/ic_launcher_background.xml" <<'XML'
<?xml version="1.0" encoding="utf-8"?>
<shape xmlns:android="http://schemas.android.com/apk/res/android" android:shape="rectangle">
    <gradient android:type="linear" android:angle="270" android:startColor="#FBFAF5" android:endColor="#E4E1D3" />
</shape>
XML

# The App Store refuses an icon with an alpha channel, and a round trip through JPEG drops it.
render square "$work/icon-1024.png" 1024
sips -s format jpeg "$work/icon-1024.png" --out "$work/icon-1024.jpg" > /dev/null
mkdir -p "$catalog"
sips -s format png "$work/icon-1024.jpg" --out "$catalog/icon-1024.png" > /dev/null
cat > "$catalog/Contents.json" <<'JSON'
{
  "images" : [
    {
      "filename" : "icon-1024.png",
      "idiom" : "universal",
      "platform" : "ios",
      "size" : "1024x1024"
    }
  ],
  "info" : {
    "author" : "xcode",
    "version" : 1
  }
}
JSON
echo "icons written under $res and $catalog"
