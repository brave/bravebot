// How a face is held for what its bot is doing.
//
// `lookAt` is a function of the registered face and the time, so the rules for working, failed,
// finishing and reduced motion are checked on plain entries rather than through a window. The
// expected gaze comes from the sprite's own resting side, not from the clock.

import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'
import { fileURLToPath } from 'node:url'
import { glance, completionNod } from '../src/renderer/avatar/motion.ts'
import { spriteOf } from '../src/renderer/avatar/pixels.ts'

const require = createRequire(import.meta.url)

function load(path) {
  const source = buildSync({
    entryPoints: [fileURLToPath(new URL(path, import.meta.url))],
    bundle: true,
    write: false,
    platform: 'node',
    format: 'cjs',
  }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
}

const { lookAt, show, tell } = load('../src/renderer/avatar/clock.ts')

const SEED = 'v2:clock-1'
const sprite = spriteOf(SEED)
const rest = sprite.gaze
const away = -sprite.gaze

/** A registered face with no clock offset, so `seconds` is the time the face itself sees. */
const face = (over = {}) => ({
  svg: null, seed: SEED, sprite, phase: 0, doing: 'open',
  was: 'open', since: 0, written: '', aim: null, ...over,
})

/** The first time, in seconds, that a glance is under way (`scale` is how the state stretches time). */
function glancing(scale) {
  for (let t = 0; t < 240; t += 0.05) if (Math.abs(glance(t * scale, SEED)) > 0.5) return t
  throw new Error('no glance found')
}

/** A time at which no glance is under way. */
function calm(scale) {
  for (let t = 0; t < 240; t += 0.05) if (Math.abs(glance(t * scale, SEED)) <= 0.5) return t
  throw new Error('no calm found')
}

test('a bot at work looks down a row for as long as it works, and glances away only now and then', () => {
  const at = glancing(1.5)
  const working = face({ doing: 'working' })
  for (let t = 0; t < 60; t += 0.7) assert.equal(lookAt(working, t, false).drop, 1)
  assert.equal(lookAt(working, calm(1.5), false).gaze, rest)
  assert.equal(lookAt(working, at, false).gaze, away)
})

test('a bot at work keeps its pose when the pointer is over it', () => {
  const t = calm(1.5)
  const look = lookAt(face({ doing: 'working', aim: { side: away, drop: -1 } }), t, false)
  assert.equal(look.gaze, rest)
  assert.equal(look.drop, 1)
})

test('a bot that has not failed looks at the pointer over it, even in the middle of a glance', () => {
  const idle = face({ doing: 'idle', aim: { side: rest, drop: -1 } })
  const look = lookAt(idle, glancing(1), false)
  assert.equal(look.gaze, rest)
  assert.equal(look.drop, -1)
  // Straight above or below the middle there is no side to turn to, so the gaze is left as it was.
  assert.equal(lookAt(face({ doing: 'open', aim: { side: 0, drop: 1 } }), calm(1), false).gaze, rest)
})

test('a failed bot looks away for three seconds, then back, and ignores the pointer until then', () => {
  const failed = (elapsed, aim) => lookAt(face({ doing: 'failed', since: 10, aim }), 10 + elapsed, false)
  assert.equal(failed(0.5, null).gaze, away)
  assert.equal(failed(2.9, null).gaze, away)
  assert.equal(failed(3.1, null).gaze, rest)
  assert.equal(failed(1, { side: rest, drop: 1 }).gaze, away)
  assert.equal(failed(1, { side: rest, drop: 1 }).drop, 0)
  assert.equal(failed(4, { side: away, drop: 1 }).gaze, away)
  assert.equal(failed(4, { side: away, drop: 1 }).drop, 1)
})

test('a bot nods once after work finishes, and only then', () => {
  const peak = (0.55 + 0.25)
  assert.ok(completionNod(peak) > 0.08, 'the nod peaks at the time this test samples')
  const nodding = (over, elapsed = peak) => lookAt(face({ since: 5, ...over }), 5 + elapsed, false).nod
  assert.equal(nodding({ was: 'working', doing: 'open' }), true)
  assert.equal(nodding({ was: 'working', doing: 'open' }, 0.1), false)
  assert.equal(nodding({ was: 'working', doing: 'open' }, 2), false)
  assert.equal(nodding({ was: 'idle', doing: 'open' }), false)
  assert.equal(nodding({ was: 'working', doing: 'failed' }), false)
  assert.equal(nodding({ was: 'working', doing: 'idle' }), false)
})

test('under reduced motion a face is held as its state says and nothing moves it', () => {
  const still = (over, t) => lookAt(face(over), t, true)
  // A glance, a pointer, a nod, a failure's look-away and a blink would each change a moving face here.
  assert.equal(still({ doing: 'idle' }, glancing(1)).gaze, rest)
  assert.equal(still({ doing: 'working' }, glancing(1.5)).gaze, rest)
  assert.equal(still({ doing: 'working', aim: { side: away, drop: -1 } }, 1).drop, 1)
  assert.equal(still({ doing: 'open', aim: { side: away, drop: 1 } }, 1).gaze, rest)
  assert.equal(still({ doing: 'open', aim: { side: away, drop: 1 } }, 1).drop, 0)
  assert.equal(still({ doing: 'failed', since: 0 }, 1).gaze, rest)
  assert.equal(still({ was: 'working', doing: 'open', since: 0 }, 0.8).nod, false)
  for (let t = 0; t < 30; t += 0.05) assert.equal(still({}, t).blink, false)
})

test('telling a face what its bot is doing changes how it is held', () => {
  globalThis.requestAnimationFrame = () => 1
  const parts = {}
  const part = (name) => parts[name] ??= {
    attributes: {},
    setAttribute(key, value) { this.attributes[key] = value },
    removeAttribute(key) { delete this.attributes[key] },
  }
  const svg = {
    parentElement: null,
    querySelector: (selector) => (selector === '[data-part="eyes"]' ? part('eyes') : null),
  }
  const stop = show(svg, SEED, 'open')
  assert.equal(parts.eyes.attributes.transform, undefined)
  tell(svg, 'working')
  assert.equal(parts.eyes.attributes.transform, 'translate(0 1)')
  tell(svg, 'open')
  assert.equal(parts.eyes.attributes.transform, undefined)
  stop()
  tell(svg, 'working')
  assert.equal(parts.eyes.attributes.transform, undefined, 'a face that was stopped is not written to')
})
