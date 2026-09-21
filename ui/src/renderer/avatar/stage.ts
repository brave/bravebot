/**
 * One WebGL context, however many avatars are on screen.
 *
 * This is the whole reason there is a module here rather than a `<canvas>` inside the component.
 * A browser will not give a page an unbounded number of WebGL contexts — Chromium's limit is
 * around sixteen, and past it the *oldest* context is thrown away, so a list of twenty bots would
 * not fail loudly, it would silently blank the ones at the top. A column is exactly the case where
 * that happens.
 *
 * So there is one renderer, drawing into one small offscreen canvas, and every avatar is a plain
 * 2D canvas that gets the result copied into it with `drawImage`. The renderer paints each figure
 * in turn, once per frame, and the copy costs a few thousand pixels. Anything else — a context per
 * avatar, or a texture atlas, or one big canvas positioned behind the list — is either fragile or
 * more machinery than a list of small pictures deserves.
 *
 * ## Consistency
 *
 * The motion is a function of the clock, not of how many frames have been drawn. Everything below
 * takes the elapsed time in seconds and computes a pose from it, so a figure turns at the same
 * rate on a busy machine as on an idle one, a dropped frame is skipped rather than accumulated,
 * and two avatars mounted a minute apart are at the same point in the turn. Each figure's own
 * offset into that cycle comes from its seed, which is what keeps a column of them from moving as
 * one block. The one thing that is not a pure function of the clock is what the bot is *doing* —
 * see `Doing` — which arrives as an event and is remembered with the moment it arrived, so that
 * the pose can still be computed from time: the time since the change.
 *
 * The loop stops when nothing is registered and when the window is hidden. An idle app should not
 * hold the GPU awake to rotate a picture nobody is looking at.
 */

import * as THREE from 'three'
import { buildFigure, traitsOf, type Figure } from './figure'
import { blink, glance, completionNod, crownMotion, randomUnit, smooth } from './motion'

/**
 * How wide the shared canvas is, in device pixels.
 *
 * One size for every avatar, scaled down into whichever canvas asked for it. Fixed rather than the
 * largest requested, because a renderer resized between two draws in the same frame resizes twice
 * a frame, and the difference between 18px and 40px is not worth that.
 */
const SIZE = 128

/** Range of per-bot clock offsets, in seconds. */
const TURN = 24

/**
 * What a bot is doing, as far as its face is concerned.
 *
 * Not a mood. The figure has no mouth for the reason `figure.ts` gives — a fixed expression is
 * wrong beside half of what a bot says — and an *animated* expression is worse: a worried face
 * beside a stack trace looks like the bot is apologising. What the face can honestly show is
 * posture, which is what a person shows across a room: whether they are looking at you, looking
 * down at something, or waiting.
 *
 * - `idle`: in a list, not the one on screen. Pauses between short glances.
 * - `waiting`: never spoken to. Faces forward and only blinks — a thing that has not started yet.
 * - `open`: the one on screen, or the one whose row is selected. Looks at the reader and holds.
 * - `working`: a turn is running. Looks down and aside with occasional small scanning movements.
 * - `failed`: the last turn ended in an error. A tilt of the head — "hm" — that settles back to
 *   `open` over a few seconds, rather than a held pose that would become a sulk.
 *
 * On returning from `working` to `open`, the head meets the reader, pauses, then nods once.
 */
export type Expression = 'neutral' | 'curious' | 'wink'

export type Doing = 'idle' | 'waiting' | 'open' | 'working' | 'failed'

/** One avatar waiting to be drawn. */
interface Registered {
  canvas: HTMLCanvasElement
  seed: string
  crownKind: ReturnType<typeof traitsOf>['crown']
  current: Pose | null
  from: Pose | null
  figure: Figure
  /** Where in the cycle this one sits, so a column does not move in lockstep. */
  phase: number
  /** How fast this one bobs, in radians a second. From the seed, so a column does not breathe as one. */
  bobRate: number
  /** Which way this one looks when it looks aside to work. From the seed, so a column varies. */
  aside: 1 | -1
  /** The head's built-in tilt, which every pose is relative to. */
  restPitch: number
  doing: Doing
  expression: Expression
  /** What it was doing before, and when it changed, so the two can be blended and the change played. */
  was: Doing
  since: number
  /** The crown's spring: its lean, and how fast the lean is changing. */
  crown: { x: number; z: number; vx: number; vz: number; yaw: number; pitch: number }
}

