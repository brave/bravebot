/**
 * One clock for every animated face on screen.
 *
 * A face moves by whole cells: the pupils change sides, the eyes drop a row, the lids close. So
 * each tick works out how every registered face is held, and writes attributes only for a face
 * whose look changed since the last tick. React renders a face once and is not involved after.
 *
 * The look is a function of the clock and of what the bot is doing, so two views of the same bot
 * move together and a dropped frame is skipped rather than accumulated. Each face's offset into
 * the cycle comes from its seed, which keeps a column of them from moving as one.
 *
 * The loop stops when nothing is registered, when the window is hidden, and under reduced motion,
 * where a face only changes when what it is doing changes.
 */

import { blink, glance, completionNod, fidget, randomUnit } from './motion'
import {
  ARM_LAG, BOB, BREATH, CROWN_LAG, RUNNING, SLEEP_BREATH,
  choreographyOf, fidgetFor, gestureFor, weightOf, type Gesture, type Weight,
} from './gestures'
import { EYE_PARTS, eyesToward, placeEyes, spriteOf, type Look, type Sprite } from './pixels'

/**
 * What a bot is doing, as far as its face is concerned. There is no mouth: the eyes show where the
 * bot is looking, and a face large enough to show detail moves its body and its eyes as the state
 * changes (`../avatar/gestures.ts`).
 *
 * - `idle`: in a list, not the one on screen. Long pauses between short glances.
 * - `waiting`: never spoken to. Holds its gaze and only blinks.
 * - `open`: the one on screen, or the one whose row is selected. Holds its gaze.
 * - `thinking`: a turn is running and the model is writing, with no tool at work. Eyes up a row,
 *   looking from side to side.
 * - `working`: a turn is running a tool. Eyes down a row, with occasional scanning glances.
 * - `asking`: a turn is waiting on the person's answer. Looks straight out at them.
 * - `failed`: the last turn ended in an error. Looks away for a few seconds, then back.
 *
 * On a turn ending in `open`, a small face pauses, then nods once. A detailed one jumps for joy
 * and closes its eyes into happy arches; on failing it startles with wide eyes, then slumps while
 * it looks away. A detailed face also:
 *
 * - breathes, with a weight its body gives it (`weightOf`), and moves every gesture with it;
 * - leans in as work starts, perks up as thinking starts or a question is asked, and while it
 *   waits for an answer bounces now and then;
 * - perks up when the pointer arrives over it, squashes under a press and springs back when let
 *   go, and giggles when pressed three times quickly; in a list, choosing its row by pointer or by
 *   Enter or Space is a press that always ends in a giggle;
 * - when it is the open one, watches the field being typed in;
 * - glances toward a neighbour that cheers, startles or giggles;
 * - in a list, dozes off when nothing has happened to it for a while, and wakes with a stretch;
 * - and now and then fidgets at rest.
 */
export type Doing = 'idle' | 'waiting' | 'open' | 'thinking' | 'working' | 'asking' | 'failed'

/** Range of per-bot clock offsets, in seconds. */
const CYCLE = 24
/** How long a failed bot looks away before looking back, in seconds. */
const LOOK_AWAY = 3
/** The least time between two ticks, in milliseconds. A blink's lids are closed for about 75. */
const AT_LEAST = 1000 / 30
/** When a detailed face's eyes are happy after work finishes, in seconds after it finished. */
const HAPPY = { from: 0.3, to: 1.6 }
/** How long a detailed face's eyes stay wide after a failure, in seconds. */
const WIDE = 0.45
/** The least time between perking up at the pointer, and between any gesture and a fidget. */
const PERK_EVERY = 4
const FIDGET_GAP = 2
/** How long a face in a list goes without anything happening to it before it dozes off. */
export const SLEEP_AFTER = 40
/** How often a face waiting on an answer bounces, in seconds. */
const NUDGE_EVERY = 5
/** How long a face watches a neighbour that cheered, startled or giggled, and how far away it notices one, in CSS pixels. */
const NOTICE = 1.5
const NOTICE_REACH = 280
/** How many presses, each within this long of the last, make a face giggle, and how long its eyes stay happy after. */
const GIGGLE = { presses: 3, within: 1.4, happy: 1.1 }
/** How long a face keeps watching the field after the last key, in seconds. */
const TYPING_HOLD = 1.5

