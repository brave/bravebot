/**
 * A bot's face, as geometry.
 *
 * The brief was *friendly and approachable*, and almost every decision here follows from taking
 * that literally rather than decoratively:
 *
 * - **Round, and large-headed.** A head about as wide as the body, eyes low and wide-set, nothing
 *   with a point on it. These are the proportions people read as young and harmless — the reason
 *   every friendly robot ever drawn has them — and the reason none of the shapes below is a cone
 *   or a sharp-edged box.
 * - **Two big eyes, and nothing else on the face.** No mouth. A mouth has an expression whether or
 *   not one was intended, and a fixed one is either a grin that never fits what the bot just said
 *   or a line that reads as sullen. Eyes alone are warm and say nothing about mood, which is right
 *   for something that is going to sit beside a failed turn as often as a good one.
 * - **A highlight in each eye.** One small bright dot, placed the same way in both. It is the
 *   difference between eyes that look at you and eyes that are holes.
 * - **It looks slightly up.** The head is tipped back a degree or two. Something looking fractionally
 *   up at the reader is open; something looking down is either sad or judging.
 * - **A drawn edge.** Every painted piece has a thin dark line around it, the way a sticker or a
 *   cartoon does. This is not decoration: the pale pieces — a bobble, an ear, a collar — sit
 *   against the *page*, not against the head, and on a light theme a pale green bobble on an
 *   off-white column is invisible. Two of the six traits were being thrown away in light mode.
 *   The line also settles the figure against a dark column, where the deep body used to bleed into
 *   the background, and it separates an ear from the head it is pressed against. It is drawn as an
 *   inverted hull — a slightly bigger back-facing copy of each piece — which is the cheapest
 *   outline there is and the one that survives being 38 pixels wide.
 *
 * ## Colour
 *
 * One colour per bot, from a fixed set, picked by the seed — and the parts told apart by *shade* of
 * it rather than by a second hue. The head is the colour itself, the body is a deeper version, and
 * the small pieces on top are a paler one. So a bot is "the blue one" rather than "the blue and
 * yellow one", which is a thing somebody can hold in their head about eight bots at once.
 *
 * The shades are far enough apart to survive being 38 pixels wide, which is the only real
 * constraint on them: a body one step darker than its head reads as a shadow rather than as a
 * different part, and the figure goes back to being one blob.
 *
 * These were built in the window's accent to begin with, on the argument that every colour in this
 * palette means something and a bot picking a hue would say something it did not mean. That was
 * wrong in practice for a reason the argument could not see: the accent is a warm orange, and a
 * warm orange sphere with two eyes in it is not an abstract mark, it is a *face*, and the whole
 * thing read as skin. A figure this simple is read as a body before it is read as anything else, so
 * the colour has to say "this is a painted object" loudly enough to stop that — which is what a
 * saturated blue head with a yellow body does and no shade of orange can.
 *
 * So the set below is primaries and near-primaries: the colours of moulded plastic toys, chosen to
 * be told apart at a glance and to be nobody's complexion. There is no orange in it, and no brown,
 * for exactly that reason — and a single hue makes that stricter rather than looser, since there is
 * no second colour to carry the "this is painted" signal if the first one fails to.
 *
 * The cost, stated plainly: these no longer follow a theme. A palette somebody writes repaints the
 * window and leaves the bots as they are. That is the right trade for a face — a face that changed
 * colour with the furniture would be a worse identity than one that does not — but it is a trade
 * rather than a free win.
 *
 * ## What else differs between bots
 *
 * The *form*: the shape of the head, what is on top of it, whether it has ears, the set of the
 * eyes, and the body under it. Six choices from small sets, times the colour, which is far more
 * combinations than anybody will have bots and — more to the point — combinations that are told
 * apart at a glance.
 *
 * Everything is a function of the seed, so a bot has one face for its whole life, and the same face
 * in every window.
 */