/**
 * Where a figure is asked to be: the parts of a pose that a state chooses, before the parts that
 * every state shares (the blink) are put on.
 */
interface Pose {
  /** Turn of the whole figure about the vertical, radians. Positive is the figure's left. */
  yaw: number
  /** Tilt of the head forward, radians. Positive looks down. */
  pitch: number
  /** Lean of the whole figure to one side, radians. */
  roll: number
  /** Height of the bob, figure units. */
  bob: number
}

/** Reduced motion is fully still, including eyes and accessories. State changes remain legible. */
let stillness = false
if (typeof matchMedia === 'function') {
  const query = matchMedia('(prefers-reduced-motion: reduce)')
  stillness = query.matches
  query.addEventListener('change', () => {
    stillness = query.matches
    schedule()
  })
}

let renderer: THREE.WebGLRenderer | null = null
let scene: THREE.Scene | null = null
let camera: THREE.PerspectiveCamera | null = null
let frame: number | null = null
/** Set once WebGL has been asked for and refused, so it is not asked again every mount. */
let refused = false

const registered = new Map<HTMLCanvasElement, Registered>()

/**
 * The renderer, or `null` where there is no WebGL to be had.
 *
 * Software rendering, a driver that will not start, a machine with the GPU process disabled: all
 * of them arrive as a throw from the constructor rather than as a flag to check. Answering `null`
 * lets the component fall back to something flat instead of failing to draw at all.
 */
function stage(): { renderer: THREE.WebGLRenderer; scene: THREE.Scene; camera: THREE.PerspectiveCamera } | null {
  if (refused) return null
  if (renderer && scene && camera) return { renderer, scene, camera }

  try {
    const canvas = document.createElement('canvas')
    canvas.width = SIZE
    canvas.height = SIZE
    renderer = new THREE.WebGLRenderer({ canvas, alpha: true, antialias: true })
    renderer.setSize(SIZE, SIZE, false)
    // The figures are lit by hand and their colours are chosen against the window's own palette,
    // so no tone mapping and no colour management: what the materials say is what is drawn, which
    // is what lets an avatar sit in a themed row without drifting away from it.
    renderer.setClearAlpha(0)
  } catch {
    refused = true
    renderer = null
    return null
  }

  scene = new THREE.Scene()

  // Framed as a portrait, not as a full-length picture. At thirty-odd pixels a whole figure is a
  // speck in a lot of air — the head has to fill most of the square or there is nothing to
  // recognise. So the camera sits close, centred on the face, with the body running out of the
  // bottom of the frame the way a headshot crops at the shoulders. What is left in view is the
  // head, whatever is on top of it, and enough of the body to say there is one.
  //
  // "Enough of the body" is the part that was wrong at first. With the camera at 6.1 and aimed at
  // the eyes, the body was a strip four pixels tall at the foot of a 38-pixel square and the collar
  // a sliver above it — two of the figure's six traits, drawn every frame and visible in none of
  // them. Sitting a little further back and aiming a little lower puts the shoulders and the
  // collar in the picture. The head is smaller for it, which is the trade, and it is still most of
  // the square.
  //
  // Slightly above and looking a little down, which is the angle a face is friendliest from: it
  // shows the roundness of the head where straight-on flattens it, and it keeps both eyes.
  camera = new THREE.PerspectiveCamera(28, 1, 0.1, 100)
  camera.position.set(0, 0.45, 6.9)
  camera.lookAt(0, -0.02, 0)

  // Soft and frontal. A hard key light gives a face a hollow, dramatic look — which is the exact
  // opposite of approachable — so the strong light is broad and almost head-on, the fill is
  // generous, and there is a cool rim behind to lift the silhouette off the column.
  //
  // Weighted heavily toward the flat terms, which is not how one would light a photograph and is
  // exactly right here. Lambert adds every light into the surface colour, so a strong key on a
  // saturated colour clips one channel long before the other two and the hue drains out — the
  // figure goes pale and stops being the colour the window is in. A large flat term with a modest
  // key keeps the surface at the accent proper and spends the directional light only on saying
  // which way the head is turned. Closer to how a cartoon is painted than to how a room is lit,
  // and a cartoon is what this is.
  //
  // The flat term is split between an ambient and a hemisphere. Ambient alone left the figures
  // flat — a red head at 38 pixels was a red disc — and the fix cannot be a stronger key, for the
  // clipping reason above. A hemisphere light is the cartoonist's answer: a little brighter on top
  // and a little darker underneath, everywhere, regardless of where the key is. The chin goes into
  // shadow and the sphere turns back into a sphere, and because the sky and ground are neutral
  // greys the hue does not move. A surface facing the camera is halfway between sky and ground and
  // gets a fixed amount from it, which is the amount the ambient below was reduced by.
  //
  // The numbers are measured rather than chosen. With these, a surface facing the camera renders at
  // its material colour *exactly* — sample a pixel off an avatar and it is the hex from the paint
  // set in `figure.ts`. That is the only defensible place to put them: anything less and every bot
  // is a muddied version of the colour it was painted, anything more and the saturated ones clip
  // and go pale. They are not intuitive, because the path from an intensity to a pixel runs through
  // three's colour management and Lambert's own scaling, so guessing lands nowhere near — if the
  // lighting is ever changed, check it by sampling a pixel rather than by eye.
  scene.add(new THREE.AmbientLight(0xffffff, 0.78))
  const sky = new THREE.HemisphereLight(0xffffff, 0xcccccc, 1.4)
  sky.position.set(0, 1, 0)
  scene.add(sky)
  const key = new THREE.DirectionalLight(0xffffff, 0.78)
  key.position.set(2.2, 3.4, 5)
  scene.add(key)
  const fill = new THREE.DirectionalLight(0xffffff, 0.22)
  fill.position.set(-3.4, -0.6, 2.6)
  scene.add(fill)
  const rim = new THREE.DirectionalLight(0xffffff, 0.3)
  rim.position.set(-1.4, 2.2, -4)
  scene.add(rim)

  return { renderer, scene, camera }
}