export interface Registered {
  svg: SVGSVGElement
  seed: string
  sprite: Sprite
  phase: number
  doing: Doing
  /** What it was doing before, and when that changed, so the change can be played. */
  was: Doing
  since: number
  /** The last look written, so an unchanged face is not written again. */
  written: string
  /** Which way the pointer is from this face while it is over the face's frame, else `null`. */
  aim: ReturnType<typeof eyesToward> | null
  /**
   * Whether the face is drawn large enough for detail: it breathes, gestures, and shows happy and
   * wide eyes. A small face keeps to whole-cell moves and nods instead.
   */
  detail?: boolean
  /** The gesture playing, so the next one replaces it, and when the last one started. */
  playing?: Animation[]
  played?: number
  /** The fidget window this face last fidgeted in. */
  fidgeted?: number
  /** How its body moves. */
  weight?: Weight
  /** When something last happened to the face: a state change, the pointer, a press, a neighbour. */
  attended?: number
  /** Whether it is held down, and when it was last let go of, for a giggle. */
  pressed?: boolean
  /** Whether the press is a row being chosen, which ends in a giggle however many came before. */
  choosing?: boolean
  releases?: number[]
  giggled?: number
  /** A neighbour it is glancing at, and until when. */
  noticing?: { aim: Aim; until: number }
  /** Where the field being typed in is from this face, while it is watching it. */
  watching?: Aim | null
  /** When it last bounced waiting on an answer. */
  nudged?: number
  /** The breathing rhythm last started, so a new one is started where the clock is. */
  rhythm?: string
}

type Aim = ReturnType<typeof eyesToward>

/** Where the pointer last was, and what it was over. `null` once it has left the window. */
let pointer: { x: number; y: number; target: EventTarget | null } | null = null
let moved = false

/**
 * Whether the pointer is over this face's frame, which is the element around the picture and
 * the status marker, and if so which way it is from the middle of the picture. Hit-tested rather
 * than measured, so a dialog laid over a face does not make it look through the dialog.
 */
function aimAt(entry: Registered): void {
  const frame = entry.svg.parentElement
  // Only a face under the pointer is measured, so a move costs one layout read, not one per face.
  if (!pointer || !frame || !(pointer.target instanceof Node) || !frame.contains(pointer.target)) {
    entry.aim = null
    return
  }
  const box = entry.svg.getBoundingClientRect()
  entry.aim = box.width > 0
    ? eyesToward(pointer.x - (box.left + box.width / 2), pointer.y - (box.top + box.height / 2), box.width)
    : null
}

/**
 * Which way a point is from a face, for something it watches rather than something under it: any
 * point below the face's middle row is a look down.
 */
function eyesAt(dx: number, dy: number, size: number): Aim {
  return {
    side: Math.abs(dx) < size * 0.1 ? 0 : dx > 0 ? 1 : -1,
    drop: dy > size / 2 ? 1 : dy < -size / 2 ? -1 : 0,
  }
}

/** The middle of an element on screen, or nothing for one that is not laid out. */
function middle(element: Element): { x: number; y: number; size: number } | null {
  const box = element.getBoundingClientRect()
  return box.width > 0 ? { x: box.left + box.width / 2, y: box.top + box.height / 2, size: box.width } : null
}

/** The field last typed in, and until when the open faces watch it. */
let typed: { target: Element; until: number } | null = null
let typedMoved = false
let typedMeasured = 0

let stillness = false
if (typeof matchMedia === 'function') {
  const query = matchMedia('(prefers-reduced-motion: reduce)')
  stillness = query.matches
  query.addEventListener('change', () => {
    stillness = query.matches
    // A gesture already playing is a Web Animation, which `animation: none` does not stop, and a
    // held press would stay squashed because the release that ends it is refused under stillness.
    if (stillness) {
      for (const entry of registered.values()) {
        for (const running of entry.playing ?? []) running.cancel()
        entry.playing = undefined
        entry.pressed = false
      }
    }
    for (const entry of registered.values()) paint(entry, performance.now() / 1000)
    schedule()
  })
}

const registered = new Map<SVGSVGElement, Registered>()
let frame: number | null = null
let drawn = 0

