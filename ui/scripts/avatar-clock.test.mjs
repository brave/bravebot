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
import { blink, glance, completionNod } from '../src/renderer/avatar/motion.ts'
import { spriteOf, BODY_NAMES } from '../src/renderer/avatar/pixels.ts'

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

// The clock reads the preference when it loads, so the test holds the query it listens to.
const reduced = { matches: false, listeners: [] }
globalThis.matchMedia = () => ({
  get matches() { return reduced.matches },
  addEventListener: (_, listener) => reduced.listeners.push(listener),
})
const { lookAt, show, tell, press, letGo, pop, dozing, SLEEP_AFTER, lookFor } = load('../src/renderer/avatar/clock.ts')
const { GESTURES, ARM_LAG, WEIGHTS, NOT_FROM_REST, HELD, gestureFor, fidgetFor, choreographyOf, weightOf } = load('../src/renderer/avatar/gestures.ts')

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

test('a small face nods once after work finishes, and only then', () => {
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
  for (let t = 0; t < 30; t += 0.05) assert.equal(still({}, t).eyes, 'open')
  // A detailed face is held still too: no happy eyes after work, no wide eyes on failing.
  assert.equal(still({ detail: true, was: 'working', doing: 'open', since: 0 }, 0.8).eyes, 'open')
  assert.equal(still({ detail: true, doing: 'failed', since: 0 }, 0.1).eyes, 'open')
})

/** The first time after `from` at which the face is not mid-blink, so a shape is its own. */
function unblinking(from) {
  for (let t = from; t < from + 30; t += 0.01) if (blink(t, SEED) >= 0.5) return t
  throw new Error('no open eyes found')
}

test('a detailed face closes its eyes happily after work finishes, where a small one nods', () => {
  const at = (over, elapsed) => lookAt(face({ since: 5, ...over }), unblinking(5 + elapsed), false)
  const finished = { was: 'working', doing: 'open' }
  assert.equal(at({ ...finished, detail: true }, 0.8).eyes, 'happy')
  assert.equal(at({ ...finished, detail: true }, 0.8).nod, false, 'the hop takes the place of the nod')
  assert.equal(at({ ...finished, detail: true }, 0.05).eyes, 'open', 'not before it has jumped')
  assert.equal(at({ ...finished, detail: true }, 2).eyes, 'open', 'and not for long')
  assert.equal(at({ was: 'idle', doing: 'open', detail: true }, 0.8).eyes, 'open', 'only after work')
  assert.equal(at({ was: 'working', doing: 'failed', detail: true }, 0.8).eyes === 'happy', false)
  assert.equal(at(finished, 0.8).eyes, 'open', 'a small face does not')
  assert.equal(at(finished, 0.8).nod, true)
})

test('a detailed face startles wide-eyed when a turn fails, then looks away', () => {
  const failed = (elapsed, detail = true) => lookAt(face({ doing: 'failed', since: 10, detail }), unblinking(10 + elapsed), false)
  assert.equal(failed(0.1).eyes, 'wide')
  assert.equal(failed(0.1).gaze, away)
  assert.equal(failed(1).eyes, 'open')
  assert.equal(failed(1).gaze, away)
  assert.equal(failed(0.1, false).eyes, 'open', 'a small face does not')
})

test('a gesture plays for each change of state that has one, and only for those', () => {
  assert.equal(gestureFor('working', 'open'), 'cheer')
  assert.equal(gestureFor('thinking', 'open'), 'cheer', 'a turn that only wrote still finished')
  assert.equal(gestureFor('asking', 'open'), 'cheer')
  assert.equal(gestureFor('open', 'working'), 'lean')
  assert.equal(gestureFor('thinking', 'working'), 'lean')
  assert.equal(gestureFor('idle', 'working'), 'lean')
  assert.equal(gestureFor('open', 'thinking'), 'perk')
  assert.equal(gestureFor('working', 'thinking'), null, 'the turn goes on')
  assert.equal(gestureFor('working', 'asking'), 'perk')
  assert.equal(gestureFor('working', 'failed'), 'startle')
  assert.equal(gestureFor('open', 'failed'), 'startle')
  assert.equal(gestureFor('working', 'idle'), null, 'a bot finishing off screen does not cheer')
  assert.equal(gestureFor('idle', 'open'), null)
  assert.equal(gestureFor('failed', 'open'), null)
  assert.equal(gestureFor('open', 'open'), null)
})