const mix = (a: number, b: number, k: number): number => a + (b - a) * k

/** How long a blend between two states takes, in seconds. */
const BLEND = 0.35

/** What one state asks for, at this moment. */
function ask(entry: Registered, doing: Doing, t: number, elapsed: number): Pose {
  const bob = stillness ? 0 : Math.sin(t * entry.bobRate)
  switch (doing) {
    case 'idle': {
      const s = stillness ? 0 : glance(t, entry.seed)
      return { yaw: s * 0.29, pitch: 0, roll: s * 0.018, bob: 0 }
    }
    case 'waiting':
      return { yaw: 0, pitch: 0, roll: 0, bob: 0 }
    case 'open':
      // A shade more up than the rest position: looking at somebody, not past them.
      return { yaw: 0, pitch: -0.04, roll: 0, bob: bob * 0.018 }
    case 'working': {
      // Down and aside, at the page. With a very slow, very small drift, which is what eyes do
      // over a page and is the difference between reading and a freeze-frame.
      const drift = stillness ? 0 : glance(t * 1.5, entry.seed) * 0.09
      return {
        yaw: entry.aside * 0.28 + drift,
        pitch: 0.32,
        roll: entry.aside * 0.03,
        bob: 0,
      }
    }
    case 'failed': {
      // The tilt, held for a moment and then let go over a few seconds toward `open`. A held tilt
      // stops being "hm" and becomes a pose, and a pose beside an error is a comment on it.
      const settle = stillness ? 1 : smooth((elapsed - 1.5) / 3)
      const open = ask(entry, 'open', t, elapsed)
      return {
        yaw: mix(entry.aside * 0.08, open.yaw, settle),
        pitch: mix(0.05, open.pitch, settle),
        roll: mix(entry.aside * 0.14, open.roll, settle),
        bob: mix(bob * 0.02, open.bob, settle),
      }
    }
  }
}

/**
 * Where a figure is at this moment.
 *
 * Blend from the last displayed pose so interrupted transitions stay continuous. Add the blink
 * and completion gesture, then let the accessory catch up with its own spring.
 */
