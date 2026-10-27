import test from 'node:test'
import { createHash } from 'node:crypto'
import assert from 'node:assert/strict'
import {
  buildSprite, spriteOf, placeEyes, hueDistance, contrast, choiceOf, pickedSeed, swatchOf, SWATCH,
  eyesToward, GRID, EYE_WHITE, EYE_CONTRAST, HUE_DISTANCE, PAINT, COLOURS, BODY_NAMES,
} from '../src/renderer/avatar/pixels.ts'
import { AVATAR_VERSION } from '../src/shared/avatar.ts'

const seeds = (count, prefix = 'v2:bot-') => Array.from({ length: count }, (_, i) => `${prefix}${i}`)
const hexOf = Object.fromEntries(PAINT.map(paint => [paint.name, paint.hex]))
const at = (sprite) => new Map(sprite.cells.map(cell => [`${cell.x},${cell.y}`, cell]))

/** Every cell an eye can occupy in any pose: its own two, the row over (looking up) and under (working). */
function eyeReach(sprite) {
  return sprite.eyes.flatMap(eye => [-1, 0, 1].flatMap(dy => [0, 1].map(dx => `${eye.x + dx},${eye.y + dy}`)))
}

function connected(sprite) {
  const cells = at(sprite)
  const [first] = cells.keys()
  const seen = new Set([first])
  const queue = [first]
  while (queue.length) {
    const [x, y] = queue.pop().split(',').map(Number)
    for (let dx = -1; dx <= 1; dx++) for (let dy = -1; dy <= 1; dy++) {
      const key = `${x + dx},${y + dy}`
      if (cells.has(key) && !seen.has(key)) { seen.add(key); queue.push(key) }
    }
  }
  return seen.size === cells.size
}

// Bots on screen and in tests have stored seeds from before this generator; they must still draw.
const LEGACY = ['review-3', 'review-61', 'existing-bot', 'a492-fc02', 'brave-bot-mascot']

test('a seed always builds the same face, so a bot looks the same in every window', () => {
  for (const seed of [...seeds(64), ...LEGACY]) {
    const sprite = buildSprite(seed)
    assert.deepEqual(buildSprite(seed), sprite)
    assert.equal(spriteOf(seed).signature, sprite.signature)
  }
})

test('every body, crown, arm, leg, gradient and symmetry is reachable from a seed', () => {
  const keys = ['body', 'crown', 'arms', 'legs', 'eyes', 'direction', 'fringe', 'symmetry']
  const sets = Object.fromEntries(keys.map(key => [key, new Set()]))
  for (const seed of seeds(512)) {
    const { traits } = buildSprite(seed)
    for (const key of keys) sets[key].add(traits[key])
  }
  assert.deepEqual(keys.map(key => sets[key].size), [8, 5, 4, 6, 2, 4, 2, 2])
})

test('every face is one connected figure with two eyes that stay on it in every pose', () => {
  for (const seed of [...seeds(512), ...LEGACY]) {
    const sprite = buildSprite(seed)
    const cells = at(sprite)
    assert.equal(sprite.eyes.length, 2)
    for (const key of eyeReach(sprite)) assert.ok(cells.has(key), `${seed}: eye cell ${key} is off the figure`)
    for (const { x, y } of sprite.cells) assert.ok(x >= 1 && x <= GRID - 2 && y >= 1 && y <= GRID - 2, `${seed}: ${x},${y} is in the margin`)
    assert.ok(connected(sprite), `${seed}: the figure is in pieces`)
  }
})

/** The avatar editor saves what a person picked as a seed, so the saved face has to be the one they picked. */
test('a face chosen in the editor has the shape and colour it names, with eyes that stay on it', () => {
  assert.equal(BODY_NAMES.length, 8)
  assert.equal(COLOURS.length, 8)
  for (const body of BODY_NAMES)
    for (const { name, paints } of COLOURS) {
      const seed = pickedSeed(body, name)
      const sprite = buildSprite(seed)
      assert.equal(sprite.traits.body, body, seed)
      assert.deepEqual(sprite.traits.paints, paints, seed)
      assert.deepEqual(choiceOf(seed), { body, colour: name })
      const cells = at(sprite)
      for (const key of eyeReach(sprite)) assert.ok(cells.has(key), `${seed}: eye cell ${key} is off the figure`)
      assert.ok(connected(sprite), `${seed}: the figure is in pieces`)
    }
})

/** A generated face has no colour of the editor's, so the editor starts on the nearest one. */
test('the editor starts a generated face on the colour whose top paint is nearest its first', () => {
  for (const seed of seeds(64, AVATAR_VERSION)) {
    const { traits } = buildSprite(seed)
    const { body, colour } = choiceOf(seed)
    assert.equal(body, traits.body, seed)
    const away = (each) => hueDistance(hexOf[each.paints[0]], hexOf[traits.paints[0]])
    const nearest = Math.min(...COLOURS.map(away))
    assert.equal(away(COLOURS.find((each) => each.name === colour)), nearest, `${seed}: ${traits.paints[0]} starts on ${colour}`)
  }
})