/** How a face is held when nothing is moving it: the look React renders before the clock starts. */
export function lookOf(sprite: Sprite, doing: Doing): Look {
  return {
    gaze: sprite.gaze,
    drop: doing === 'working' ? 1 : doing === 'thinking' ? -1 : 0,
    eyes: 'open',
    nod: false,
  }
}

/** How a face is held at this moment. `still` is whether the person asked for reduced motion. */
export function lookAt(entry: Registered, seconds: number, still = stillness): Look {
  const look = lookOf(entry.sprite, entry.doing)
  if (still) return look
  const t = seconds + entry.phase
  const elapsed = seconds - entry.since
  const away = -entry.sprite.gaze as 1 | -1
  if (entry.doing === 'idle' && Math.abs(glance(t, entry.seed)) > 0.5) look.gaze = away
  if (entry.doing === 'working' && Math.abs(glance(t * 1.5, entry.seed)) > 0.5) look.gaze = away
  if (entry.doing === 'failed') look.gaze = elapsed < LOOK_AWAY ? away : entry.sprite.gaze
  // Thinking looks from one side to the other, slowly.
  if (entry.doing === 'thinking') look.gaze = Math.floor(t / 2.5) % 2 === 0 ? entry.sprite.gaze : away
  // A bot under the pointer looks at it. One at work, or just failed, keeps its pose.
  const free = entry.doing === 'working' ? false : entry.doing !== 'failed' || elapsed >= LOOK_AWAY
  const toward = (aim: Aim) => {
    if (aim.side) look.gaze = aim.side
    look.drop = aim.drop
  }
  if (free && entry.doing === 'open' && entry.watching) toward(entry.watching)
  if (free && entry.noticing && seconds < entry.noticing.until) toward(entry.noticing.aim)
  if (free && entry.aim) toward(entry.aim)
  if (blink(t, entry.seed) < 0.5) look.eyes = 'blink'
  const finished = RUNNING.includes(entry.was) && entry.doing === 'open'
  if (entry.detail) {
    if (dozing(entry, seconds)) look.eyes = 'sleepy'
    if (finished && elapsed >= HAPPY.from && elapsed < HAPPY.to) look.eyes = 'happy'
    if (entry.giggled !== undefined && seconds - entry.giggled < GIGGLE.happy) look.eyes = 'happy'
    if ((entry.doing === 'failed' || entry.doing === 'asking') && elapsed < WIDE) look.eyes = 'wide'
  } else {
    look.nod = finished && completionNod(elapsed) > 0.08
  }
  return look
}

/**
 * Whether a detailed face has dozed off: one in a list, with nothing having happened to it for
 * `SLEEP_AFTER` seconds. The pointer over it keeps it awake.
 */
export function dozing(entry: Registered, seconds: number): boolean {
  return !!entry.detail && (entry.doing === 'idle' || entry.doing === 'waiting') && !entry.aim &&
    entry.attended !== undefined && seconds - entry.attended >= SLEEP_AFTER
}

/**
 * Write a face's look into its SVG, if it differs from what was last written. `force` is for a
 * change React has just rendered too, which may have rewritten attributes behind the clock.
 */
function paint(entry: Registered, seconds: number, force = false): void {
  const look = lookAt(entry, seconds)
  const key = JSON.stringify(look)
  if (key === entry.written && !force) return
  entry.written = key
  const { svg, sprite } = entry
  const set = (element: Element | null, name: string, value: string | null) => {
    if (!element) return
    if (value === null) element.removeAttribute(name)
    else element.setAttribute(name, value)
  }
  const frame = svg.parentElement
  // A dozing face breathes slowly, which `avatar.css` draws from this.
  set(frame, 'data-asleep', look.eyes === 'sleepy' ? '' : null)
  rhythm(entry, seconds, look.eyes === 'sleepy')
  set(svg.querySelector('[data-part="figure"]'), 'transform', look.nod ? 'translate(0 1)' : null)
  set(svg.querySelector('[data-part="eyes"]'), 'transform', look.drop ? `translate(0 ${look.drop})` : null)
  placeEyes(sprite, look).forEach((eye, i) => {
    const group = svg.querySelector(`[data-eye="${i}"]`)
    if (!group) return
    for (const part of EYE_PARTS) {
      const rect = group.querySelector(`[data-part="${part}"]`)
      const at = eye.rects[part]
      set(rect, 'x', String(at.x))
      set(rect, 'y', String(at.y))
      set(rect, 'width', String(at.width))
      set(rect, 'height', String(at.height))
      set(rect, 'visibility', at.hidden ? 'hidden' : null)
    }
  })
}

