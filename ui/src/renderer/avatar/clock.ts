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

import { blink, glance, completionNod, randomUnit } from './motion'
import { LID, eyesToward, placeEyes, spriteOf, type Look, type Sprite } from './pixels'

/**
 * What a bot is doing, as far as its face is concerned. Not a mood: there is no mouth, and the
 * eyes show only where the bot is looking.
 *
 * - `idle`: in a list, not the one on screen. Long pauses between short glances.
 * - `waiting`: never spoken to. Holds its gaze and only blinks.
 * - `open`: the one on screen, or the one whose row is selected. Holds its gaze.
 * - `working`: a turn is running. Eyes down a row, with occasional scanning glances.
 * - `failed`: the last turn ended in an error. Looks away for a few seconds, then back.
 *
 * On returning from `working` to `open`, the face pauses, then nods once.
 */
export type Doing = 'idle' | 'waiting' | 'open' | 'working' | 'failed'

export type Expression = 'neutral' | 'curious' | 'wink'

/** Range of per-bot clock offsets, in seconds. */
const CYCLE = 24
/** How long a failed bot looks away before looking back, in seconds. */
const LOOK_AWAY = 3
/** The least time between two ticks, in milliseconds. A blink's lids are closed for about 75. */
const AT_LEAST = 1000 / 30

interface Registered {
  svg: SVGSVGElement
  seed: string
  sprite: Sprite
  phase: number
  doing: Doing
  expression: Expression
  /** What it was doing before, and when that changed, so the change can be played. */
  was: Doing
  since: number
  /** The last look written, so an unchanged face is not written again. */
  written: string
  /** Which way the pointer is from this face while it is over the face's frame, else `null`. */
  aim: ReturnType<typeof eyesToward> | null
}

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
  const box = entry.svg.getBoundingClientRect()
  entry.aim = pointer && frame && box.width > 0 && pointer.target instanceof Node && frame.contains(pointer.target)
    ? eyesToward(pointer.x - (box.left + box.width / 2), pointer.y - (box.top + box.height / 2), box.width)
    : null
}

let stillness = false
if (typeof matchMedia === 'function') {
  const query = matchMedia('(prefers-reduced-motion: reduce)')
  stillness = query.matches
  query.addEventListener('change', () => {
    stillness = query.matches
    for (const entry of registered.values()) paint(entry, performance.now() / 1000)
    schedule()
  })
}

const registered = new Map<SVGSVGElement, Registered>()
let frame: number | null = null
let drawn = 0

/** How a face is held when nothing is moving it: the look React renders before the clock starts. */
export function lookOf(sprite: Sprite, doing: Doing, expression: Expression): Look {
  return {
    gaze: sprite.gaze,
    drop: doing === 'working' ? 1 : 0,
    blink: false,
    curious: expression === 'curious',
    wink: expression === 'wink',
    nod: false,
  }
}

/** How a face is held at this moment. */
function lookAt(entry: Registered, seconds: number): Look {
  const look = lookOf(entry.sprite, entry.doing, entry.expression)
  if (stillness) return look
  const t = seconds + entry.phase
  const elapsed = seconds - entry.since
  const away = -entry.sprite.gaze as 1 | -1
  if (entry.doing === 'idle' && Math.abs(glance(t, entry.seed)) > 0.5) look.gaze = away
  if (entry.doing === 'working' && Math.abs(glance(t * 1.5, entry.seed)) > 0.5) look.gaze = away
  if (entry.doing === 'failed') look.gaze = elapsed < LOOK_AWAY ? away : entry.sprite.gaze
  // A bot under the pointer looks at it. One at work, or just failed, keeps its pose.
  const free = entry.doing === 'working' ? false : entry.doing !== 'failed' || elapsed >= LOOK_AWAY
  if (free && entry.aim) {
    if (entry.aim.side) look.gaze = entry.aim.side
    // A curious eye already uses the row above, so it can only look down.
    look.drop = look.curious ? (Math.max(0, entry.aim.drop) as 0 | 1) : entry.aim.drop
  }
  look.blink = blink(t, entry.seed) < 0.5
  look.nod = entry.was === 'working' && entry.doing === 'open' && completionNod(elapsed) > 0.08
  return look
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
  set(svg.querySelector('[data-part="figure"]'), 'transform', look.nod ? 'translate(0 1)' : null)
  set(svg.querySelector('[data-part="eyes"]'), 'transform', look.drop ? `translate(0 ${look.drop})` : null)
  placeEyes(sprite, look).forEach((eye, i) => {
    const group = svg.querySelector(`[data-eye="${i}"]`)
    if (!group) return
    for (const [part, x] of [['white', eye.whiteX], ['pupil', eye.pupilX]] as const) {
      const rect = group.querySelector(`[data-part="${part}"]`)
      set(rect, 'x', String(x))
      set(rect, 'y', String(eye.y))
      set(rect, 'height', String(eye.height))
      set(rect, 'visibility', eye.closed ? 'hidden' : null)
    }
    const lid = group.querySelector('[data-part="lid"]')
    set(lid, 'x', String(Math.min(eye.whiteX, eye.pupilX)))
    set(lid, 'y', String(sprite.eyes[i]!.y + LID.offset))
    set(lid, 'visibility', eye.closed ? null : 'hidden')
  })
}

function tick(): void {
  frame = null
  if (registered.size === 0 || stillness) return
  const now = performance.now()
  // A little under the interval is allowed, since frames arrive every 16.7ms.
  if (now - drawn >= AT_LEAST - 4) {
    drawn = now
    // Where every face is is read once per move rather than once per pointer event.
    if (moved) {
      moved = false
      for (const entry of registered.values()) aimAt(entry)
    }
    for (const entry of registered.values()) paint(entry, now / 1000)
  }
  schedule()
}

function schedule(): void {
  if (frame !== null || registered.size === 0 || stillness) return
  if (typeof document !== 'undefined' && document.hidden) return
  frame = requestAnimationFrame(tick)
}

/** Start moving a face, and answer how to stop. */
export function show(svg: SVGSVGElement, seed: string, doing: Doing = 'idle', expression: Expression = 'neutral'): () => void {
  const sprite = spriteOf(seed)
  const entry: Registered = {
    svg,
    seed,
    sprite,
    phase: randomUnit(seed) * CYCLE,
    doing,
    expression,
    was: doing,
    // Well in the past, so a face mounted while failed does not look away as if it just failed.
    since: performance.now() / 1000 - 60,
    written: JSON.stringify(lookOf(sprite, doing, expression)),
    aim: null,
  }
  registered.set(svg, entry)
  aimAt(entry)
  paint(entry, performance.now() / 1000)
  schedule()
  return () => {
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
  paint(entry, entry.since, true)
  schedule()
}

/** Change the mascot's eyes without changing its task posture. */
export function express(svg: SVGSVGElement, expression: Expression): void {
  const entry = registered.get(svg)
  if (!entry || entry.expression === expression) return
  entry.expression = expression
  paint(entry, performance.now() / 1000, true)
  schedule()
}

if (typeof document !== 'undefined') {
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
