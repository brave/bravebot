/**
 * A bot's face, as a grid of coloured cells.
 *
 * The silhouette is a Space Invaders figure: a body, something on top, arms and legs, built on
 * the left half of the grid and mirrored. The fill is a gradient between two or three hues, one
 * colour per cell, so the cells read as tiles. The eyes are two cells each, a white one and a
 * near-black pupil, and the side the pupil is on is where the bot is looking.
 *
 * Everything is a function of the seed, so a bot has one face for its whole life and the same
 * face in every window. Each trait reads its own stream (`${seed}/${trait}`), so adding a trait
 * later does not reshuffle the ones already here.
 *
 * Pure: no DOM, so the tests import it directly.
 */

/** The side of the grid, in cells. The outer ring is margin and is never painted. */
export const GRID = 12
const INNER = GRID - 2
/** Columns in one half, counted outward from the centre: `h = 0` is columns 5 and 6. */
const HALF = INNER / 2

export const EYE_WHITE = '#ffffff'
export const EYE_PUPIL = '#1d1f27'
/** The least contrast ratio between an eye's white cell and the fill around it. */
export const EYE_CONTRAST = 1.8
/** The least hue distance, in OKLCH degrees, between any two colours in one bot. */
export const HUE_DISTANCE = 60

/**
 * The colours a bot can be painted in. Bright, and far enough apart in hue that any two allowed
 * together make a gradient that reads as painted. Named, because the names go into `data-avatar`.
 */
export const PAINT: readonly { name: string; hex: string }[] = [
  { name: 'red', hex: '#e5484d' },
  { name: 'orange', hex: '#f5923a' },
  { name: 'yellow', hex: '#f2c94c' },
  { name: 'lime', hex: '#9fd848' },
  { name: 'green', hex: '#30a46c' },
  { name: 'teal', hex: '#12b5a5' },
  { name: 'cyan', hex: '#3cb4f0' },
  { name: 'blue', hex: '#3e63dd' },
  { name: 'violet', hex: '#8e4ec6' },
  { name: 'magenta', hex: '#d6409f' },
  { name: 'pink', hex: '#f38bb5' },
]

/** Body outlines as half-widths per row, top to bottom. A half-width of 5 fills the half. */
const BODIES = {
  dome: [2, 4, 5, 5, 5],
  squat: [3, 5, 5, 5],
  block: [5, 5, 5, 5, 4],
  round: [2, 3, 4, 4, 4, 3],
  tall: [3, 4, 4, 4, 4, 3],
  diamond: [1, 2, 3, 4, 4, 3],
  blob: [3, 4, 5, 5, 5, 4, 3],
  bust: [4, 4, 4, 4, 4, 4, 3],
} as const

/** Bodies drawn as a head with no legs, the way the gradient faces in the references are. */
const LEGLESS: readonly (keyof typeof BODIES)[] = ['blob', 'bust']

const CROWNS = ['none', 'antennae', 'spike', 'tuft', 'horns'] as const
const ARMS = ['none', 'raised', 'down', 'side'] as const
const LEGS = ['none', 'two', 'splayed', 'three', 'comb', 'feet'] as const
const DIRECTIONS = ['vertical', 'horizontal', 'diagonal', 'radial'] as const

export interface Traits {
  body: keyof typeof BODIES
  crown: (typeof CROWNS)[number]
  /** Only a body narrower than the half has room for arms; the others always have `none`. */
  arms: (typeof ARMS)[number]
  legs: (typeof LEGS)[number]
  eyes: 'close' | 'wide'
  /** The gradient's colours, by name, in order along it. */
  paints: string[]
  direction: (typeof DIRECTIONS)[number]
  /** Whether the outermost cells are faded toward white. */
  fringe: boolean
  /** `lopsided` when a few cells on one side break the mirror. */
  symmetry: 'mirror' | 'lopsided'
}

export interface Cell { x: number; y: number; fill: string }