/**
 * Play a gesture on a detailed face, in place of any it is playing. The keyframes add to the
 * breathing rather than replacing it, so a gesture ends where the breath has got to.
 */
function play(entry: Registered, gesture: Gesture, seconds: number): void {
  if (!entry.detail || stillness || typeof entry.svg.animate !== 'function') return
  for (const running of entry.playing ?? []) running.cancel()
  const { duration, figure, crown, arms } = choreographyOf(gesture, entry.weight)
  // A press holds its squash until the release that follows it takes over.
  const fill: FillMode = gesture === 'press' ? 'forwards' : 'none'
  const timing = (delay = 0): KeyframeAnimationOptions => ({ duration, delay, fill, composite: 'add' })
  const started = [entry.svg.animate(figure, timing())]
  if (crown) for (const part of entry.svg.querySelectorAll('[data-part="crown"]')) started.push(part.animate(crown, timing(CROWN_LAG)))
  if (arms) {
    entry.svg.querySelectorAll('[data-part="arm"]').forEach((part, i) => {
      started.push(part.animate(arms, timing(i * ARM_LAG)))
    })
  }
  entry.playing = started
  entry.played = seconds
  if (gesture === 'cheer' || gesture === 'startle' || gesture === 'giggle') notice(entry, seconds)
}

/** Turn the eyes of the detailed faces near one that just made a scene toward it, and wake them. */
function notice(from: Registered, seconds: number): void {
  const there = middle(from.svg)
  if (!there) return
  for (const entry of registered.values()) {
    if (entry === from || !entry.detail || !['idle', 'waiting', 'open'].includes(entry.doing)) continue
    const here = middle(entry.svg)
    if (!here || Math.hypot(there.x - here.x, there.y - here.y) > NOTICE_REACH) continue
    entry.noticing = { aim: eyesAt(there.x - here.x, there.y - here.y, here.size), until: seconds + NOTICE }
    entry.attended = seconds
  }
}

/**
 * Start a detailed face's breathing where the clock is, whenever its rhythm changes, so two views
 * of one bot breathe together. `avatar.css` picks the rhythm from what the bot is doing and whether
 * it dozes, and restarts the animation when it changes; only then is the delay set, because
 * changing a running animation's delay would jump it. A heavy body breathes more slowly.
 */
function rhythm(entry: Registered, seconds: number, asleep: boolean): void {
  const frame = entry.svg.parentElement
  if (!entry.detail || !frame) return
  const name = asleep ? 'doze' : entry.doing === 'working' ? 'bob' : entry.doing === 'thinking' ? 'ponder' : 'breathe'
  if (entry.rhythm === name) return
  entry.rhythm = name
  const tempo = entry.weight?.tempo ?? 1
  const period = (asleep ? SLEEP_BREATH : entry.doing === 'working' ? BOB : BREATH) * tempo
  const offset = (seconds + randomUnit(entry.seed) * period) % period
  frame.style.setProperty('--avatar-tempo', String(tempo))
  frame.style.setProperty('--avatar-delay', `${-offset.toFixed(3)}s`)
}

/** A fidget, if a detailed face at rest has reached the moment in its window for one. */
function maybeFidget(entry: Registered, seconds: number): void {
  if (!entry.detail || entry.aim || entry.pressed || (entry.doing !== 'idle' && entry.doing !== 'open')) return
  if (dozing(entry, seconds)) return
  const t = seconds + entry.phase
  const { cycle, start, kind } = fidget(t, entry.seed)
  // Only within a second of the moment, so a window that was hidden through it does not catch up.
  if (entry.fidgeted === cycle || t < start || t > start + 1) return
  entry.fidgeted = cycle
  if (entry.played !== undefined && seconds - entry.played < FIDGET_GAP) return
  play(entry, fidgetFor(kind, entry.sprite.traits.crown !== 'none'), seconds)
}

/** A bounce, every `NUDGE_EVERY` seconds, from a detailed face waiting on an answer. */
function maybeNudge(entry: Registered, seconds: number): void {
  if (!entry.detail || entry.doing !== 'asking' || entry.pressed) return
  if (seconds - Math.max(entry.since, entry.nudged ?? -Infinity) < NUDGE_EVERY) return
  entry.nudged = seconds
  play(entry, 'nudge', seconds)
}