import * as THREE from 'three'
import { AVATAR_VERSION } from '../../shared/avatar'

export interface Figure {
  root: THREE.Group
  /** The head and everything on it, so a nod or a look down moves the face and not the body. */
  head: THREE.Group
  /**
   * Whatever is on top of the head, pivoted at the crown of the skull, or `null` for a bare one.
   * Its own group so it can lag the head by a frame — a bobble that swings and an aerial that
   * wobbles when the head turns, which is the cheapest thing that makes a figure read as a cartoon
   * rather than a model.
   */
  crown: THREE.Group | null
  /** The eyes, together, so a blink can squash both at once. */
  eyes: THREE.Group
  /**
   * The catchlights, apart from the eyes they sit in. A reflection does not squash when a lid
   * closes over it, it goes away — so a blink hides these rather than scaling them.
   */
  glints: THREE.Group
  dispose: () => void
}

/** A stream of numbers from a seed. xorshift32 over an FNV-1a hash — see `stage.ts` for the same. */
function stream(seed: string): () => number {
  let state = 0x811c9dc5
  for (let at = 0; at < seed.length; at++) {
    state ^= seed.charCodeAt(at)
    state = Math.imul(state, 0x01000193)
  }
  state = state >>> 0 || 1
  return () => {
    state ^= state << 13
    state ^= state >>> 17
    state ^= state << 5
    return (state >>> 0) / 0x100000000
  }
}

/**
 * The colours a bot can be painted in.
 *
 * Primaries and near-primaries — the colours of moulded plastic rather than of anything alive. No
 * orange and no brown: those are the two that turn a rounded head with eyes into a face, which is
 * the thing these are deliberately not.
 *
 * Muted rather than pure. Each is its primary at roughly two thirds of full saturation, pulled a
 * little toward mid lightness — the difference between a poster and a painted wooden toy. Fully
 * saturated versions were the first thing tried and they shouted: eight of them down a column, each
 * one small and each one at maximum chroma, is a lot of noise beside a list of quiet grey text, and
 * the avatar started competing with the name it belongs to. What is kept is the *hue*, which is the
 * part doing the identifying — a bot is still recognisably the blue one or the green one.
 *
 * `yellow` and `lime` are the pair that needs watching. Muting moves everything toward grey, and
 * two hues 40° apart both heading for the same grey converge — so `lime` is pushed greener and
 * darker than a straight muting would give it, to keep the two apart at 38 pixels.
 *
 * Named, because the name is what a test reads: `data-avatar` says `blue-round-…`, and a driver
 * asserting that two bots differ should be able to say how.
 */
const PAINT: { name: string; hex: string }[] = [
  { name: 'red', hex: '#c26058' },
  { name: 'blue', hex: '#5a80c0' },
  { name: 'yellow', hex: '#d4b45c' },
  { name: 'green', hex: '#57a06d' },
  { name: 'violet', hex: '#8a6fc4' },
  { name: 'cyan', hex: '#4aa7b5' },
  { name: 'pink', hex: '#c66a99' },
  { name: 'lime', hex: '#8fae55' },
]

/** The traits a seed picks. Named, because they are also what the window reports for a driver. */
export interface Traits {
  version: 1 | 2
  eyeShape: 'round' | 'oval' | 'square' | 'pill'
  faceShape: 'bare' | 'oval' | 'panel'
  earShape: 'disc' | 'fin' | 'pod'
  proportion: 'balanced' | 'forehead' | 'tapered' | 'compact'
  torso: 'classic' | 'compact' | 'broad' | 'neck'
  head: 'round' | 'squat' | 'tall' | 'boxy'
  crown: 'none' | 'antenna' | 'bobble' | 'tuft'
  ears: boolean
  eyes: 'wide' | 'close' | 'big'
  body: 'dome' | 'barrel' | 'pill'
  collar: boolean
  /** The one colour the whole figure is painted in, by name. Its parts differ by shade. */
  paint: string
}