/**
 * The editor's colours have to look different from each other. Two colours whose top paints and
 * bottom paints are both close in hue read as one, and so do two that swap the same pair.
 */
test('every two colours in the editor differ by at least 60 degrees of hue between their ends', () => {
  const hue = (a, b) => hueDistance(hexOf[a], hexOf[b])
  for (let i = 0; i < COLOURS.length; i++)
    for (let j = i + 1; j < COLOURS.length; j++) {
      const [[a, b], [c, d]] = [COLOURS[i].paints, COLOURS[j].paints]
      const apart = Math.min(hue(a, c) + hue(b, d), hue(a, d) + hue(b, c))
      assert.ok(apart >= 60, `${COLOURS[i].name} and ${COLOURS[j].name} are ${apart.toFixed(0)} degrees apart`)
    }
})

/** The editor recolours its eight shapes as the colour changes, so a shape must not change outline with it. */
test('a chosen shape keeps its outline in every colour, and the eight outlines differ', () => {
  const outlines = new Set()
  for (const body of BODY_NAMES) {
    const outline = (colour) => buildSprite(pickedSeed(body, colour)).cells.map(cell => `${cell.x},${cell.y}`).join(' ')
    const first = outline(COLOURS[0].name)
    for (const { name } of COLOURS) assert.equal(outline(name), first, `${body} in ${name}`)
    outlines.add(first)
  }
  assert.equal(outlines.size, 8)
})

/**
 * The editor's colour swatches show the paints a face in that colour is mixed from, and look the
 * same for every bot, so cycling through them changes only the colour. A swatch that took its
 * direction or flip from a face would put the bottom paint on top for some bots.
 */
test('a colour swatch runs top to bottom from its first paint to its second', () => {
  for (const { name, paints: [top, bottom] } of COLOURS) {
    const swatch = swatchOf(name)
    assert.equal(swatch.length, SWATCH * SWATCH, name)
    const ys = swatch.map(cell => cell.y)
    const row = (y) => swatch.filter(cell => cell.y === y).map(cell => cell.fill)
    for (const fill of row(Math.min(...ys)))
      assert.ok(hueDistance(fill, hexOf[top]) < hueDistance(fill, hexOf[bottom]), `${name}: ${fill} at the top`)
    for (const fill of row(Math.max(...ys)))
      assert.ok(hueDistance(fill, hexOf[bottom]) < hueDistance(fill, hexOf[top]), `${name}: ${fill} at the bottom`)
  }
})

test('a seed naming no known shape or colour still draws a face', () => {
  for (const seed of ['v4:nope/red-yellow', 'v4:dome/nope', 'v4:dome/teal', 'v4:', 'v4:dome']) {
    const sprite = buildSprite(seed)
    assert.ok(sprite.cells.length > 0, seed)
    assert.ok(connected(sprite), seed)
  }
})

test('faces differ between bots in colour and, mostly, in shape', () => {
  const full = new Set()
  const shapes = new Set()
  for (const seed of seeds(1000)) {
    const sprite = buildSprite(seed)
    full.add(JSON.stringify(sprite.cells))
    shapes.add(sprite.cells.map(cell => `${cell.x},${cell.y}`).join(' '))
  }
  assert.ok(full.size >= 995, `${full.size} distinct faces in 1000`)
  assert.ok(shapes.size >= 400, `${shapes.size} distinct silhouettes in 1000`)
})

test('a stored seed keeps the colours it was painted with', () => {
  const earlier = new Set(['red', 'orange', 'yellow', 'lime', 'green', 'teal', 'cyan', 'blue', 'violet', 'magenta', 'pink'])
  assert.deepEqual(buildSprite('v2:example-0').traits.paints, ['teal', 'yellow', 'pink'])
  assert.deepEqual(buildSprite('review-3').traits.paints, ['yellow', 'magenta'])
  assert.deepEqual(buildSprite('readme-4').traits.paints, ['teal', 'violet'])
  for (const seed of [...seeds(64), ...LEGACY]) {
    for (const name of buildSprite(seed).traits.paints)
      assert.ok(earlier.has(name), `${seed} painted ${name}`)
  }
})

/**
 * A seed from before `v3:` walks the eleven-paint ring, so the five paints added since do not turn
 * up in its gradient. The hashes are of every cell's fill as `main` drew these seeds before those
 * paints existed, so a gradient that passes through a new paint changes one of them.
 */