/** Where the field being typed in is from each open face, measured at most a few times a second. */
function watch(seconds: number): void {
  if (!typedMoved || seconds - typedMeasured < 0.3) {
    if (typed && seconds >= typed.until) {
      typed = null
      for (const entry of registered.values()) entry.watching = null
    }
    return
  }
  typedMoved = false
  typedMeasured = seconds
  const there = typed && typed.target.isConnected ? middle(typed.target) : null
  for (const entry of registered.values()) {
    const here = entry.doing === 'open' && there ? middle(entry.svg) : null
    entry.watching = here && there ? eyesAt(there.x - here.x, there.y - here.y, here.size) : null
  }
}

function tick(): void {
  frame = null
  if (registered.size === 0 || stillness) return
  const now = performance.now()
  // A little under the interval is allowed, since frames arrive every 16.7ms.
  if (now - drawn >= AT_LEAST - 4) {
    drawn = now
    // Where every face is is read once per move rather than once per pointer event.
    const seconds = now / 1000
    if (moved) {
      moved = false
      for (const entry of registered.values()) {
        const before = entry.aim
        const asleep = dozing(entry, seconds)
        aimAt(entry)
        if (!entry.aim) continue
        entry.attended = seconds
        // Perks up as the pointer arrives, not while it moves about over the face. A dozing face
        // wakes with a stretch instead, however recently it moved.
        if (asleep) play(entry, 'wake', seconds)
        else if (!before && entry.doing !== 'working' && !entry.pressed && (entry.played === undefined || seconds - entry.played >= PERK_EVERY))
          play(entry, 'perk', seconds)
      }
    }
    watch(seconds)
    for (const entry of registered.values()) {
      maybeFidget(entry, seconds)
      maybeNudge(entry, seconds)
      paint(entry, seconds)
    }
  }
  schedule()
}

function schedule(): void {
  if (frame !== null || registered.size === 0 || stillness) return
  if (typeof document !== 'undefined' && document.hidden) return
  frame = requestAnimationFrame(tick)
}

/** Start moving a face, and answer how to stop. */
export function show(svg: SVGSVGElement, seed: string, doing: Doing = 'idle', detail = false): () => void {
  const sprite = spriteOf(seed)
  const entry: Registered = {
    svg,
    seed,
    sprite,
    phase: randomUnit(seed) * CYCLE,
    doing,
    was: doing,
    // Well in the past, so a face mounted while failed does not look away as if it just failed.
    since: performance.now() / 1000 - 60,
    written: '',
    aim: null,
    detail,
    weight: weightOf(sprite.traits.body),
    attended: performance.now() / 1000,
  }
  registered.set(svg, entry)
  aimAt(entry)
  // Forced, because React may have rendered a different face into this element, or rewritten
  // some of its attributes, since the last look the clock wrote to it.
  paint(entry, performance.now() / 1000, true)
  schedule()
  return () => {
    for (const running of entry.playing ?? []) running.cancel()
    registered.delete(svg)
  }
}

/** Tell a face what its bot is doing now. A no-op for the state it is already in. */
export function tell(svg: SVGSVGElement, doing: Doing): void {
  const entry = registered.get(svg)
  if (!entry || entry.doing === doing) return
  entry.was = entry.doing
  entry.doing = doing
  entry.since = performance.now() / 1000
  entry.attended = entry.since
  const gesture = gestureFor(entry.was, doing)
  if (gesture) play(entry, gesture, entry.since)
  paint(entry, entry.since, true)
  schedule()
}

/** How a face on screen is held at a moment, ignoring reduced motion. For the tests. */
export function lookFor(svg: SVGSVGElement, seconds: number): Look | null {
  const entry = registered.get(svg)
  return entry ? lookAt(entry, seconds, false) : null
}

/** Pop a face in, for a new look chosen in the avatar editor. */
export function pop(svg: SVGSVGElement): void {
  const entry = registered.get(svg)
  if (!entry) return
  entry.attended = performance.now() / 1000
  play(entry, 'pop', entry.attended)
}