const HEADS: Traits['head'][] = ['round', 'squat', 'tall', 'boxy']
const CROWNS: Traits['crown'][] = ['none', 'antenna', 'bobble', 'tuft']
const EYES: Traits['eyes'][] = ['wide', 'close', 'big']
const BODIES: Traits['body'][] = ['dome', 'barrel', 'pill']

/** Which figure a seed describes. Exported so the window can say, without drawing anything. */
export function traitsOf(seed: string): Traits {
  const next = stream(seed)
  const pick = <T,>(from: T[]): T => from[Math.floor(next() * from.length)] as T
  const head = pick(HEADS)
  const crown = pick(CROWNS)
  const ears = next() < 0.45
  const eyes = pick(EYES)
  const body = pick(BODIES)
  const collar = next() < 0.5
  const paint = pick(PAINT)
  // Each added trait has its own stream. Appending future traits cannot reshuffle these.
  const variant = seed.startsWith(AVATAR_VERSION)
  const choose = <T,>(key: string, values: readonly T[]): T =>
    values[Math.floor(stream(`${seed}/${key}`)() * values.length)]!
  return {
    head, crown, ears, eyes, body, collar, paint: paint.name,
    version: variant ? 2 : 1,
    eyeShape: variant ? choose('eye-shape', ['round', 'oval', 'square', 'pill'] as const) : 'round',
    faceShape: variant ? choose('face-shape', ['bare', 'oval', 'panel'] as const) : 'bare',
    earShape: variant ? choose('ear-shape', ['disc', 'fin', 'pod'] as const) : 'disc',
    proportion: variant ? choose('proportion', ['balanced', 'forehead', 'tapered', 'compact'] as const) : 'balanced',
    torso: variant ? choose('torso', ['compact', 'broad', 'neck'] as const) : 'classic',
  }
}

/** A short stable string naming the figure, for a test that must not compare pixels. */
export function signature(seed: string): string {
  const t = traitsOf(seed)
  return [
    t.paint,
    t.head,
    t.crown,
    t.ears ? 'ears' : 'plain',
    t.eyes,
    t.body,
    t.collar ? 'collar' : 'bare',
    ...(t.version === 2 ? [t.eyeShape, t.faceShape, t.earShape, t.proportion, t.torso, 'v2'] : []),
  ].join('-')
}

/** Width adjustment at a normalized height, shared by the model and flat silhouette. */
export function headWidth(proportion: Traits['proportion'], y: number): number {
  if (proportion === 'forehead') return 1 + Math.max(0, y) * 0.16
  if (proportion === 'tapered') return 1 - Math.max(0, -y) * 0.20
  return proportion === 'compact' ? 1.04 : 1
}

export function eyeDimensions(shape: Traits['eyeShape']): { x: number; y: number } {
  if (shape === 'oval') return { x: 0.8, y: 1.2 }
  if (shape === 'pill') return { x: 1.2, y: 0.7 }
  return { x: 1, y: 1 }
}

export function torsoDimensions(torso: Traits['torso']) {
  if (torso === 'broad') return { width: 1.13, height: 0.86, y: -1.4 }
  if (torso === 'neck') return { width: 0.84, height: 0.8, y: -1.62 }
  if (torso === 'compact') return { width: 0.84, height: 0.9, y: -1.34 }
  return { width: 1, height: 1, y: -1.35 }
}

/** A named paint, or the first one if the name is not one this build has. */
function paintOf(name: string): THREE.Color {
  const found = PAINT.find((paint) => paint.name === name) ?? PAINT[0]
  return new THREE.Color(found!.hex)
}

/**
 * The three shades a figure is painted in.
 *
 * Deep is a straight darkening; pale is mixed toward white rather than lightened, which keeps a
 * saturated colour from simply clipping to its own hue at full brightness and staying the same
 * shade. The gaps are wide on purpose — see the note about 38 pixels above.
 */