export interface Sprite {
  traits: Traits
  /** Every painted cell, eye cells included, in row order. */
  cells: Cell[]
  /** The white-and-pupil pair of each eye, by its left cell: the bot's left eye first. */
  eyes: [{ x: number; y: number }, { x: number; y: number }]
  /** Which way the pupils sit at rest: 1 is toward the viewer's right. */
  gaze: 1 | -1
  /**
   * The square, in grid cells, that just contains the figure. The picture is drawn through it so
   * the figure fills its container, whatever its own width and height.
   */
  view: { x: number; y: number; size: number }
  signature: string
}

/** A stream of numbers in [0, 1): xorshift32 over an FNV-1a hash of the seed. */
export function stream(seed: string): () => number {
  let state = 0x811c9dc5
  for (let at = 0; at < seed.length; at++) {
    state ^= seed.charCodeAt(at)
    state = Math.imul(state, 0x01000193)
  }
  state ^= state >>> 16
  state = Math.imul(state, 0x7feb352d)
  state = (state ^ (state >>> 15)) >>> 0 || 1
  return () => {
    state ^= state << 13
    state ^= state >>> 17
    state ^= state << 5
    return (state >>> 0) / 0x100000000
  }
}

/** How the face is held at one moment. Every move is a whole cell, except the lid. */
export interface Look {
  gaze: 1 | -1
  /** Eyes moved by whole rows: 1 is down, at the work or at a pointer below; -1 is up. */
  drop: -1 | 0 | 1
  blink: boolean
  /** The whole figure one row down. */
  nod: boolean
}

export interface EyePlacement { whiteX: number; pupilX: number; y: number; height: number; closed: boolean }

/** Where each eye's white, pupil and lid go for a look, in grid cells. */
export function placeEyes(sprite: Sprite, look: Look): EyePlacement[] {
  return sprite.eyes.map((eye) => ({
    whiteX: look.gaze > 0 ? eye.x : eye.x + 1,
    pupilX: look.gaze > 0 ? eye.x + 1 : eye.x,
    y: eye.y,
    height: 1,
    closed: look.blink,
  }))
}

/**
 * Where the eyes turn to face a point over the figure, given its offset from the figure's centre
 * and the figure's side, in the same units. `side` is 0 near the middle column, where the eyes
 * keep their own gaze. `drop` is nonzero only when the point is mostly above or below the
 * middle, so a point toward a corner is a sideways look.
 */
export function eyesToward(dx: number, dy: number, size: number): { side: -1 | 0 | 1; drop: -1 | 0 | 1 } {
  const side = Math.abs(dx) < size * 0.1 ? 0 : dx > 0 ? 1 : -1
  const drop = Math.abs(dy) > size * 0.25 && Math.abs(dy) >= Math.abs(dx) ? (dy > 0 ? 1 : -1) : 0
  return { side, drop }
}

/** A closed eye is a thin dark line across both of its cells, a little below their middle. */
export const LID = { offset: 0.4, height: 0.3 }

// --- colour ---------------------------------------------------------------------------------

type Rgb = [number, number, number]
type Lch = { l: number; c: number; h: number }

const linear = (v: number): number => (v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4)
const gamma = (v: number): number => (v <= 0.0031308 ? 12.92 * v : 1.055 * v ** (1 / 2.4) - 0.055)

function rgbOf(hex: string): Rgb {
  const n = parseInt(hex.slice(1), 16)
  return [((n >> 16) & 255) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255]
}

function hexOf([r, g, b]: Rgb): string {
  const byte = (v: number) => Math.round(Math.min(1, Math.max(0, v)) * 255).toString(16).padStart(2, '0')
  return `#${byte(r)}${byte(g)}${byte(b)}`
}