test('every gesture but a held press ends at rest and runs forward, and all but two start at rest', () => {
  const rest = /^translateY\(0(\.0)?(%|px)\) rotate\(0deg\)( scale\(1, 1\))?$/
  for (const weight of Object.values(WEIGHTS))
    for (const name of Object.keys(GESTURES)) {
      const { duration, ...tracks } = choreographyOf(name, weight)
      assert.ok(duration > 0, name)
      for (const [track, frames] of Object.entries(tracks)) {
        if (!frames) continue
        assert.equal(frames[0].offset, 0, `${name}.${track} starts at the start`)
        assert.equal(frames.at(-1).offset, 1, `${name}.${track} ends at the end`)
        if (!NOT_FROM_REST.includes(name)) assert.match(frames[0].transform, rest, `${name}.${track} starts at rest`)
        if (!HELD.includes(name)) assert.match(frames.at(-1).transform, rest, `${name}.${track} ends at rest`)
        for (let i = 1; i < frames.length; i++) assert.ok(frames[i].offset > frames[i - 1].offset, `${name}.${track} runs forward`)
      }
    }
  // A release takes over a held press without a jump.
  for (const weight of Object.values(WEIGHTS)) {
    const press = choreographyOf('press', weight), release = choreographyOf('release', weight)
    assert.equal(release.figure[0].transform, press.figure.at(-1).transform)
    assert.equal(release.crown[0].transform, press.crown.at(-1).transform)
  }
})