function shades(name: string): { base: THREE.Color; deep: THREE.Color; pale: THREE.Color } {
  const base = paintOf(name)
  return {
    base,
    deep: base.clone().multiplyScalar(0.58),
    pale: base.clone().lerp(new THREE.Color('#ffffff'), 0.42),
  }
}

/**
 * The shades a seed paints a bot in, as hex, for anything that is not three.js — plus `edge`, the
 * colour of the drawn line around it, which is the same darkening of `deep` the figure uses.
 */
export function paintsOf(seed: string): { base: string; deep: string; pale: string; edge: string; face: string } {
  const { base, deep, pale } = shades(traitsOf(seed).paint)
  const hex = (colour: THREE.Color): string => `#${colour.getHexString()}`
  return { base: hex(base), deep: hex(deep), pale: hex(pale), face: hex(base.clone().lerp(new THREE.Color('#ffffff'), 0.24)), edge: hex(deep.clone().multiplyScalar(0.5)) }
}

/**
 * The materials a figure is made of.
 *
 * Lambert rather than a physical material: these are 40 pixels across and lit by hand, so roughness
 * and metalness are cost with nothing to show for it, and a matte surface is friendlier than a
 * shiny one — a gloss highlight reads as hard plastic or metal, and these are meant to read as
 * painted wood.
 */
function palette(traits: Traits): {
  shell: THREE.Material
  trim: THREE.Material
  bright: THREE.Material
  spark: THREE.Material
  glint: THREE.Material
  edge: THREE.Material
} {
  const { base, deep, pale } = shades(traits.paint)
  // A little of the colour emitted as well as reflected, so the hue stays itself where the light
  // falls away. Without it a saturated blue goes to near-black on its shadowed side and the figure
  // reads as two different objects rather than as one painted one — which matters more now that
  // being one object in one colour is the whole idea.
  const glow = (colour: THREE.Color): THREE.Color => colour.clone().multiplyScalar(0.16)
  return {
    // The head: the colour itself, which is what somebody will remember the bot as.
    shell: new THREE.MeshLambertMaterial({ color: base, emissive: glow(base), vertexColors: true }),
    // The body, deeper — it sits below and behind, so a darker shade is also what the light would
    // have done to it, and the figure reads as lit rather than as two-tone.
    trim: new THREE.MeshLambertMaterial({ color: deep, emissive: glow(deep), vertexColors: true }),
    // The small pieces — an ear, a bobble, a collar — in the palest shade. Anything nearer the
    // body's own would vanish into it at this size, which is the one failure mode of a
    // single-colour figure.
    bright: new THREE.MeshLambertMaterial({ color: pale, emissive: glow(pale) }),
    // The eyes, and the one thing that is not painted: a very dark neutral, so they read as eyes
    // against any of the colours above. Basic, so they stay flat and dark wherever the light falls
    // — a lit sphere here would catch the key light and go grey.
    spark: new THREE.MeshBasicMaterial({ color: '#20222b' }),
    // The catchlight in each eye. White rather than a tint of the paint, and unlit like the eye it
    // sits in: it is standing in for a reflection of the room, which is not the colour of the bot.
    glint: new THREE.MeshBasicMaterial({ color: '#ffffff' }),
    // The drawn edge. A very dark version of the bot's own colour rather than black: a neutral
    // black line around a coloured toy reads as a hole cut in the page, and one in the hue reads as
    // the same object seen against the light. Back faces only — see `outline` below.
    edge: new THREE.MeshBasicMaterial({
      color: deep.clone().multiplyScalar(0.5),
      side: THREE.BackSide,
    }),
  }
}

/**
 * How far the edge stands off the surface, in figure units (the head is radius 1). A constant
 * rather than a scale, so an ear gets the same width of line as the head does — a hull that is 5%
 * bigger than its piece draws a bold line around a head and a hair around a bobble.
 */
const EDGE = 0.045