function lchOf(hex: string): Lch {
  const [r, g, b] = rgbOf(hex).map(linear) as Rgb
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b)
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b)
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b)
  const L = 0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s
  const A = 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s
  const B = 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s
  return { l: L, c: Math.hypot(A, B), h: ((Math.atan2(B, A) * 180) / Math.PI + 360) % 360 }
}

function hexOfLch({ l: L, c, h }: Lch): string {
  const A = c * Math.cos((h * Math.PI) / 180)
  const B = c * Math.sin((h * Math.PI) / 180)
  const l = (L + 0.3963377774 * A + 0.2158037573 * B) ** 3
  const m = (L - 0.1055613458 * A - 0.0638541728 * B) ** 3
  const s = (L - 0.0894841775 * A - 1.291485548 * B) ** 3
  return hexOf([
    gamma(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s),
    gamma(-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s),
    gamma(-0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s),
  ])
}

/** The angle between two hues, in degrees, the short way round. */
export function hueDistance(a: string, b: string): number {
  const d = Math.abs(lchOf(a).h - lchOf(b).h) % 360
  return d > 180 ? 360 - d : d
}

/** WCAG contrast ratio between two colours. */
export function contrast(a: string, b: string): number {
  const luminance = (hex: string) => {
    const [r, g, bl] = rgbOf(hex).map(linear) as Rgb
    return 0.2126 * r + 0.7152 * g + 0.0722 * bl
  }
  const [x, y] = [luminance(a), luminance(b)].sort((p, q) => q - p) as [number, number]
  return (x + 0.05) / (y + 0.05)
}

/** The paints in hue order, so a gradient can walk from one to another through the ones between. */
const RING = [...PAINT].sort((a, b) => lchOf(a.hex).h - lchOf(b.hex).h)

/**
 * The stops from one paint to the next, through every paint between them the short way round.
 * Mixing two distant hues directly passes through a dull, low-chroma middle (red to green is
 * olive), and walking through the real paints keeps every step as bright as they are.
 */
function walk(names: string[]): Lch[] {
  const at = names.map((name) => RING.findIndex((paint) => paint.name === name))
  const stops: Lch[] = [lchOf(RING[at[0]!]!.hex)]
  for (let i = 1; i < at.length; i++) {
    const from = at[i - 1]!
    const to = at[i]!
    const forward = (to - from + RING.length) % RING.length
    const step = forward <= RING.length / 2 ? 1 : -1
    for (let k = from; k !== to; ) {
      k = (k + step + RING.length) % RING.length
      stops.push(lchOf(RING[k]!.hex))
    }
  }
  return stops
}

/** A point along the gradient, through OKLCH the short way round the hue circle. */
function along(stops: Lch[], t: number): Lch {
  const at = Math.min(0.9999, Math.max(0, t)) * (stops.length - 1)
  const i = Math.floor(at)
  const k = at - i
  const a = stops[i]!
  const b = stops[i + 1] ?? a
  let dh = b.h - a.h
  if (dh > 180) dh -= 360
  if (dh < -180) dh += 360
  return { l: a.l + (b.l - a.l) * k, c: a.c + (b.c - a.c) * k, h: (a.h + dh * k + 360) % 360 }
}

// --- shape ----------------------------------------------------------------------------------

const pick = <T,>(seed: string, key: string, from: readonly T[]): T =>
  from[Math.floor(stream(`${seed}/${key}`)() * from.length)]!

/** Which figure a seed describes, without drawing it. */
export function traitsOf(seed: string): Traits {
  return spriteOf(seed).traits
}

const cache = new Map<string, Sprite>()

/** The face a seed describes. Memoised, since every row of a list asks for it on each render. */
export function spriteOf(seed: string): Sprite {
  const known = cache.get(seed)
  if (known) return known
  if (cache.size > 512) cache.clear()
  const sprite = buildSprite(seed)
  cache.set(seed, sprite)
  return sprite
}