test('a seed from before v3 is drawn cell for cell as it was', () => {
  const drawn = [
  ['v2:example-0', '8c5d8c9475af'],
  ['v2:example-7', '2f969f3a55cb'],
  ['v2:example-13', '7d0228a66cda'],
  ['v2:example-21', '8832645a1062'],
  ['review-3', '6a4d4cd25ecf'],
  ['existing-bot', '58a28d99d91a'],
  ['a492-fc02', '4550a798980b'],
  ['v2:bot-5', 'f09b13a08e48'],
  ['v2:bot-17', 'de94511e03c9'],
  ['v2:bot-33', '09c583cf5b51'],
  ['v2:bot-40', '26b52c746630'],
  ['v2:bot-52', 'e6b1dd679487'],
  ]
  for (const [seed, hash] of drawn) {
    const fills = buildSprite(seed).cells.map(cell => `${cell.x},${cell.y},${cell.fill}`).join(';')
    assert.equal(createHash('sha1').update(fills).digest('hex').slice(0, 12), hash, seed)
  }
})

test('a new seed can wear any of the sixteen paints', () => {
  assert.equal(PAINT.length, 16)
  const seen = new Set()
  for (const seed of seeds(512, `${AVATAR_VERSION}bot-`)) {
    const { paints } = buildSprite(seed).traits
    assert.ok(paints.length >= 2 && paints.length <= 3, seed)
    for (const name of paints) seen.add(name)
  }
  assert.deepEqual([...seen].sort(), PAINT.map((paint) => paint.name).sort())
})

test('a face chooses paints at least HUE_DISTANCE apart, and its eye whites stand out from the fill around them', () => {
  for (const seed of [...seeds(512), ...seeds(512, `${AVATAR_VERSION}bot-`)]) {
    const sprite = buildSprite(seed)
    const { paints } = sprite.traits
    assert.ok(paints.length >= 2 && paints.length <= 3)
    for (let i = 0; i < paints.length; i++)
      for (let j = i + 1; j < paints.length; j++)
        assert.ok(hueDistance(hexOf[paints[i]], hexOf[paints[j]]) >= HUE_DISTANCE, `${seed}: ${paints[i]} beside ${paints[j]}`)
    const eyes = new Set(sprite.eyes.flatMap(eye => [`${eye.x},${eye.y}`, `${eye.x + 1},${eye.y}`]))
    for (const cell of sprite.cells) {
      const near = sprite.eyes.some(eye => cell.x >= eye.x - 1 && cell.x <= eye.x + 2 && cell.y >= eye.y - 2 && cell.y <= eye.y + 2)
      if (near && !eyes.has(`${cell.x},${cell.y}`))
        assert.ok(contrast(EYE_WHITE, cell.fill) >= EYE_CONTRAST, `${seed}: ${cell.fill} at ${cell.x},${cell.y} beside an eye`)
    }
  }
})

test('a look moves the pupils, or closes the eyes', () => {
  const sprite = buildSprite('v2:look')
  const rest = { gaze: 1, drop: 0, blink: false, nod: false }
  const [left] = sprite.eyes
  const right = placeEyes(sprite, rest)
  assert.deepEqual([right[0].whiteX, right[0].pupilX], [left.x, left.x + 1])
  const away = placeEyes(sprite, { ...rest, gaze: -1 })
  assert.deepEqual([away[0].whiteX, away[0].pupilX], [left.x + 1, left.x])
  assert.deepEqual(placeEyes(sprite, { ...rest, blink: true }).map(eye => eye.closed), [true, true])
})

test('over a figure the eyes turn to the side the pointer is on, and up or down only when it is mostly there', () => {
  assert.deepEqual(eyesToward(15, 3, 40), { side: 1, drop: 0 })
  assert.deepEqual(eyesToward(-15, -3, 40), { side: -1, drop: 0 })
  assert.deepEqual(eyesToward(2, 3, 40), { side: 0, drop: 0 })
  assert.deepEqual(eyesToward(2, 15, 40), { side: 0, drop: 1 })
  assert.deepEqual(eyesToward(-6, -15, 40), { side: -1, drop: -1 })
  // Toward a corner, further across than down: a sideways look only.
  assert.deepEqual(eyesToward(18, 12, 40), { side: 1, drop: 0 })
})

test('the view is the smallest square that holds the figure, so it fills its container without cropping', () => {
  for (const seed of seeds(512)) {
    const { view, cells } = buildSprite(seed)
    const xs = cells.map(cell => cell.x), ys = cells.map(cell => cell.y)
    const width = Math.max(...xs) - Math.min(...xs) + 1, height = Math.max(...ys) - Math.min(...ys) + 1
    assert.equal(view.size, Math.max(width, height), `${seed}: not the tightest square`)
    for (const { x, y } of cells)
      assert.ok(x >= view.x && x + 1 <= view.x + view.size && y >= view.y && y + 1 <= view.y + view.size, `${seed}: ${x},${y} is cropped`)
  }
})
