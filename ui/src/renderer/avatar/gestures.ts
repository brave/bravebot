/**
 * The moves a face makes with its whole body, as Web Animations keyframes.
 *
 * A gesture is a short, one-off movement that plays over the face's breathing: it squashes before
 * it jumps, stretches as it rises, squashes again as it lands and settles past its rest before it
 * stops. The crown and arms move a little after the body and overshoot it. Every keyframe set but
 * `HELD` ends at rest, and all but `NOT_FROM_REST` start there, so a gesture cut short by the next one
 * leaves nothing behind. A body performs each with its weight (`weightOf`).
 *
 * The figure's moves are fractions of its own size; a part's moves are in grid cells, which are
 * the user units of the picture. Pure: `clock.ts` decides when to play one.
 */

import type { Doing } from './clock'
import type { Body } from './pixels'

export type Gesture =
  | 'cheer' | 'lean' | 'startle' | 'perk' | 'sway' | 'hop' | 'wiggle'
  | 'press' | 'release' | 'giggle' | 'wake' | 'nudge' | 'pop'

/**
 * How a body carries its weight. A heavy one moves more slowly, leaves the ground less and squashes
 * more when it lands; a light one is quick and springy. `tempo` stretches time, `lift` scales how
 * high it goes, `squash` how far it deforms.
 */
export interface Weight { tempo: number; lift: number; squash: number }
export const WEIGHTS = {
  heavy: { tempo: 1.18, lift: 0.7, squash: 1.3 },
  middling: { tempo: 1, lift: 1, squash: 1 },
  light: { tempo: 0.85, lift: 1.3, squash: 0.8 },
} as const satisfies Record<string, Weight>

const HEFT: Record<Body, keyof typeof WEIGHTS> = {
  block: 'heavy', blob: 'heavy', squat: 'heavy', bust: 'heavy',
  dome: 'middling', tall: 'middling',
  round: 'light', diamond: 'light',
}

/** The weight a body moves with. */
export const weightOf = (body: Body): Weight => WEIGHTS[HEFT[body]]

/**
 * The breathing period, in seconds, and the working bob's. `avatar.css` animates with the same
 * durations; the clock uses these to start a face partway through so two views breathe together.
 */
export const BREATH = 3.4
export const BOB = 1.2
/** The breath of a face that has dozed off: slower, and `avatar.css` makes it deeper. */
export const SLEEP_BREATH = 5.6

interface Pose {
  /** Up is negative, as a fraction of the figure's height. */
  y?: number
  /** Degrees, about the middle of the figure's feet. */
  turn?: number
  /** Width and height scale. */
  wide?: number
  tall?: number
}

interface PartPose { y?: number; turn?: number }

const pose = ({ y = 0, turn = 0, wide = 1, tall = 1 }: Pose): string =>
  `translateY(${(y * 100).toFixed(1)}%) rotate(${turn}deg) scale(${wide}, ${tall})`
const partPose = ({ y = 0, turn = 0 }: PartPose): string => `translateY(${y}px) rotate(${turn}deg)`

/** One timeline as keyframes, from moments in milliseconds. Each step eases in and out. */
function frames<T>(duration: number, moments: [number, T][], write: (at: T) => string): Keyframe[] {
  return moments.map(([at, value]) => ({ offset: at / duration, transform: write(value), easing: 'ease-in-out' }))
}

export interface Choreography {
  duration: number
  figure: Keyframe[]
  crown?: Keyframe[]
  /** The arms, the left one's; the right one's runs the same keyframes a little later. */
  arms?: Keyframe[]
}

const REST: Pose = {}
/** Where `press` holds the figure and `release` lets it go from. */
const PRESSED: Pose = { wide: 1.1, tall: 0.86 }

interface Script {
  duration: number
  figure: [number, Pose][]
  crown?: [number, PartPose][]
  arms?: [number, PartPose][]
}