function pose(entry: Registered, seconds: number, dt: number): void {
  const { figure } = entry
  const t = seconds + entry.phase
  const elapsed = seconds - entry.since
  const want = ask(entry, entry.doing, t, elapsed)
  const k = stillness ? 1 : smooth(elapsed / BLEND)
  // Blend from the last displayed pose, including when another transition interrupts a nod.
  const had = k < 1 && entry.from ? entry.from : want
  const yaw = mix(had.yaw, want.yaw, k)
  let pitch = mix(had.pitch, want.pitch, k)
  const roll = mix(had.roll, want.roll, k)
  const bob = mix(had.bob, want.bob, k)

  // Eye contact first, then a separate nod. Errors never acknowledge completion.
  if (!stillness && entry.was === 'working' && entry.doing === 'open') {
    pitch += completionNod(elapsed)
  }
  entry.current = { yaw, pitch, roll, bob }

  figure.root.rotation.y = yaw
  figure.root.rotation.z = roll
  figure.root.position.y = bob
  figure.head.rotation.x = entry.restPitch + pitch

  const open = stillness ? 1 : blink(t, entry.seed)
  const expression = entry.expression === 'curious' ? 1.3 : 1
  figure.eyes.scale.y = open * expression
  figure.glints.scale.y = expression
  // The catchlight is a reflection, and a reflection is not squashed by a closing lid — it is
  // covered. Hide it as the lid starts closing so it cannot float above the compressed eye.
  figure.glints.visible = open > 0.95
  // Close one eye for the mascot's click response, leaving the other eye and frame alone.
  const eye = figure.eyes.children[0]!
  eye.userData.restScaleY ??= eye.scale.y
  eye.scale.y = eye.userData.restScaleY * (entry.expression === 'wink' ? 0.08 : 1)
  figure.glints.children[0]!.visible = entry.expression !== 'wink'

  // The crown, a frame behind. A spring pulled toward leaning against the head's motion — turn the
  // head and the bobble swings the other way and wobbles back — which is follow-through, the
  // oldest trick in animation and the one that most cheaply says "this is a drawing, not a model".
  // Underdamped on purpose: the wobble is the point. Simulated on real time so it is the same
  // wobble at any frame rate, and clamped so a long pause between frames is not a catapult.
  if (figure.crown) {
    const spring = entry.crown
    if (stillness || dt <= 0) {
      spring.x = spring.z = spring.vx = spring.vz = 0
    } else {
      const duration = Math.min(dt, 0.1)
      const placed = Number.isFinite(spring.yaw)
      const yawRate = placed ? (yaw - spring.yaw) / duration : 0
      const pitchRate = placed ? (pitch - spring.pitch) / duration : 0
      const { stiffness, damping, response } = crownMotion(entry.crownKind)
      const towardZ = -yawRate * response
      const towardX = -pitchRate * response
      // Small integration steps keep the spring stable when the UI drops a frame.
      const steps = Math.ceil(duration / (1 / 120))
      const step = duration / steps
      for (let i = 0; i < steps; i++) {
        spring.vz += (-stiffness * (spring.z - towardZ) - damping * spring.vz) * step
        spring.vx += (-stiffness * (spring.x - towardX) - damping * spring.vx) * step
        spring.z += spring.vz * step
        spring.x += spring.vx * step
      }
    }
    spring.yaw = yaw
    spring.pitch = pitch
    const { limit } = crownMotion(entry.crownKind)
    figure.crown.rotation.z = Math.max(-limit, Math.min(limit, spring.z))
    figure.crown.rotation.x = Math.max(-limit, Math.min(limit, spring.x))
  }
}

/**
 * The least time between two draws, in milliseconds. Thirty frames a second, not sixty: the turn
 * takes twenty-four seconds and the bob about seven, so between one frame and the next at 30 a
 * figure moves a fraction of a device pixel — nobody can see the difference, and a list of twenty
 * bots is drawing twenty scenes a frame. Half the frames is half the GPU for the same picture.
 */
const AT_LEAST = 1000 / 30
let drawn = 0

