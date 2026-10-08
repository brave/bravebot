import test from 'node:test'
import assert from 'node:assert/strict'
import {
  buildSprite, spriteOf, placeEyes, hueDistance, contrast,
  eyesToward, GRID, EYE_WHITE, EYE_CONTRAST, HUE_DISTANCE, PAINT,
} from '../src/renderer/avatar/pixels.ts'

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

test('a face chooses paints at least HUE_DISTANCE apart, and its eye whites stand out from the fill around them', () => {
  for (const seed of seeds(512)) {
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