const choreograph = (
  duration: number,
  figure: [number, Pose][],
  parts: { crown?: [number, PartPose][]; arms?: [number, PartPose][] } = {},
): Script => ({ duration, figure, ...parts })

/** A pose moved by a weight: higher or lower, deformed more or less, about the same rest. */
const weigh = (at: Pose, { lift, squash }: Weight): Pose => ({
  y: (at.y ?? 0) * lift,
  turn: at.turn ?? 0,
  wide: +(1 + ((at.wide ?? 1) - 1) * squash).toFixed(3),
  tall: +(1 + ((at.tall ?? 1) - 1) * squash).toFixed(3),
})

function perform({ duration, figure, crown, arms }: Script, weight: Weight): Choreography {
  const lasting = Math.round(duration * weight.tempo)
  const later = <T,>(moments: [number, T][]): [number, T][] => moments.map(([at, value]) => [Math.round(at * weight.tempo), value])
  const parts = (moments: [number, PartPose][]): [number, PartPose][] =>
    later(moments).map(([at, { y = 0, turn = 0 }]) => [at, { y: +(y * weight.lift).toFixed(2), turn }])
  return {
    duration: lasting,
    figure: frames(lasting, later(figure).map(([at, value]) => [at, weigh(value, weight)]), pose),
    crown: crown && frames(lasting, parts(crown), partPose),
    arms: arms && frames(lasting, parts(arms), partPose),
  }
}