/** The face a seed describes, built afresh. `spriteOf` is the cached way in. */
export function buildSprite(seed: string): Sprite {
  const body = pick(seed, 'body', Object.keys(BODIES) as (keyof typeof BODIES)[])
  const widths: readonly number[] = BODIES[body]
  const widest = Math.max(...widths)
  const eyes: Traits['eyes'] = widest === HALF ? pick(seed, 'eyes', ['close', 'wide'] as const) : 'close'
  const crown = pick(seed, 'crown', CROWNS)
  const arms: Traits['arms'] = widest < HALF ? pick(seed, 'arms', ARMS) : 'none'
  const legs: Traits['legs'] = LEGLESS.includes(body) ? 'none' : pick(seed, 'legs', LEGS.slice(1))

  // Rows: the crown, the body, then two rows of legs, centred in the inner ten.
  const crownHeight = crown === 'none' ? 0 : crown === 'tuft' ? 1 : 2
  const height = crownHeight + widths.length + (legs === 'none' ? 0 : 2)
  const top = 1 + Math.floor((INNER - height) / 2) + crownHeight
  const bottom = top + widths.length - 1

  // The eyes sit as low on the body as leaves a body row under them, so looking down has
  // somewhere to go, and a row over them, so looking up has somewhere to go too.
  const outer = eyes === 'close' ? 2 : 3
  let eyeIndex = widths.length - (legs === 'none' ? 3 : 2)
  while (
    eyeIndex > 1 &&
    !(widths[eyeIndex]! >= outer + 2 && widths[eyeIndex - 1]! >= outer + 1 && widths[eyeIndex + 1]! >= outer + 1)
  ) eyeIndex--
  const eyeRow = top + eyeIndex

  // The left half, as (row, h) with h counted outward from the centre.
  const half: [number, number][] = []
  widths.forEach((width, i) => {
    for (let h = 0; h < width; h++) half.push([top + i, h])
  })
  const topWidth = widths[0]!
  const crownCells: Record<Traits['crown'], [number, number][]> = {
    none: [],
    antennae: [[-1, Math.min(HALF - 1, topWidth)], [-2, Math.min(HALF - 1, topWidth + 1)]],
    spike: [[-1, 0], [-2, 0]],
    tuft: Array.from({ length: Math.ceil(topWidth / 2) }, (_, i) => [-1, i * 2] as [number, number]),
    horns: [[-1, topWidth - 1], [-2, topWidth - 1]],
  }
  for (const [dr, h] of crownCells[crown]) half.push([top + dr, h])

  const edge = HALF - 1
  const armCells: Record<Traits['arms'], [number, number][]> = {
    none: [],
    raised: [[0, edge], [-1, edge], [-2, edge]],
    down: [[0, edge], [1, edge], [2, edge]],
    side: [[-1, edge], [0, edge], [1, edge]],
  }
  for (const [dr, h] of armCells[arms]) half.push([eyeRow + dr, h])

  const base = widths[widths.length - 1]!
  const legCells: Record<Traits['legs'], [number, number][]> = {
    none: [],
    two: [[1, base - 2], [2, base - 2]],
    splayed: [[1, base - 2], [2, Math.min(edge, base - 1)]],
    three: [[1, 0], [2, 0], [1, base - 1], [2, base - 1]],
    comb: Array.from({ length: Math.floor(base / 2) }, (_, i) => [[1, i * 2 + 1], [2, i * 2 + 1]] as [number, number][]).flat(),
    feet: [[1, base - 2], [2, base - 2], [2, Math.max(0, base - 3)]],
  }
  for (const [dr, h] of legCells[legs]) half.push([bottom + dr, h])

  const filled: boolean[][] = Array.from({ length: GRID }, () => Array<boolean>(GRID).fill(false))
  for (const [row, h] of half) {
    if (row < 1 || row > INNER || h < 0 || h >= HALF) continue
    filled[row]![HALF - h]! = true
    filled[row]![HALF + 1 + h]! = true
  }

  const gaze: 1 | -1 = stream(`${seed}/gaze`)() < 0.5 ? -1 : 1
  const leftEye = { x: HALF - outer + 1, y: eyeRow }
  const rightEye = { x: HALF + outer - 1, y: eyeRow }

  // The cells an eye may occupy in any pose: its own, the row over it and the row under it.
  const guarded = new Set<string>()
  for (const eye of [leftEye, rightEye])
    for (let dx = -1; dx <= 2; dx++) for (let dy = -1; dy <= 1; dy++) guarded.add(`${eye.x + dx},${eye.y + dy}`)

  const symmetry = lopsided(seed, filled, guarded)
  keepConnected(filled, leftEye)

  const paints = paintsFor(seed)
  const direction = pick(seed, 'direction', DIRECTIONS)
  const fringe = stream(`${seed}/fringe`)() < 0.35
  const traits: Traits = { body, crown, arms, legs, eyes, paints: paints.map((p) => p.name), direction, fringe, symmetry }

  const cells = paint(seed, filled, traits, walk(traits.paints), leftEye, rightEye)
  const xs = cells.map((cell) => cell.x)
  const ys = cells.map((cell) => cell.y)
  const spanX = Math.max(...xs) - Math.min(...xs) + 1
  const spanY = Math.max(...ys) - Math.min(...ys) + 1
  const size = Math.max(spanX, spanY)
  return {
    traits,
    cells,
    eyes: [leftEye, rightEye],
    gaze,
    view: { x: Math.min(...xs) - (size - spanX) / 2, y: Math.min(...ys) - (size - spanY) / 2, size },
    signature: ['pixel', ...traits.paints, direction, body, crown, arms, legs, eyes, symmetry].join('-'),
  }
}

