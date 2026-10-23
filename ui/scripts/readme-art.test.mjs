// The README's two pictures are generated, and GitHub shows an SVG only when it is a closed,
// self-contained document. These tests hold the generator to what that needs, to the rule that
// motion stops for a person who asked for less of it, and to the pixel figures being the app's.
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { buildSprite } from '../src/renderer/avatar/pixels.ts'
import { ART, CAST, GLYPHS, bot, wordmark } from './readme-art.mjs'

const pictures = Object.entries(ART).map(([name, draw]) => [name, draw()])
const css = (text) => text.match(/<style>([\s\S]*?)<\/style>/)[1]

/** Why an SVG's tags do not nest and close, or null when they do. */
function unclosed(text) {
  const open = []
  for (const [, closing, name, , selfClosing] of text.matchAll(/<(\/?)([A-Za-z][\w:-]*)((?:[^>"]|"[^"]*")*?)(\/?)>/g)) {
    if (selfClosing) continue
    if (!closing) open.push(name)
    else if (open.pop() !== name) return `</${name}> closes something other than its own tag`
  }
  return open.length > 0 ? `<${open.pop()}> is never closed` : null
}

test('every picture is a closed document, so a browser draws all of it', () => {
  for (const [name, text] of pictures) {
    assert.equal(unclosed(text), null, name)
    assert.match(text, /^<svg [^>]*viewBox="0 0 \d+ \d+"/, name)
    assert.match(text, /<\/svg>\n$/, name)
  }
})

test('a picture refers to nothing outside its own file, which GitHub would block or strip', () => {
  const [namespace] = pictures[0][1].match(/xmlns="[^"]+"/)
  for (const [name, text] of pictures) {
    assert.doesNotMatch(text.replace(namespace, ''), /<script|<foreignObject|<image|<use|@import|https?:|href=|url\((?!#)/i, name)
  }
})

test('every animation stops under prefers-reduced-motion and leaves the eyes open', () => {
  for (const [name, text] of pictures) {
    const sheet = css(text)
    const reduced = sheet.match(/@media \(prefers-reduced-motion:reduce\)\{([^{]*)\{animation:none\}\}/)
    assert.ok(reduced, `${name} has no reduced-motion rule`)
    const stopped = new Set(reduced[1].split(',').map((selector) => selector.trim()))
    const animated = [...sheet.replace(reduced[0], '').matchAll(/(\.[\w-]+)\{[^}]*animation:/g)].map((match) => match[1])
    assert.ok(animated.length > 0, name)
    for (const selector of animated) assert.ok(stopped.has(selector), `${name}: ${selector} keeps moving`)
    assert.match(sheet, /\.lid\{opacity:0;/, `${name}: the closed lid shows without an animation`)
  }
})

test('the same pictures come out on every run, so regenerating changes only what was edited', () => {
  for (const [name, draw] of Object.entries(ART)) assert.equal(draw(), draw(), name)
})

test('the pictures hold no em-dash, which the repository forbids everywhere', () => {
  const emDash = String.fromCodePoint(0x2014)
  for (const [name, text] of pictures) assert.ok(!text.includes(emDash), name)
})

test('every glyph of the wordmark is five cells by seven, and bravebot can be spelled', () => {
  for (const [letter, rows] of Object.entries(GLYPHS)) {
    assert.equal(rows.length, 7, letter)
    for (const row of rows) assert.match(row, /^[#.]{5}$/, `${letter}: ${row}`)
  }
  const { rects, width } = wordmark('bravebot', { x: 0, y: 0, cell: 10 })
  assert.equal(width, (8 * 6 - 1) * 10)
  assert.ok(rects.length > 0)
})

test('a bot is drawn with every cell and colour the desktop app draws for its seed', () => {
  for (const seed of CAST) {
    const sprite = buildSprite(seed)
    const markup = bot(seed, { centre: 100, baseline: 100 })
    // Each eye is a white cell, a pupil cell and a lid.
    assert.equal(markup.match(/<rect /g).length, sprite.cells.length + 6, seed)
    for (const cell of sprite.cells) assert.ok(markup.includes(`<rect x="${cell.x}" y="${cell.y}" width="1" height="1" fill="${cell.fill}"/>`), `${seed}: ${cell.x},${cell.y}`)
  }
})

test('the README shows both pictures, each with a description for a reader who cannot see them', () => {
  const readme = readFileSync(new URL('../../README.md', import.meta.url), 'utf8')
  for (const name of Object.keys(ART)) {
    const tag = readme.match(new RegExp(`<img [^>]*src="docs/assets/${name}"[^>]*>`))
    assert.ok(tag, `README.md does not show ${name}`)
    assert.match(tag[0], /alt="[^"]{20,}"/, name)
  }
})