const SCRIPTS: Record<Gesture, Script> = {
  // A turn finished: crouch, jump, land and settle, with the crown and arms flung up behind.
  cheer: choreograph(900, [
    [0, REST],
    [120, { wide: 1.08, tall: 0.9 }],
    [300, { y: -0.16, wide: 0.95, tall: 1.07 }],
    [450, { y: -0.2 }],
    [590, { wide: 1.07, tall: 0.92 }],
    [720, { wide: 0.97, tall: 1.03 }],
    [820, { wide: 1.01, tall: 0.99 }],
    [900, REST],
  ], {
    crown: [[0, {}], [180, { y: 0.3 }], [360, { y: 0.6 }], [560, { y: -0.5 }], [680, { y: 0.4 }], [800, { y: -0.15 }], [900, {}]],
    arms: [[0, {}], [200, { y: 0.4 }], [380, { y: -1 }], [600, { y: -0.6 }], [720, { y: 0.3 }], [900, {}]],
  }),
  // A turn started: lean into it.
  lean: choreograph(340, [
    [0, REST],
    [120, { wide: 1.04, tall: 0.95 }],
    [240, { wide: 0.99, tall: 1.01 }],
    [340, REST],
  ], {
    crown: [[0, {}], [160, { y: 0.3 }], [280, { y: -0.1 }], [340, {}]],
  }),
  // A turn failed: jump in surprise, shake it off, then slump for as long as it looks away.
  startle: choreograph(3400, [
    [0, REST],
    [110, { y: -0.08, wide: 0.95, tall: 1.08 }],
    [260, { wide: 1.05, tall: 0.95 }],
    [360, { turn: -4 }],
    [450, { turn: 4 }],
    [540, { turn: -2 }],
    [640, { wide: 1.02, tall: 0.96 }],
    [3000, { wide: 1.02, tall: 0.96 }],
    [3400, REST],
  ], {
    crown: [[0, {}], [150, { y: -0.7 }], [320, { y: 0.3 }], [640, { y: 0.3 }], [3000, { y: 0.3 }], [3400, {}]],
    arms: [[0, {}], [130, { y: -1 }], [330, { y: 0.4 }], [640, { y: 0.6 }], [3000, { y: 0.6 }], [3400, {}]],
  }),
  // The pointer arrived: perk up.
  perk: choreograph(420, [
    [0, REST],
    [90, { wide: 1.03, tall: 0.97 }],
    [210, { y: -0.06, wide: 0.97, tall: 1.05 }],
    [330, { wide: 1.02, tall: 0.98 }],
    [420, REST],
  ], {
    crown: [[0, {}], [230, { y: 0.3 }], [340, { y: -0.2 }], [420, {}]],
  }),
  // Fidgets at rest.
  sway: choreograph(1600, [
    [0, REST],
    [400, { turn: -3 }],
    [900, { turn: 3 }],
    [1300, { turn: -1.5 }],
    [1600, REST],
  ], {
    crown: [[0, {}], [500, { turn: -6 }], [1000, { turn: 6 }], [1400, { turn: -2 }], [1600, {}]],
  }),
  hop: choreograph(900, [
    [0, REST],
    [80, { wide: 1.05, tall: 0.94 }],
    [190, { y: -0.08, wide: 0.97, tall: 1.04 }],
    [320, { wide: 1.04, tall: 0.95 }],
    [400, { wide: 1.05, tall: 0.94 }],
    [520, { y: -0.08, wide: 0.97, tall: 1.04 }],
    [660, { wide: 1.04, tall: 0.95 }],
    [800, { wide: 0.99, tall: 1.01 }],
    [900, REST],
  ], {
    crown: [[0, {}], [220, { y: 0.3 }], [340, { y: -0.3 }], [550, { y: 0.3 }], [680, { y: -0.3 }], [900, {}]],
    arms: [[0, {}], [220, { y: -0.5 }], [340, {}], [550, { y: -0.5 }], [680, {}], [900, {}]],
  }),
  wiggle: choreograph(1000, [
    [0, REST],
    [200, { wide: 0.99, tall: 1.01 }],
    [800, { wide: 0.99, tall: 1.01 }],
    [1000, REST],
  ], {
    crown: [[0, {}], [150, { turn: -12 }], [300, { turn: 12 }], [450, { turn: -10 }], [600, { turn: 8 }], [750, { turn: -4 }], [1000, {}]],
  }),
  // Pressed: squash under the pointer and stay there until let go. Played to hold its last frame.
  press: choreograph(120, [[0, REST], [120, PRESSED]], {
    crown: [[0, {}], [120, { y: 0.4 }]],
  }),
  // Let go: spring up out of the squash, overshoot and settle.
  release: choreograph(560, [
    [0, PRESSED],
    [140, { y: -0.1, wide: 0.93, tall: 1.09 }],
    [300, { wide: 1.05, tall: 0.95 }],
    [430, { wide: 0.98, tall: 1.02 }],
    [560, REST],
  ], {
    crown: [[0, { y: 0.4 }], [170, { y: -0.6 }], [320, { y: 0.3 }], [450, { y: -0.1 }], [560, {}]],
    arms: [[0, {}], [160, { y: -1 }], [330, { y: 0.3 }], [560, {}]],
  }),
  // Pressed again and again: a fit of giggles, a flurry of little hops and shakes.
  giggle: choreograph(1100, [
    [0, REST],
    [90, { y: -0.05, turn: -4, wide: 0.97, tall: 1.04 }],
    [180, { turn: 3, wide: 1.04, tall: 0.96 }],
    [270, { y: -0.05, turn: 4, wide: 0.97, tall: 1.04 }],
    [360, { turn: -3, wide: 1.04, tall: 0.96 }],
    [450, { y: -0.05, turn: -4, wide: 0.97, tall: 1.04 }],
    [540, { turn: 3, wide: 1.04, tall: 0.96 }],
    [700, { y: -0.03, turn: -1.5 }],
    [860, { wide: 1.02, tall: 0.98 }],
    [1100, REST],
  ], {
    crown: [[0, {}], [140, { turn: 10 }], [320, { turn: -10 }], [500, { turn: 10 }], [700, { turn: -5 }], [1100, {}]],
    arms: [[0, {}], [90, { y: -0.6 }], [180, {}], [270, { y: -0.6 }], [360, {}], [450, { y: -0.6 }], [540, {}], [1100, {}]],
  }),
  // Woken: a long stretch up, a shiver, and back.
  wake: choreograph(1000, [
    [0, REST],
    [350, { wide: 0.93, tall: 1.1 }],
    [520, { wide: 0.94, tall: 1.09 }],
    [640, { turn: -2, wide: 1.02, tall: 0.98 }],
    [740, { turn: 2 }],
    [860, { wide: 1.01, tall: 0.99 }],
    [1000, REST],
  ], {
    crown: [[0, {}], [380, { y: -0.6 }], [560, { y: -0.6 }], [700, { y: 0.2 }], [1000, {}]],
    arms: [[0, {}], [380, { y: -1 }], [560, { y: -1 }], [720, {}], [1000, {}]],
  }),
  // Waiting on an answer: a small, eager bounce on the spot.
  nudge: choreograph(520, [
    [0, REST],
    [90, { wide: 1.04, tall: 0.95 }],
    [200, { y: -0.05, wide: 0.98, tall: 1.03 }],
    [320, { wide: 1.03, tall: 0.97 }],
    [420, { wide: 0.99, tall: 1.01 }],
    [520, REST],
  ], {
    crown: [[0, {}], [220, { y: 0.3 }], [340, { y: -0.2 }], [520, {}]],
  }),
  // A new look in the editor: the new figure pops in out of a squash.
  pop: choreograph(420, [
    [0, { wide: 1.14, tall: 0.8 }],
    [130, { y: -0.06, wide: 0.93, tall: 1.09 }],
    [270, { wide: 1.03, tall: 0.97 }],
    [420, REST],
  ], {
    crown: [[0, { y: 0.5 }], [160, { y: -0.5 }], [300, { y: 0.15 }], [420, {}]],
  }),
}