/**
 * The drawn edge around a piece: a copy of its geometry pushed out along its normals, wearing the
 * back-facing material. Where the real piece is in front, its front faces are nearer than the
 * hull's back faces and cover them; the hull shows only past the piece's own silhouette, which is
 * exactly where a drawn line goes. Added as a child, so it turns, tips and blinks with the piece.
 */
function outline(mesh: THREE.Mesh, material: THREE.Material): THREE.BufferGeometry {
  const geometry = mesh.geometry.clone()
  const position = geometry.attributes.position as THREE.BufferAttribute | undefined
  const normal = geometry.attributes.normal as THREE.BufferAttribute | undefined
  if (position && normal) {
    for (let i = 0; i < position.count; i++) {
      position.setXYZ(
        i,
        position.getX(i) + normal.getX(i) * EDGE,
        position.getY(i) + normal.getY(i) * EDGE,
        position.getZ(i) + normal.getZ(i) * EDGE,
      )
    }
  }
  mesh.add(new THREE.Mesh(geometry, material))
  return geometry
}

/** The head, by trait. Spheres scaled rather than four different geometries. */
function headMesh(traits: Traits, material: THREE.Material): THREE.Mesh {
  if (traits.head === 'boxy') {
    // The one flat-sided head, and still round: a box with a radius on every edge. A hard cube
    // among the spheres would be the one unfriendly face in the set.
    //
    // Pushed further toward the box than it first was. At 0.62 the corners were so soft that, at
    // 38 pixels, a boxy head and a round one were the same head — a trait that cannot be seen is
    // not a trait. Wider than it is tall as well, which is the other half of what makes a shape
    // read as a box rather than a ball.
    const geometry = new THREE.SphereGeometry(1, 24, 20)
    const mesh = new THREE.Mesh(geometry, material)
    mesh.scale.set(1.08, 0.9, 0.9)
    // Flattening the sphere's sides towards a rounded box, by hand, so there is still no seam.
    // `position` is optional on the attribute map's type; a sphere always has one, and a head
    // drawn as a plain sphere is a fine head, so the absence is skipped rather than asserted.
    const position = geometry.attributes.position as THREE.BufferAttribute | undefined
    if (!position) return mesh
    for (let i = 0; i < position.count; i++) {
      const x = position.getX(i)
      const y = position.getY(i)
      const z = position.getZ(i)
      const soften = (v: number) => Math.sign(v) * Math.pow(Math.abs(v), 0.5)
      position.setXYZ(i, soften(x), soften(y), soften(z))
    }
    geometry.computeVertexNormals()
    return mesh
  }

  const mesh = new THREE.Mesh(new THREE.SphereGeometry(1, 28, 22), material)
  if (traits.head === 'squat') mesh.scale.set(1.12, 0.86, 1)
  else if (traits.head === 'tall') mesh.scale.set(0.9, 1.12, 0.94)
  else mesh.scale.set(1, 0.98, 0.98)
  return mesh
}

/** The body, by trait. Under the head and mostly hidden by it, so it is a silhouette job. */
function bodyMesh(traits: Traits, material: THREE.Material): THREE.Mesh {
  if (traits.body === 'barrel') {
    return new THREE.Mesh(new THREE.CylinderGeometry(0.74, 0.86, 1.0, 24, 1, false), material)
  }
  if (traits.body === 'pill') {
    return new THREE.Mesh(new THREE.CapsuleGeometry(0.66, 0.5, 8, 20), material)
  }
  const mesh = new THREE.Mesh(new THREE.SphereGeometry(0.92, 24, 18), material)
  mesh.scale.set(1, 0.78, 0.94)
  return mesh
}