/** About a third of bots break the mirror with one to three cells on one side. */
function lopsided(seed: string, filled: boolean[][], guarded: Set<string>): Traits['symmetry'] {
  const next = stream(`${seed}/lopsided`)
  if (next() >= 0.34) return 'mirror'
  const side = next() < 0.5 ? 'left' : 'right'
  const inSide = (x: number) => (side === 'left' ? x >= 1 && x <= HALF : x > HALF && x <= INNER)
  const at = (x: number, y: number) => filled[y]?.[x] === true
  const edits = 1 + Math.floor(next() * 3)
  for (let edit = 0; edit < edits; edit++) {
    const add = next() < 0.65
    const candidates: [number, number][] = []
    for (let y = 1; y <= INNER; y++)
      for (let x = 1; x <= INNER; x++) {
        if (!inSide(x) || guarded.has(`${x},${y}`)) continue
        const touching = [[-1, 0], [1, 0], [0, -1], [0, 1]].some(([dx, dy]) => at(x + dx!, y + dy!))
        if (add ? !at(x, y) && touching : at(x, y) && [[-1, 0], [1, 0], [0, -1], [0, 1]].some(([dx, dy]) => !at(x + dx!, y + dy!)))
          candidates.push([x, y])
      }
    const chosen = candidates[Math.floor(next() * candidates.length)]
    if (chosen) filled[chosen[1]]![chosen[0]]! = add
  }
  return 'lopsided'
}

/** Drop anything not joined to the body, corners counting as joined, as they do in the art. */
function keepConnected(filled: boolean[][], from: { x: number; y: number }): void {
  const seen = new Set<string>([`${from.x},${from.y}`])
  const queue = [from]
  while (queue.length > 0) {
    const { x, y } = queue.pop()!
    for (let dx = -1; dx <= 1; dx++)
      for (let dy = -1; dy <= 1; dy++) {
        const key = `${x + dx},${y + dy}`
        if (filled[y + dy]?.[x + dx] && !seen.has(key)) {
          seen.add(key)
          queue.push({ x: x + dx, y: y + dy })
        }
      }
  }
  for (let y = 0; y < GRID; y++) for (let x = 0; x < GRID; x++) if (!seen.has(`${x},${y}`)) filled[y]![x]! = false
}