/**
 * Tell the clock a key was typed in a field, so the open face watches it. Only where the field is
 * is read, and only on the next tick.
 */
export function typing(): void {
  if (typeof document === 'undefined' || !document.activeElement) return
  typed = { target: document.activeElement, until: performance.now() / 1000 + TYPING_HOLD }
  typedMoved = true
  schedule()
}

/**
 * The detailed face a press on an element presses, if any: the one whose frame holds it, or the one
 * inside the control around it that says, with `data-presses-face`, that choosing it is touching its
 * face. A bot's row in a list says so, and choosing the bot makes its face giggle. `choosing` is
 * whether the press is on such a control.
 */
function faceAt(target: EventTarget | null): { entry: Registered; choosing: boolean } | null {
  if (!(target instanceof Node)) return null
  const holder = (target instanceof Element ? target : target.parentElement)?.closest('[data-presses-face]')
  for (const entry of registered.values()) {
    if (!entry.detail) continue
    if (holder?.contains(entry.svg)) return { entry, choosing: true }
    if (entry.svg.parentElement?.contains(target)) return { entry, choosing: false }
  }
  return null
}

/** Let go of a pressed face: it springs back, or, at the third press in a row or on choosing it, giggles. */
function release(entry: Registered, seconds: number): void {
  entry.pressed = false
  const choosing = entry.choosing
  entry.choosing = false
  const last = entry.releases?.at(-1)
  // A press too long after the one before starts the count again.
  entry.releases = last !== undefined && seconds - last <= GIGGLE.within ? [...entry.releases!, seconds] : [seconds]
  if (choosing || entry.releases.length >= GIGGLE.presses) {
    entry.releases = []
    entry.giggled = seconds
    play(entry, 'giggle', seconds)
  } else {
    play(entry, 'release', seconds)
  }
}

/** Press a face and let go of it, for the pointer handlers and the tests. */
export function press(svg: SVGSVGElement, choosing = false): void {
  const entry = registered.get(svg)
  if (!entry?.detail) return
  entry.pressed = true
  entry.choosing = choosing
  entry.attended = performance.now() / 1000
  play(entry, 'press', entry.attended)
  schedule()
}
export function letGo(svg: SVGSVGElement): void {
  const entry = registered.get(svg)
  if (!entry?.pressed) return
  release(entry, performance.now() / 1000)
  schedule()
}

if (typeof document !== 'undefined') {
  // A press on a face squashes it; the press still reaches whatever the face is in.
  document.addEventListener('pointerdown', (event) => {
    if (event.button !== 0) return
    const hit = faceAt(event.target)
    if (hit) press(hit.entry.svg, hit.choosing)
  }, { passive: true })
  const letGoAll = () => {
    for (const entry of registered.values()) if (entry.pressed) letGo(entry.svg)
  }
  document.addEventListener('pointerup', letGoAll, { passive: true })
  document.addEventListener('pointercancel', letGoAll, { passive: true })
  // Choosing a row with Enter or Space touches its face as a click does: down presses, up lets go.
  // A key held down repeats, and only its first press counts.
  document.addEventListener('keydown', (event) => {
    if ((event.key !== 'Enter' && event.key !== ' ') || event.repeat) return
    const hit = faceAt(event.target)
    if (hit && !hit.entry.pressed) press(hit.entry.svg, hit.choosing)
  }, { passive: true })
  document.addEventListener('keyup', (event) => {
    if (event.key === 'Enter' || event.key === ' ') letGoAll()
  }, { passive: true })
  // The pointer is only recorded here; the clock reads it on its next tick, so a burst of events
  // costs nothing. A touch or pen is not a pointer anyone is looking from.
  document.addEventListener('pointermove', (event) => {
    if (event.pointerType !== 'mouse') return
    pointer = { x: event.clientX, y: event.clientY, target: event.target }
    moved = true
    schedule()
  }, { passive: true })
  // Leaving the window sends every pair of eyes back to what it was doing.
  document.addEventListener('pointerout', (event) => {
    if (event.relatedTarget !== null) return
    pointer = null
    moved = true
    schedule()
  }, { passive: true })
  // A window that comes back from being hidden has a stopped loop, because `schedule` refused to
  // start one. Nothing else restarts it, so this does.
  document.addEventListener('visibilitychange', () => {
    if (!document.hidden) schedule()
  })
}