function draw(): void {
  frame = null
  const built = stage()
  if (!built || registered.size === 0) return

  // Too soon since the last one: ask for the next frame and draw nothing. Cheaper than a timer,
  // and it keeps the loop on the display's own clock so a draw is never mid-refresh. A little
  // under the interval is allowed, since frames arrive at 16.7ms and a strict 33.3 would skip
  // every third one and land at 20 a second.
  const now = performance.now()
  if (now - drawn < AT_LEAST - 4) {
    schedule()
    return
  }
  const dt = drawn === 0 ? 0 : (now - drawn) / 1000
  drawn = now

  const seconds = now / 1000
  for (const entry of registered.values()) {
    // One figure in the scene at a time. Adding all of them and moving the camera would mean every
    // draw paying for every other bot's geometry.
    built.scene.add(entry.figure.root)
    pose(entry, seconds, dt)
    built.renderer.render(built.scene, built.camera)
    built.scene.remove(entry.figure.root)

    const context = entry.canvas.getContext('2d')
    if (!context) continue
    context.clearRect(0, 0, entry.canvas.width, entry.canvas.height)
    context.drawImage(built.renderer.domElement, 0, 0, entry.canvas.width, entry.canvas.height)
  }

  if (!stillness) schedule()
}

function schedule(): void {
  if (frame !== null || registered.size === 0) return
  // Not while the window is hidden: `requestAnimationFrame` is already throttled there, but the
  // intent is worth stating — nothing about this is worth waking a machine for.
  if (typeof document !== 'undefined' && document.hidden) return
  frame = requestAnimationFrame(draw)
}

/**
 * Put an avatar on screen, and answer how to take it off again.
 *
 * The figure is built once per canvas rather than per frame. Building one is a few dozen small
 * geometries, which is nothing to do once and wasteful to do sixty times a second.
 */
export function show(canvas: HTMLCanvasElement, seed: string, doing: Doing = 'idle', size = 38): () => void {
  if (!stage()) return () => undefined

  const figure = buildFigure(seed, size)
  registered.set(canvas, {
    canvas,
    seed,
    crownKind: traitsOf(seed).crown,
    current: null,
    from: null,
    figure,
    // Spread over the whole cycle from the seed, so a column of bots is a group of individuals
    // rather than a chorus line. Deterministic, like everything else drawn from a seed.
    phase: hashPhase(seed) * TURN,
    // A bob somewhere between five and a half and eight and a half seconds. With one rate for all
    // of them the phase offset was not enough: the turn and the bob were both on the same clock
    // and a column still rose and fell together.
    bobRate: (Math.PI * 2) / (5.5 + hashPhase(`${seed}/bob`) * 3),
    aside: hashPhase(`${seed}/aside`) < 0.5 ? -1 : 1,
    restPitch: figure.head.rotation.x,
    doing,
    expression: 'neutral',
    was: doing,
    // Well in the past, so the first pose is taken up without a blend from nothing.
    since: performance.now() / 1000 - 60,
    crown: { x: 0, z: 0, vx: 0, vz: 0, yaw: NaN, pitch: NaN },
  })
  schedule()

  return () => {
    registered.delete(canvas)
    figure.dispose()
  }
}

/** Change the mascot's eyes without moving its frame or changing its task posture. */
export function express(canvas: HTMLCanvasElement, expression: Expression): void {
  const entry = registered.get(canvas)
  if (!entry || entry.expression === expression) return
  entry.expression = expression
  schedule()
}

/**
 * Tell an avatar what its bot is doing now. A no-op for a state it is already in, so a component
 * can say so on every render without restarting a blend each time.
 */
export function tell(canvas: HTMLCanvasElement, doing: Doing): void {
  const entry = registered.get(canvas)
  if (!entry || entry.doing === doing) return
  entry.from = entry.current
  entry.was = entry.doing
  entry.doing = doing
  entry.since = performance.now() / 1000
  schedule()
}

const hashPhase = randomUnit

/** Whether anything can be drawn at all, for the component's fallback. */
export function available(): boolean {
  return stage() !== null
}

if (typeof document !== 'undefined') {
  // A window that comes back from being hidden has a stopped loop, because `schedule` refused to
  // start one. Nothing else restarts it, so this does.
  document.addEventListener('visibilitychange', () => {
    if (!document.hidden) schedule()
  })
}