/** Two or three paints, each at least `HUE_DISTANCE` from every other. */
function paintsFor(seed: string): { name: string; hex: string }[] {
  const next = stream(`${seed}/paints`)
  const chosen = [PAINT[Math.floor(next() * PAINT.length)]!]
  const count = next() < 0.4 ? 3 : 2
  while (chosen.length < count) {
    const allowed = PAINT.filter((p) => chosen.every((c) => hueDistance(c.hex, p.hex) >= HUE_DISTANCE))
    if (allowed.length === 0) break
    chosen.push(allowed[Math.floor(next() * allowed.length)]!)
  }
  return chosen
}

function paint(
  seed: string,
  filled: boolean[][],
  traits: Traits,
  stops: Lch[],
  leftEye: { x: number; y: number },
  rightEye: { x: number; y: number },
): Cell[] {
  const next = stream(`${seed}/tiles`)
  const flip = stream(`${seed}/flip`)() < 0.5
  let minX = GRID, maxX = 0, minY = GRID, maxY = 0
  for (let y = 0; y < GRID; y++)
    for (let x = 0; x < GRID; x++)
      if (filled[y]![x]) {
        minX = Math.min(minX, x); maxX = Math.max(maxX, x); minY = Math.min(minY, y); maxY = Math.max(maxY, y)
      }
  const span = (v: number, lo: number, hi: number) => (hi === lo ? 0.5 : (v - lo) / (hi - lo))
  const face = { x: (leftEye.x + rightEye.x + 1) / 2 + 0.5, y: leftEye.y + 0.5 }
  const reach = Math.max(...[[minX, minY], [maxX + 1, minY], [minX, maxY + 1], [maxX + 1, maxY + 1]].map(([x, y]) => Math.hypot(x! - face.x, y! - face.y)))

  const eyeCells = new Set<string>()
  for (const eye of [leftEye, rightEye]) for (let dx = 0; dx < 2; dx++) eyeCells.add(`${eye.x + dx},${eye.y}`)
  const besideEye = (x: number, y: number) =>
    [leftEye, rightEye].some((eye) => x >= eye.x - 1 && x <= eye.x + 2 && y >= eye.y - 2 && y <= eye.y + 2)

  const cells: Cell[] = []
  for (let y = 0; y < GRID; y++)
    for (let x = 0; x < GRID; x++) {
      if (!filled[y]![x]) continue
      const sx = span(x, minX, maxX)
      const sy = span(y, minY, maxY)
      let t =
        traits.direction === 'vertical' ? sy
        : traits.direction === 'horizontal' ? sx
        : traits.direction === 'diagonal' ? (sx + sy) / 2
        : Math.hypot(x + 0.5 - face.x, y + 0.5 - face.y) / reach
      if (flip) t = 1 - t
      t += (next() - 0.5) * 0.14
      const colour = along(stops, t)
      colour.l = Math.min(0.95, Math.max(0.3, colour.l + (next() - 0.5) * 0.06))
      const open = [[-1, 0], [1, 0], [0, -1], [0, 1]].some(([dx, dy]) => !filled[y + dy!]?.[x + dx!])
      if (traits.fringe && open && !besideEye(x, y)) {
        // Paler at the edge, but never so pale that the edge is lost against a white page.
        colour.l = Math.max(colour.l, Math.min(0.86, colour.l + (0.96 - colour.l) * 0.4))
        colour.c *= 0.7
      }
      let fill = hexOfLch(colour)
      if (besideEye(x, y) && !eyeCells.has(`${x},${y}`)) {
        for (let step = 0; step < 12 && contrast(EYE_WHITE, fill) < EYE_CONTRAST; step++) {
          colour.l -= 0.04
          fill = hexOfLch(colour)
        }
      }
      cells.push({ x, y, fill })
    }
  return cells
}
