// The Android app's icons, which Gradle packages and nothing here builds.
//
// A layer at the wrong density is scaled by the system, which blurs the squares the icon is drawn
// with, and an adaptive icon without its themed layer shows the full-colour one with no report.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, statSync } from 'node:fs'
import { join } from 'node:path'
import { pngSize } from './icon-test-support.mjs'

const RES = new URL('../../mobile/android/app/src/main/res/', import.meta.url).pathname

const DENSITIES = { mdpi: 1, hdpi: 1.5, xhdpi: 2, xxhdpi: 3, xxxhdpi: 4 }

// The adaptive layers are 108 dp and the legacy icons 48 dp. A layer at the wrong density is
// scaled by the system, which blurs the squares the icon is drawn with.
test('each Android density has an adaptive foreground, a themed layer and legacy icons at its own size', () => {
  for (const [density, scale] of Object.entries(DENSITIES)) {
    const folder = join(RES, `mipmap-${density}`)
    for (const [name, dp] of [['ic_launcher_foreground', 108], ['ic_launcher_monochrome', 108], ['ic_launcher', 48], ['ic_launcher_round', 48]]) {
      const px = Math.round(dp * scale)
      assert.deepEqual(pngSize(readFileSync(join(folder, `${name}.png`))), { width: px, height: px }, `${density}/${name}`)
    }
  }
})

// Without the element a themed icon shows the full-colour one, and nothing reports it.
test('both adaptive icons name the themed layer, and the background is white', () => {
  for (const name of ['ic_launcher', 'ic_launcher_round']) {
    const xml = readFileSync(join(RES, 'mipmap-anydpi-v26', `${name}.xml`), 'utf8')
    assert.match(xml, /<monochrome android:drawable="@mipmap\/ic_launcher_monochrome" \/>/)
    assert.match(xml, /<foreground android:drawable="@mipmap\/ic_launcher_foreground" \/>/)
  }
  assert.match(readFileSync(join(RES, 'drawable/ic_launcher_background.xml'), 'utf8'), /<solid android:color="#FFFFFF" \/>/)
  assert.ok(statSync(join(RES, 'mipmap-xxxhdpi/ic_launcher_monochrome.png')).size > 0)
})