/** Sculpt only the versioned heads; legacy geometry remains unchanged. */
function sculptHead(mesh: THREE.Mesh, traits: Traits): THREE.Mesh {
  if (traits.version === 1) return mesh
  const positions = mesh.geometry.getAttribute('position')
  for (let i = 0; i < positions.count; i++) {
    const y = positions.getY(i)
    positions.setX(i, positions.getX(i) * headWidth(traits.proportion, y))
    if (traits.proportion === 'compact') positions.setY(i, y * 0.88)
  }
  mesh.geometry.computeVertexNormals()
  return mesh
}

/** Sample the finished shell in head-local coordinates, without its animated parent transform. */
function surfaceOf(shell: THREE.Mesh): (x: number, y: number) => number {
  const sample = new THREE.Mesh(shell.geometry, shell.material)
  sample.scale.copy(shell.scale)
  sample.updateMatrixWorld(true)
  const ray = new THREE.Raycaster()
  return (x, y) => {
    ray.set(new THREE.Vector3(x, y, 3), new THREE.Vector3(0, 0, -1))
    return ray.intersectObject(sample, false)[0]?.point.z ?? 0
  }
}

/** A face inset follows the head surface so its edges never float beside a curved cheek. */
function facePanel(traits: Traits, surface: (x: number, y: number) => number): THREE.BufferGeometry {
  const positions: number[] = []
  const indices: number[] = []
  const rings = 8, segments = 48
  for (let ring = 0; ring <= rings; ring++) {
    for (let segment = 0; segment <= segments; segment++) {
      const angle = segment / segments * Math.PI * 2
      const curve = (v: number) => traits.faceShape === 'panel' ? Math.sign(v) * Math.sqrt(Math.abs(v)) : v
      const x = curve(Math.cos(angle)) * 0.74 * ring / rings
      const y = curve(Math.sin(angle)) * 0.40 * ring / rings - 0.12
      positions.push(x, y, surface(x, y) + 0.028)
      if (ring < rings && segment < segments) {
        const at = ring * (segments + 1) + segment
        indices.push(at, at + segments + 1, at + 1, at + 1, at + segments + 1, at + segments + 2)
      }
    }
  }
  const geometry = new THREE.BufferGeometry()
  geometry.setAttribute('position', new THREE.Float32BufferAttribute(positions, 3))
  geometry.setIndex(indices)
  geometry.computeVertexNormals()
  return geometry
}

/** Soft painted occlusion: a little depth at the chin and where the head meets the shoulders. */
function depth(mesh: THREE.Mesh, part: 'head' | 'body'): THREE.Mesh {
  const positions = mesh.geometry.getAttribute('position')
  const colours = new Float32Array(positions.count * 3)
  for (let i = 0; i < positions.count; i++) {
    const y = positions.getY(i)
    const x = positions.getX(i)
    const shadow = part === 'head'
      ? Math.max(0, Math.min(1, (-y - 0.25) / 0.75)) * 0.14
      : Math.exp(-(((y - 0.4) / 0.3) ** 2)) * Math.exp(-((x / 0.7) ** 2)) * 0.22
    colours.fill(1 - shadow, i * 3, i * 3 + 3)
  }
  mesh.geometry.setAttribute('color', new THREE.BufferAttribute(colours, 3))
  return mesh
}

export function buildFigure(seed: string, size = 38): Figure {
  return buildFigureFromTraits(traitsOf(seed), size)
}