const performed = new Map<string, Choreography>()

/** A gesture as a body of a given weight performs it. */
export function choreographyOf(gesture: Gesture, weight: Weight = WEIGHTS.middling): Choreography {
  const key = `${gesture}/${weight.tempo}/${weight.lift}/${weight.squash}`
  let known = performed.get(key)
  if (!known) performed.set(key, (known = perform(SCRIPTS[gesture], weight)))
  return known
}

/** Every gesture at a middling weight. */
export const GESTURES = Object.fromEntries(
  (Object.keys(SCRIPTS) as Gesture[]).map((name) => [name, choreographyOf(name)]),
) as Record<Gesture, Choreography>

/**
 * The gestures that do not start at rest: `release` starts where `press` left the figure, and `pop`
 * from a squash, since the face it starts on has only just been drawn. Each still ends at rest.
 */
export const NOT_FROM_REST: readonly Gesture[] = ['release', 'pop']
/** The gesture that does not end at rest: `press` holds its squash until a `release` replaces it. */
export const HELD: readonly Gesture[] = ['press']

/** How much later the right arm follows the left, so the two do not move as one piece. */
export const ARM_LAG = 60
/** How much later the crown follows the body. */
export const CROWN_LAG = 40

/** The states in which a turn is running. Leaving one of them for `open` is finishing it. */
export const RUNNING: readonly Doing[] = ['thinking', 'working', 'asking']

/** The gesture a change in what a bot is doing plays, if any. */
export function gestureFor(was: Doing, doing: Doing): Gesture | null {
  if (was === doing) return null
  if (doing === 'failed') return 'startle'
  if (doing === 'working') return 'lean'
  // Asked something: perk up at the person who has to answer.
  if (doing === 'asking') return 'perk'
  // Starting to think, from rest: look up and attend. From work, the turn simply goes on.
  if (doing === 'thinking') return RUNNING.includes(was) ? null : 'perk'
  if (RUNNING.includes(was) && doing === 'open') return 'cheer'
  return null
}

/** The fidget a face at rest plays for a draw in [0, 1): an antenna wiggle only if it has a crown. */
export function fidgetFor(kind: number, hasCrown: boolean): Gesture {
  const choices: Gesture[] = hasCrown ? ['sway', 'hop', 'wiggle'] : ['sway', 'hop']
  return choices[Math.min(choices.length - 1, Math.floor(kind * choices.length))]!
}
