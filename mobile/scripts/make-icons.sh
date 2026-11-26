#!/bin/bash
# Remakes the Android app's icons from the desktop app's drawing, ui/build/icon.svg. The iOS app
# takes the Icon Composer document instead: make-ios-icon.sh.
# Needs python3 with Pillow. Run it from anywhere; it rewrites the icon files under android/.
set -euo pipefail

mobile="$(cd "$(dirname "$0")/.." && pwd)"
drawing="$mobile/../ui/build/icon.svg"
res="$mobile/android/app/src/main/res"

python3 - "$drawing" "$res" <<'PY'
import re
import sys
from pathlib import Path

from PIL import Image, ImageColor

source, res = sys.argv[1], Path(sys.argv[2])

# The drawing is 8 by 9 squares of 64 units, one colour each. The grid keeps those colours, and
# every size is drawn with a whole number of pixels per square, so no edge is blurred.
squares = re.findall(r'd="M(\d+) (\d+)H(\d+)V\d+H\d+V\d+Z" fill="([^"]+)"', open(source, encoding="utf-8").read())
assert len(squares) == 63, f"the drawing no longer has the 63 squares this script reads, it has {len(squares)}"
grid = Image.new("RGBA", (8, 9))
for _, top, left, fill in squares:
    grid.putpixel(((int(left) - 32) // 64, int(top) // 64), ImageColor.getrgb(fill))
# The eyes are the four squares in the sixth row that are white or near black.
EYES = [(x, 6) for x in range(8) if grid.getpixel((x, 6))[:3] in ((255, 255, 255), (29, 31, 39))]
assert len(EYES) == 4, f"expected four eye squares, found {EYES}"
WHITES = [e for e in EYES if grid.getpixel(e)[:3] == (255, 255, 255)]


def mascot(cell, silhouette=False):
    art = grid.resize((8 * cell, 9 * cell), Image.NEAREST)
    if silhouette:
        # One colour for the system to tint, with the white squares of the eyes cut out.
        solid = Image.new("RGBA", art.size, (0, 0, 0, 255))
        solid.putalpha(art.getchannel("A"))
        art = solid
        hole = Image.new("RGBA", (cell, cell), (0, 0, 0, 0))
        for x, y in WHITES:
            art.paste(hole, (x * cell, y * cell))
    return art


def centred(size, art):
    canvas = Image.new("RGBA", (size, size))
    canvas.paste(art, ((size - art.width) // 2, (size - art.height) // 2))
    return canvas


# Android 8 and later. The system draws the mask over a 108 dp canvas, and only the middle 66 dp
# circle is always visible. The figure's diagonal is 12.04 squares, so a square is the largest
# whole number of pixels for which that diagonal fits in 66 dp at the density.
for name, scale in (("mdpi", 1), ("hdpi", 1.5), ("xhdpi", 2), ("xxhdpi", 3), ("xxxhdpi", 4)):
    folder = res / f"mipmap-{name}"
    folder.mkdir(parents=True, exist_ok=True)
    layer = round(108 * scale)
    cell = int(66 * scale / (8 ** 2 + 9 ** 2) ** 0.5)
    centred(layer, mascot(cell)).save(folder / "ic_launcher_foreground.png")
    centred(layer, mascot(cell, silhouette=True)).save(folder / "ic_launcher_monochrome.png")
PY

mkdir -p "$res/mipmap-anydpi-v26" "$res/drawable"
for name in ic_launcher ic_launcher_round; do
  cat > "$res/mipmap-anydpi-v26/$name.xml" <<'XML'
<?xml version="1.0" encoding="utf-8"?>
<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">
    <background android:drawable="@drawable/ic_launcher_background" />
    <foreground android:drawable="@mipmap/ic_launcher_foreground" />
    <monochrome android:drawable="@mipmap/ic_launcher_monochrome" />
</adaptive-icon>
XML
done
cat > "$res/drawable/ic_launcher_background.xml" <<'XML'
<?xml version="1.0" encoding="utf-8"?>
<shape xmlns:android="http://schemas.android.com/apk/res/android" android:shape="rectangle">
    <solid android:color="#FFFFFF" />
</shape>
XML
echo "icons written under $res"
