// The desktop app's icons, which no build step reads back.
//
// The packager skips a missing Icon Composer document without a message, and an `.ico` with a size
// left out is legal and shows a blurred one at that size. So the things that can be wrong are the
// ones nothing fails on: a file that is not where the platform looks for it, and a bitmap that is
// not the size its name says.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { existsSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { ICON_SIZES } from './linux-package.mjs'
import { pixels, pngSize } from './icon-test-support.mjs'

const UI_BUILD = new URL('../build/', import.meta.url).pathname

// The directory of an `.ico` lists each image's size, and the images it points at are PNGs that
// state their own. A file whose directory says 24 over a 32 pixel image draws at the wrong scale.
test('the Windows icon holds every size the taskbar and the file list ask for, and each image is that size', () => {
  const ico = readFileSync(join(UI_BUILD, 'icon.ico'))
  assert.equal(ico.readUInt16LE(2), 1, 'not an icon resource')
  const count = ico.readUInt16LE(4)
  const sizes = []
  for (let i = 0; i < count; i++) {
    const entry = 6 + i * 16
    const declared = ico[entry] || 256
    const length = ico.readUInt32LE(entry + 8)
    const offset = ico.readUInt32LE(entry + 12)
    assert.deepEqual(pngSize(ico.subarray(offset, offset + length)), { width: declared, height: declared })
    sizes.push(declared)
  }
  assert.deepEqual(sizes.sort((a, b) => a - b), [16, 24, 32, 48, 64, 128, 256])
})

test('the Linux icons include each size the Windows icon holds, and the one for a 200% desktop', () => {
  for (const size of [16, 24, 32, 48, 64, 128, 256]) assert.ok(ICON_SIZES.includes(size), `no ${size} pixel Linux icon`)
  assert.ok(ICON_SIZES.includes(512))
  for (const size of ICON_SIZES) {
    const bytes = readFileSync(join(UI_BUILD, 'icons', `${size}x${size}.png`))
    assert.deepEqual(pngSize(bytes), { width: size, height: size })
  }
})

// The packager compiles `icon.icon` only when it sits beside the `.icns` it was given, and a
// bundle without it shows the `.icns` on macOS 26 as well, which has no glass and no tile.
test('the Icon Composer document sits beside the .icns and every layer it names exists', () => {
  const document = join(UI_BUILD, 'icon.icon')
  assert.ok(existsSync(join(UI_BUILD, 'icon.icns')))
  const layers = JSON.parse(readFileSync(join(document, 'icon.json'), 'utf8')).groups.flatMap((group) => group.layers)
  assert.ok(layers.length > 0)
  for (const layer of layers) assert.ok(existsSync(join(document, 'Assets', layer['image-name'])), `${layer['image-name']} is a layer with no file`)
})

// `app.dock.setIcon` cannot load an `.icns`, so the development run reads `icon.png`, which has
// to be the picture the `.icns` holds at 1024 or the dock shows a different icon from the bundle.
test('the development dock icon is the 1024 pixel picture inside the .icns', () => {
  const icns = readFileSync(join(UI_BUILD, 'icon.icns'))
  let image
  for (let at = 8; at < icns.length; at += icns.readUInt32BE(at + 4)) {
    if (icns.subarray(at, at + 4).toString('latin1') === 'ic10') image = icns.subarray(at + 8, at + icns.readUInt32BE(at + 4))
  }
  assert.ok(image, 'the .icns has no 1024 pixel image')
  const png = readFileSync(join(UI_BUILD, 'icon.png'))
  assert.deepEqual(pngSize(png), { width: 1024, height: 1024 })
  assert.ok(pixels(png).equals(pixels(image)), 'icon.png is not the 1024 pixel image in icon.icns')
})