test('a heavy body moves more slowly and leaves the ground less than a light one, and squashes more', () => {
  const height = (gesture) => Math.min(...gesture.figure.map(frame => Number(/translateY\((-?[0-9.]+)%\)/.exec(frame.transform)[1])))
  const squash = (gesture) => Math.max(...gesture.figure.map(frame => Number(/scale\(([0-9.]+),/.exec(frame.transform)[1])))
  const [heavy, middling, light] = [WEIGHTS.heavy, WEIGHTS.middling, WEIGHTS.light].map(weight => choreographyOf('cheer', weight))
  assert.ok(heavy.duration > middling.duration && middling.duration > light.duration)
  assert.ok(height(heavy) > height(middling) && height(middling) > height(light), 'up is negative')
  assert.ok(squash(heavy) > squash(middling) && squash(middling) > squash(light))
  assert.equal(weightOf('block'), WEIGHTS.heavy)
  assert.equal(weightOf('diamond'), WEIGHTS.light)
  for (const body of BODY_NAMES) assert.ok(Object.values(WEIGHTS).includes(weightOf(body)), body)
})

test('a fidget wiggles an antenna only on a face that has a crown', () => {
  const kinds = Array.from({ length: 30 }, (_, i) => i / 30)
  assert.deepEqual(new Set(kinds.map(kind => fidgetFor(kind, true))), new Set(['sway', 'hop', 'wiggle']))
  assert.deepEqual(new Set(kinds.map(kind => fidgetFor(kind, false))), new Set(['sway', 'hop']))
})

test('a bot thinking looks up a row, and from one side to the other', () => {
  const thinking = face({ doing: 'thinking' })
  const gazes = new Set()
  for (let t = 0; t < 10; t += 0.5) {
    const look = lookAt(thinking, t, false)
    assert.equal(look.drop, -1)
    gazes.add(look.gaze)
  }
  assert.deepEqual(gazes, new Set([rest, away]))
  assert.equal(lookAt(thinking, 0, true).drop, -1, 'held up under reduced motion too')
})

test('a detailed face asked a question opens its eyes wide, then looks straight out', () => {
  const asking = (elapsed, detail = true) => lookAt(face({ doing: 'asking', since: 10, detail }), unblinking(10 + elapsed), false)
  assert.equal(asking(0.1).eyes, 'wide')
  assert.equal(asking(1).eyes, 'open')
  assert.equal(asking(1).gaze, rest)
  assert.equal(asking(1).drop, 0)
  assert.equal(asking(0.1, false).eyes, 'open', 'a small face does not')
})

test('a detailed face in a list dozes off when nothing has happened to it for a while', () => {
  const at = (over, t) => lookAt(face({ detail: true, attended: 0, ...over }), unblinking(t), false).eyes
  assert.equal(at({ doing: 'idle' }, SLEEP_AFTER + 1), 'sleepy')
  assert.equal(at({ doing: 'waiting' }, SLEEP_AFTER + 1), 'sleepy')
  assert.notEqual(at({ doing: 'idle' }, SLEEP_AFTER - 1), 'sleepy', 'not before')
  assert.notEqual(at({ doing: 'open' }, SLEEP_AFTER + 1), 'sleepy', 'the one on screen stays awake')
  assert.notEqual(at({ doing: 'idle', aim: { side: rest, drop: 0 } }, SLEEP_AFTER + 1), 'sleepy', 'the pointer keeps it awake')
  assert.notEqual(at({ doing: 'idle', detail: false }, SLEEP_AFTER + 1), 'sleepy', 'a small face does not')
  assert.equal(dozing(face({ detail: true, attended: 0, doing: 'idle' }), SLEEP_AFTER), true)
})

test('an open face watches the field being typed in, and glances at a neighbour that made a scene', () => {
  const below = { side: -rest, drop: 1 }
  assert.deepEqual(pick(lookAt(face({ doing: 'open', watching: below }), calm(1), false)), [-rest, 1])
  assert.deepEqual(pick(lookAt(face({ doing: 'working', watching: below }), calm(1.5), false)), [rest, 1], 'one at work keeps to its work')
  assert.deepEqual(pick(lookAt(face({ doing: 'idle', watching: below }), calm(1), false)), [rest, 0], 'only the open one watches')
  const noticing = { aim: { side: -rest, drop: -1 }, until: 10 }
  assert.deepEqual(pick(lookAt(face({ doing: 'idle', noticing }), 9, false)), [-rest, -1])
  assert.deepEqual(pick(lookAt(face({ doing: 'idle', noticing }), 11, false)), [rest, 0], 'and then looks back')
  // The pointer over a face wins over both.
  assert.deepEqual(pick(lookAt(face({ doing: 'open', watching: below, noticing, aim: { side: rest, drop: 0 } }), 9, false)), [rest, 0])
})
const pick = (look) => [look.gaze, look.drop]

/**
 * A pretend SVG that records the gestures played on it, with a frame, one crown and two arms,
 * laid out at `x`, `y` with side `size`.
 */
function stage({ x = 0, y = 0, size = 40 } = {}) {
  const played = []
  const animated = (name) => ({
    animate(frames, options) {
      const animation = { name, frames, options, cancelled: false, cancel() { this.cancelled = true } }
      played.push(animation)
      return animation
    },
  })
  const element = (name) => ({
    ...animated(name),
    attributes: {},
    setAttribute(key, value) { this.attributes[key] = value },
    removeAttribute(key) { delete this.attributes[key] },
  })
  const crown = animated('crown')
  const arms = [animated('arm'), animated('arm')]
  const frame = { ...element('frame'), style: { values: {}, setProperty(key, value) { this.values[key] = value } }, contains: () => false }
  const svg = {
    ...element('figure'),
    parentElement: frame,
    querySelector: () => null,
    querySelectorAll: (selector) => (selector === '[data-part="crown"]' ? [crown] : selector === '[data-part="arm"]' ? arms : []),
    getBoundingClientRect: () => ({ left: x, top: y, width: size, height: size }),
  }
  return { svg, frame, played }
}

/** The clock and this test each bundle the keyframes, so they are compared by value. */
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b)
const weight = weightOf(sprite.traits.body)
const moves = (gesture) => choreographyOf(gesture, weight)

test('a detailed face plays the gesture its state change calls for, replacing the one before', () => {
  globalThis.requestAnimationFrame = () => 1
  const { svg, frame, played } = stage()
  const stop = show(svg, SEED, 'open', true)
  assert.match(frame.style.values['--avatar-delay'], /^-?[0-9.]+s$/, 'the breath starts where the clock is')
  assert.equal(frame.style.values['--avatar-tempo'], String(weight.tempo), 'at its body\'s tempo')
  tell(svg, 'working')
  const lean = played.filter(each => each.name === 'figure')
  assert.equal(lean.length, 1)
  assert.ok(same(lean[0].frames, moves('lean').figure), 'performed with its body\'s weight')
  assert.equal(lean[0].options.composite, 'add', 'over the breath, not in place of it')
  tell(svg, 'open')
  const of = (frames) => played.filter(each => same(each.frames, frames))
  assert.ok(of(moves('lean').figure).every(each => each.cancelled), 'the lean gives way to the cheer')
  const cheer = of(moves('cheer').figure)
  assert.equal(cheer.length, 1)
  const arms = of(moves('cheer').arms).filter(each => each.name === 'arm')
  assert.deepEqual(arms.map(each => each.options.delay), [0, ARM_LAG], 'the second arm trails the first')
  assert.equal(cheer[0].cancelled, false)
  stop()
  assert.equal(cheer[0].cancelled, true, 'a face taken off screen stops its gesture')
})