/** Build a resolved design; also used by the comparison gallery and geometry checks. */
export function buildFigureFromTraits(traits: Traits, size = 38): Figure {
  const materials = palette(traits)
  const owned: (THREE.BufferGeometry | THREE.Material)[] = Object.values(materials)

  const root = new THREE.Group()
  // Every painted piece gets its edge here, as it is kept. The eyes do not go through this: a
  // pupil is already the darkest thing on the face, and a line around a catchlight would be a line
  // around a reflection.
  const keep = <T extends THREE.Mesh>(mesh: T): T => {
    owned.push(mesh.geometry)
    owned.push(outline(mesh, materials.edge))
    return mesh
  }

  // --- body -------------------------------------------------------------------------------
  const body = keep(depth(bodyMesh(traits, materials.trim), 'body'))
  const torso = torsoDimensions(traits.torso)
  body.scale.multiply(new THREE.Vector3(torso.width, torso.height, 1))
  body.position.y = torso.y
  if (traits.version === 2) {
    // A continuous join for every head/body pair, including short heads over broad shoulders.
    const neck = keep(new THREE.Mesh(new THREE.CylinderGeometry(0.30, 0.36, 0.9, 20), traits.torso === 'neck' ? materials.bright : materials.trim))
    neck.position.y = -1.0
    root.add(neck)
  }
  root.add(body)

  if (traits.collar) {
    const collar = keep(new THREE.Mesh(new THREE.TorusGeometry(0.6, 0.1, 10, 26), materials.bright))
    collar.rotation.x = Math.PI / 2
    collar.position.y = -0.92
    root.add(collar)
  }

  // --- head -------------------------------------------------------------------------------
  // Its own group, tipped back a little, so everything on it tips with it — see the note above
  // about looking fractionally up.
  const head = new THREE.Group()
  head.position.y = 0.12
  head.rotation.x = -0.07
  root.add(head)
  const shell = keep(depth(sculptHead(headMesh(traits, materials.shell), traits), 'head'))
  shell.name = 'shell'
  const surface = surfaceOf(shell)
  head.add(shell)
  if (traits.faceShape !== 'bare') {
    const geometry = facePanel(traits, surface)
    const material = new THREE.MeshLambertMaterial({ color: paintOf(traits.paint).lerp(new THREE.Color('#ffffff'), 0.24) })
    owned.push(geometry, material)
    const panel = new THREE.Mesh(geometry, material)
    panel.name = 'face-panel'
    head.add(panel)
  }

  if (traits.ears) {
    for (const side of [-1, 1]) {
      const ear = keep(new THREE.Mesh(new THREE.SphereGeometry(0.28, 16, 12), materials.bright))
      const width = (traits.head === 'tall' ? 0.91 : traits.head === 'squat' ? 1.1 : 1.04) * headWidth(traits.proportion, -0.05)
      ear.position.set(side * width, -0.05, 0)
      if (traits.earShape === 'fin') {
        ear.scale.set(0.5, 1.6, 0.65)
        ear.rotation.z = side * -0.24
      } else if (traits.earShape === 'pod') {
        ear.scale.set(0.85, 0.85, 1)
        ear.position.x += side * 0.035
      } else ear.scale.set(0.6, 1, 0.85)
      head.add(ear)
    }
  }

  // The crown pivots where it meets the skull, so a swing rotates it about its base the way a
  // thing stuck on top of a head would swing. Positions below are relative to that point.
  const CROWN_AT = (traits.head === 'tall' ? 1.12 : traits.head === 'squat' ? 0.86 : traits.head === 'boxy' ? 0.9 : 0.98) * (traits.proportion === 'compact' ? 0.88 : 1)
  let crown: THREE.Group | null = null
  if (traits.crown !== 'none') {
    crown = new THREE.Group()
    crown.position.y = CROWN_AT
    head.add(crown)
  }

  if (crown && traits.crown === 'antenna') {
    // Short. A tall one is the tallest thing in the set and would decide how far back the camera
    // has to sit for every other figure — one bot's aerial costing every other bot's face the
    // room it needed.
    //
    // Stalk and tip in the one shade. They were deep and pale respectively, which meant that on a
    // dark column the stalk vanished and the tip floated, and on a light one the reverse — an
    // aerial is one object, and it should read as one on either background.
    const stalk = keep(new THREE.Mesh(new THREE.CylinderGeometry(0.065, 0.065, 0.3, 8), materials.bright))
    stalk.position.y = 0.12
    crown.add(stalk)
    const tip = keep(new THREE.Mesh(new THREE.SphereGeometry(0.15, 14, 12), materials.bright))
    tip.position.y = 0.32
    crown.add(tip)
  } else if (crown && traits.crown === 'bobble') {
    const bobble = keep(new THREE.Mesh(new THREE.SphereGeometry(0.3, 18, 14), materials.bright))
    bobble.position.y = 0.08
    bobble.scale.set(1, 0.82, 1)
    crown.add(bobble)
  } else if (crown && traits.crown === 'tuft') {
    // Three small spheres in a row, which at this size reads as a tuft of hair rather than as
    // three spheres — and is the friendliest thing on offer to put on top of a head.
    for (const [index, side] of [-1, 0, 1].entries()) {
      const puff = keep(new THREE.Mesh(new THREE.SphereGeometry(0.23, 14, 12), materials.bright))
      puff.position.set(side * 0.28, index === 1 ? 0.19 : 0.04, 0)
      crown.add(puff)
    }
  }

  // --- eyes -------------------------------------------------------------------------------
  // Low on the face, which is the single strongest cue for "young, and therefore harmless". Eyes
  // set at the middle of a head read as an adult; set below it, as a child.
  const spread = traits.eyes === 'wide' ? 0.44 : traits.eyes === 'close' ? 0.29 : 0.37
  const eyeSize = (traits.eyes === 'big' ? 0.23 : 0.18) * (size <= 30 ? 1.16 : 1)

  // Not through `keep`: no edge on an eye. The geometries are owned by hand below.
  const eyes = new THREE.Group()
  eyes.position.set(0, -0.12, 0)
  head.add(eyes)
  // The catchlights, in a group of their own at the same place, so a blink can hide them without
  // squashing them. Same parent, same position, so they follow the head exactly as the eyes do.
  const glints = new THREE.Group()
  glints.position.copy(eyes.position)
  head.add(glints)

  for (const side of [-1, 1]) {
    const eye = new THREE.Mesh(new THREE.SphereGeometry(eyeSize, 18, 14), materials.spark)
    owned.push(eye.geometry)
    const dimensions = eyeDimensions(traits.eyeShape)
    const z = traits.version === 1 ? 0.86 : surface(side * spread, -0.12) + 0.04
    if (traits.eyeShape === 'square') {
      const positions = eye.geometry.getAttribute('position')
      for (let i = 0; i < positions.count; i++) {
        const soften = (v: number) => Math.sign(v) * Math.pow(Math.abs(v / eyeSize), 0.5) * eyeSize
        positions.setX(i, soften(positions.getX(i)))
        positions.setY(i, soften(positions.getY(i)))
      }
      eye.geometry.computeVertexNormals()
    }
    eye.position.set(side * spread, 0, z)
    eye.scale.set(dimensions.x, dimensions.y, 0.55)
    eye.name = 'eye'
    eyes.add(eye)

    // The highlight, up and toward the outside on both — the same place on each, because two
    // highlights in different places make a face look cross-eyed. Larger than the smallest thing
    // that reads as a highlight at 128 pixels, because it is shown at 38: at a third of the pupil
    // it was one device pixel and came and went with the turn.
    const glint = new THREE.Mesh(new THREE.SphereGeometry(eyeSize * 0.42, 10, 8), materials.glint)
    owned.push(glint.geometry)
    glint.position.set(side * spread + eyeSize * dimensions.x * 0.28, eyeSize * dimensions.y * 0.28, z + eyeSize * 0.46)
    if (traits.version === 1) glint.position.set(side * spread + eyeSize * 0.34, eyeSize * 0.36, 0.86 + eyeSize * 0.4)
    glints.add(glint)
  }

  // No scaling or nudging here: the camera above decides the crop, and a figure that also moved
  // itself would mean two places deciding the same thing and neither of them alone.


  return {
    root,
    head,
    crown,
    eyes,
    glints,
    dispose: () => {
      for (const thing of owned) thing.dispose()
    },
  }
}