test('a small face plays no gestures, pressed, popped or changing state', () => {
  globalThis.requestAnimationFrame = () => 1
  const { svg, played } = stage()
  const stop = show(svg, SEED, 'open')
  tell(svg, 'working')
  tell(svg, 'open')
  tell(svg, 'failed')
  press(svg)
  letGo(svg)
  pop(svg)
  assert.deepEqual(played, [])
  stop()
})

test('a pressed face holds its squash until let go, springs back, and giggles at the third quick press', () => {
  globalThis.requestAnimationFrame = () => 1
  const { svg, played } = stage()
  const stop = show(svg, SEED, 'open', true)
  const figures = () => played.filter(each => each.name === 'figure')
  press(svg)
  assert.ok(same(figures().at(-1).frames, moves('press').figure))
  assert.equal(figures().at(-1).options.fill, 'forwards', 'the squash is held')
  letGo(svg)
  assert.ok(same(figures().at(-1).frames, moves('release').figure))
  assert.equal(figures().at(-2).cancelled, true, 'the release takes over the press')
  press(svg); letGo(svg)
  assert.ok(same(figures().at(-1).frames, moves('release').figure), 'two presses are not yet a giggle')
  press(svg); letGo(svg)
  assert.ok(same(figures().at(-1).frames, moves('giggle').figure))
  press(svg); letGo(svg)
  assert.ok(same(figures().at(-1).frames, moves('release').figure), 'the count starts again after a giggle')
  stop()
})

test('choosing a face\'s row giggles at the first press and takes no count towards the next', () => {
  globalThis.requestAnimationFrame = () => 1
  const { svg, played } = stage()
  const stop = show(svg, SEED, 'open', true)
  const figures = () => played.filter(each => each.name === 'figure')
  press(svg, true); letGo(svg)
  assert.ok(same(figures().at(-1).frames, moves('giggle').figure), 'a chosen row giggles')
  press(svg); letGo(svg)
  assert.ok(same(figures().at(-1).frames, moves('release').figure), 'the giggle starts the count again')
  stop()
})

test('a face changed in the editor pops in, and its neighbours notice a cheer', () => {
  globalThis.requestAnimationFrame = () => 1
  const one = stage({ x: 0, y: 0 })
  const near = stage({ x: 0, y: 60 })
  const far = stage({ x: 0, y: 2000 })
  const stops = [show(one.svg, SEED, 'working', true), show(near.svg, 'v2:near', 'idle', true), show(far.svg, 'v2:far', 'idle', true)]
  pop(one.svg)
  assert.ok(same(one.played.filter(each => each.name === 'figure').at(-1).frames, moves('pop').figure))
  tell(one.svg, 'open')
  assert.ok(same(one.played.filter(each => each.name === 'figure').at(-1).frames, moves('cheer').figure))
  // The neighbour below looks up at it; the one far down the list does not.
  const seconds = performance.now() / 1000
  assert.equal(lookFor(near.svg, seconds).drop, -1)
  assert.equal(lookFor(far.svg, seconds).drop, 0)
  for (const stop of stops) stop()
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

test('asking for reduced motion mid-gesture cancels what plays and lets a held press go', () => {
  globalThis.requestAnimationFrame = () => 1
  const { svg, played } = stage()
  const stop = show(svg, SEED, 'open', true)
  press(svg)
  const figure = played.filter(each => each.name === 'figure').at(-1)
  assert.equal(figure.cancelled, false)
  try {
    reduced.matches = true
    for (const listener of reduced.listeners) listener()
    assert.ok(played.every(each => each.cancelled), 'a Web Animation is not stopped by a style rule')
    const before = played.length
    letGo(svg)
    assert.equal(played.length, before, 'the press is already let go, so nothing is played on release')
  } finally {
    reduced.matches = false
    for (const listener of reduced.listeners) listener()
    stop()
  }
})
