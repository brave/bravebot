// The two pictures in the README, drawn from the pixel figures the desktop app gives each bot.
//
// A bot's face is a function of its seed (src/renderer/avatar/pixels.ts), so the bots here are
// faces real bots have, not a redrawing of them. `node scripts/readme-art.mjs` writes
// docs/assets/readme-banner.svg and docs/assets/readme-how-it-works.svg.
//
// GitHub serves a repository's SVG as an image, so each file is self-contained: no script, no
// external reference, no web font and no foreignObject. The motion is CSS, and it stops under
// prefers-reduced-motion with every bot's eyes open.
import { mkdirSync, realpathSync, writeFileSync } from 'node:fs'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { buildSprite, placeEyes, stream, EYE_PUPIL, EYE_WHITE, LID } from '../src/renderer/avatar/pixels.ts'

export const CELL = 8

const SANS = "-apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica, Arial, sans-serif"
const MONO = "ui-monospace, SFMono-Regular, Menlo, Consolas, 'Liberation Mono', monospace"

const INK = '#eceaf8'
const MUTED = '#a6abd0'
const ORANGE = '#ff5601'
const DANGER = '#ff5d73'

/** Seeds of the bots in the banner, left to right, with the middle one the warmest. */
export const CAST = [30, 11, 4, 5, 42, 27, 25, 3, 1].map((n) => `readme-${n}`)

/** The letters of the wordmark, five cells wide and seven tall. */
export const GLYPHS = {
  b: ['#....', '#....', '####.', '#...#', '#...#', '#...#', '####.'],
  r: ['.....', '.....', '#.##.', '##..#', '#....', '#....', '#....'],
  a: ['.....', '.....', '.###.', '....#', '.####', '#...#', '.####'],
  v: ['.....', '.....', '#...#', '#...#', '#...#', '.#.#.', '..#..'],
  e: ['.....', '.....', '.###.', '#...#', '#####', '#....', '.####'],
  o: ['.....', '.....', '.###.', '#...#', '#...#', '#...#', '.###.'],
  t: ['.#...', '.#...', '####.', '.#...', '.#...', '.#..#', '..##.'],
}

/** One colour per row of a letter, from the top of an ascender down to the baseline. */
const WORDMARK_ROWS = ['#ffd24a', '#ffb02e', '#ff8a14', '#ff6a06', '#ff5601', '#ff3a01', '#ff1f01']
const WORDMARK_SHADOW = '#2d0d1b'

const CSS = `
.march{animation:march 4s steps(1,end) infinite}
@keyframes march{0%{transform:translate(-24px,0)}25%{transform:translate(0,0)}50%{transform:translate(24px,0)}75%{transform:translate(0,0)}}
.nod{animation:nod 2s steps(1,end) infinite;animation-delay:var(--n,0s)}
@keyframes nod{0%{transform:translate(0,0)}50%{transform:translate(0,var(--c))}}
.open{animation:open var(--t,6s) steps(1,end) infinite;animation-delay:var(--d,0s)}
@keyframes open{0%{opacity:1}94%{opacity:0}97%{opacity:1}}
.lid{opacity:0;animation:lid var(--t,6s) steps(1,end) infinite;animation-delay:var(--d,0s)}
@keyframes lid{0%{opacity:0}94%{opacity:1}97%{opacity:0}}
.w{animation:glance-w 9s steps(1,end) infinite;animation-delay:var(--g,0s)}
.p{animation:glance-p 9s steps(1,end) infinite;animation-delay:var(--g,0s)}
@keyframes glance-w{0%{transform:translate(0,0)}62%{transform:translate(calc(var(--s) * 1px),0)}74%{transform:translate(0,0)}}
@keyframes glance-p{0%{transform:translate(0,0)}62%{transform:translate(calc(var(--s) * -1px),0)}74%{transform:translate(0,0)}}
.tw{animation:tw 3s steps(1,end) infinite;animation-delay:var(--d,0s)}
@keyframes tw{0%{opacity:.25}50%{opacity:.9}}
@media (prefers-reduced-motion:reduce){.march,.nod,.open,.lid,.w,.p,.tw{animation:none}}
`.trim()

const escape = (text) => text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')

function svg({ width, height, title, description, body }) {
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" role="img" aria-labelledby="t d" shape-rendering="crispEdges">
<title id="t">${escape(title)}</title>
<desc id="d">${escape(description)}</desc>
<style>${CSS}</style>
<defs>
<linearGradient id="sky" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#0d0e1c"/><stop offset="1" stop-color="#1b1233"/></linearGradient>
<radialGradient id="glow" cx="50%" cy="50%" r="50%"><stop offset="0" stop-color="${ORANGE}" stop-opacity=".26"/><stop offset="1" stop-color="${ORANGE}" stop-opacity="0"/></radialGradient>
<pattern id="hatch" width="12" height="12" patternUnits="userSpaceOnUse" patternTransform="rotate(45)"><rect width="12" height="12" fill="#2a1424"/><rect width="5" height="12" fill="${DANGER}" fill-opacity=".38"/></pattern>
</defs>
<rect width="${width}" height="${height}" rx="24" fill="url(#sky)"/>
${body}
<rect x="1" y="1" width="${width - 2}" height="${height - 2}" rx="23" fill="none" stroke="#ffffff" stroke-opacity=".09" stroke-width="2"/>
</svg>
`
}

/**
 * One bot as a group of rects, standing on `baseline` and centred on `centre`. `gaze` overrides
 * which way the bot looks, and `n`, `d`, `t` and `g` are the delays and period of its nod, blink
 * and glance, so a row of bots does not move in step.
 */
export function bot(seed, { centre, baseline, cell = CELL, gaze, n = 0, d = 0, t = 6, g = 0, nods = true }) {
  const sprite = buildSprite(seed)
  const looking = gaze ?? sprite.gaze
  const xs = sprite.cells.map((c) => c.x)
  const ys = sprite.cells.map((c) => c.y)
  const left = Math.min(...xs)
  const across = Math.max(...xs) + 1 - left
  const x = Math.round(centre - (left + across / 2) * cell)
  const y = baseline - (Math.max(...ys) + 1) * cell
  const body = sprite.cells.map((c) => `<rect x="${c.x}" y="${c.y}" width="1" height="1" fill="${c.fill}"/>`).join('')
  const eyes = placeEyes(sprite, { gaze: looking, drop: 0, blink: false, nod: false })
    .map((eye, i) => {
      const lidX = Math.min(eye.whiteX, eye.pupilX)
      const lidY = sprite.eyes[i].y + LID.offset
      return (
        `<g class="open"><rect class="w" x="${eye.whiteX}" y="${eye.y}" width="1" height="1" fill="${EYE_WHITE}"/>` +
        `<rect class="p" x="${eye.pupilX}" y="${eye.y}" width="1" height="1" fill="${EYE_PUPIL}"/></g>` +
        `<rect class="lid" x="${lidX}" y="${lidY}" width="2" height="${LID.height}" fill="${EYE_PUPIL}"/>`
      )
    })
    .join('')
  const vars = `--c:${cell}px;--s:${looking};--n:-${n}s;--d:-${d}s;--t:${t}s;--g:-${g}s`
  return `<g class="${nods ? 'nod' : ''}" style="${vars}"><g transform="translate(${x} ${y}) scale(${cell})">${body}${eyes}</g></g>`
}

/** The cells of `text` in the wordmark face, as one rect per horizontal run, banded by row. */
export function wordmark(text, { x, y, cell, fill = (row) => WORDMARK_ROWS[row] }) {
  const width = text.length * 6 - 1
  const rects = []
  for (let row = 0; row < 7; row++) {
    let at = 0
    for (const letter of text) {
      const cells = GLYPHS[letter][row]
      for (const run of cells.matchAll(/#+/g)) {
        rects.push(`<rect x="${x + (at + run.index) * cell}" y="${y + row * cell}" width="${run[0].length * cell}" height="${cell}" fill="${fill(row)}"/>`)
      }
      at += 6
    }
  }
  return { rects: rects.join(''), width: width * cell }
}

function stars(count, keepOut, width, height) {
  const next = stream('readme-banner-stars')
  const out = []
  while (out.length < count) {
    const x = Math.floor((next() * width) / CELL) * CELL
    const y = Math.floor((next() * height) / CELL) * CELL
    const size = next() < 0.3 ? CELL : CELL / 2
    const opacity = (0.18 + next() * 0.4).toFixed(2)
    const twinkles = next() < 0.2
    const delay = (next() * 3).toFixed(1)
    if (keepOut.some((box) => x + size > box.x && x < box.x + box.width && y + size > box.y && y < box.y + box.height)) continue
    out.push(`<rect${twinkles ? ` class="tw" style="--d:-${delay}s"` : ''} x="${x}" y="${y}" width="${size}" height="${size}" fill="#ffffff" fill-opacity="${opacity}"/>`)
  }
  return out.join('')
}

export const TAGLINE = 'A general-purpose agent that a web page cannot redirect.'

export function banner() {
  const width = 1280
  const height = 448
  const botCell = 9
  const pitch = 10 * botCell + 44
  const bots = CAST.map((seed, i) => {
    const centre = width / 2 + (i - (CAST.length - 1) / 2) * pitch
    return bot(seed, { centre, baseline: 150, cell: botCell, n: i % 2, d: (i * 1.7) % 6, t: 5 + (i % 3), g: (i * 2.3) % 9 })
  }).join('')

  const cell = 18
  const letters = wordmark('bravebot', { x: 0, y: 0, cell })
  const wordX = (width - letters.width) / 2
  const wordY = 186
  const shadow = wordmark('bravebot', { x: wordX + cell, y: wordY + cell, cell, fill: () => WORDMARK_SHADOW })
  const face = wordmark('bravebot', { x: wordX, y: wordY, cell })

  const keepOut = [
    { x: wordX - 16, y: wordY - 16, width: letters.width + 48, height: 7 * cell + 48 },
    { x: 140, y: 340, width: width - 280, height: 64 },
  ]
  const body = `${stars(90, [...keepOut, { x: 40, y: 36, width: width - 80, height: 124 }], width, height)}
<ellipse cx="${width / 2}" cy="${wordY + 56}" rx="520" ry="190" fill="url(#glow)"/>
<g class="march">${bots}</g>
${shadow.rects}
${face.rects}
<text x="${width / 2}" y="388" text-anchor="middle" font-family="${SANS}" font-size="30" font-weight="600" fill="${INK}">${escape(TAGLINE)}</text>`

  return svg({
    width,
    height,
    title: 'bravebot',
    description: `A row of nine pixel-art bots above the name bravebot. ${TAGLINE}`,
    body,
  })
}

function arrow(x1, y, x2, colour) {
  return `<rect x="${x1}" y="${y - 2}" width="${x2 - x1 - 10}" height="4" fill="${colour}"/><polygon points="${x2},${y} ${x2 - 14},${y - 9} ${x2 - 14},${y + 9}" fill="${colour}"/>`
}

const label = (x, y, text, anchor = 'middle') =>
  `<text x="${x}" y="${y}" text-anchor="${anchor}" font-family="${SANS}" font-size="17" font-weight="700" letter-spacing="1.5" fill="${MUTED}">${escape(text)}</text>`

const words = (x, y, text, { size = 24, colour = INK, anchor = 'middle', mono = false, weight = 600 } = {}) =>
  `<text x="${x}" y="${y}" text-anchor="${anchor}" font-family="${mono ? MONO : SANS}" font-size="${size}" font-weight="${weight}" fill="${colour}">${escape(text)}</text>`

export const REFERENCE = 'ref:2  notes.md, 84 lines, 2.1 KiB, (U,priv)'
export const CALL = 'write_file  path: out.md  contents_ref: ref:2'

/** What the planner is handed, and where the bytes go instead. */
export function howItWorks() {
  const width = 1280
  const height = 520

  // The effect lane runs left to right at one height. Each column is [left edge, width].
  const top = 316
  const lane = 150
  const mid = top + lane / 2
  const [page, slot, gate, effect] = [[64, 170], [336, 210], [650, 230], [966, 254]]
  const centreOf = ([left, wide]) => left + wide / 2
  const rightOf = ([left, wide]) => left + wide
  const panel = (column, stroke, fill = '#161a30') =>
    `<rect x="${column[0]}" y="${top}" width="${column[1]}" height="${lane}" rx="14" fill="${fill}" stroke="${stroke}" stroke-width="3"/>`

  const planner = bot('readme-42', { centre: 145, baseline: 196, cell: 12, gaze: 1, d: 1, g: 3 })
  const card = `<rect x="280" y="52" width="940" height="168" rx="16" fill="#161a30" stroke="#3b4278" stroke-width="2"/>
${label(310, 86, 'THE PLANNER IS HANDED', 'start')}
${words(310, 128, REFERENCE, { mono: true, size: 28, anchor: 'start' })}
${label(310, 172, 'AND ANSWERS WITH', 'start')}
${words(310, 208, CALL, { mono: true, size: 28, colour: '#ffb02e', anchor: 'start' })}`

  // The page in its window, with one line of it picked out in the danger colour.
  const [px, pw] = page
  const browser = `<rect x="${px}" y="${top + 14}" width="${pw}" height="124" rx="10" fill="#161a30" stroke="${DANGER}" stroke-width="3"/>
<rect x="${px}" y="${top + 14}" width="${pw}" height="26" rx="10" fill="${DANGER}" fill-opacity=".35"/>
${[14, 30, 46].map((dx) => `<rect x="${px + dx}" y="${top + 24}" width="8" height="8" fill="${INK}" fill-opacity=".7"/>`).join('')}
<rect x="${px + 18}" y="${top + 58}" width="134" height="8" fill="${MUTED}" fill-opacity=".6"/>
<rect x="${px + 18}" y="${top + 78}" width="104" height="8" fill="${MUTED}" fill-opacity=".6"/>
<rect x="${px + 18}" y="${top + 98}" width="134" height="8" fill="${DANGER}"/>
<rect x="${px + 18}" y="${top + 118}" width="78" height="8" fill="${MUTED}" fill-opacity=".6"/>`

  const slotBox = `${panel(slot, DANGER, 'url(#hatch)')}
${words(centreOf(slot), top + 66, 'ref:2', { mono: true, size: 34, weight: 700 })}
${words(centreOf(slot), top + 104, 'the bytes', { size: 20, colour: INK, weight: 500 })}`

  const gateBox = `${panel(gate, ORANGE)}
${words(centreOf(gate), top + 70, 'resolves ref:2', { size: 28 })}
${words(centreOf(gate), top + 106, 'when the write or', { size: 22, colour: MUTED, weight: 500 })}
${words(centreOf(gate), top + 134, 'the call happens', { size: 22, colour: MUTED, weight: 500 })}`

  const effects = ['a file write you approve', "a program's stdin", 'a processor']
    .map((text, i) => {
      const y = top + i * 54
      return `<rect x="${effect[0]}" y="${y}" width="${effect[1]}" height="42" rx="21" fill="#161a30" stroke="#3b4278" stroke-width="2"/>${words(centreOf(effect), y + 28, text, { size: 20, weight: 500 })}`
    })
    .join('')

  // The planner's call reaches the gate. Nothing runs from the slot to the planner.
  const names = `<line x1="${centreOf(gate)}" y1="228" x2="${centreOf(gate)}" y2="298" stroke="${ORANGE}" stroke-width="4" stroke-dasharray="10 8"/><polygon points="${centreOf(gate)},${top - 2} ${centreOf(gate) - 10},${top - 20} ${centreOf(gate) + 10},${top - 20}" fill="${ORANGE}"/>
${words(centreOf(gate) + 20, 276, 'names ref:2', { size: 20, colour: ORANGE, anchor: 'start', weight: 700 })}`
  const never = `<line x1="${centreOf(slot)}" y1="${top - 2}" x2="${centreOf(slot)}" y2="230" stroke="${DANGER}" stroke-width="4" stroke-dasharray="6 6"/>
<path d="M${centreOf(slot) - 15} 258 L${centreOf(slot) + 15} 288 M${centreOf(slot) + 15} 258 L${centreOf(slot) - 15} 288" stroke="${DANGER}" stroke-width="6" stroke-linecap="square"/>
${words(centreOf(slot) - 34, 279, 'the bytes never reach the planner', { size: 20, colour: DANGER, anchor: 'end', weight: 700 })}`

  const captionY = top + lane + 30
  const body = `${planner}
${card}
${arrow(224, 140, 270, MUTED)}
${browser}
${label(centreOf(page), captionY, 'UNTRUSTED CONTENT')}
${arrow(rightOf(page) + 10, mid, slot[0] - 10, DANGER)}
${slotBox}
${label(centreOf(slot), captionY, 'WRITE-ONCE SLOT')}
${arrow(rightOf(slot) + 10, mid, gate[0] - 10, ORANGE)}
${gateBox}
${label(centreOf(gate), captionY, 'POLICY LAYER')}
${arrow(rightOf(gate) + 10, mid, effect[0] - 10, ORANGE)}
${effects}
${label(centreOf(effect), captionY, 'THE EFFECT')}
${names}
${never}`

  return svg({
    width,
    height,
    title: 'How bravebot keeps untrusted content away from the planner',
    description:
      `Untrusted content goes into a write-once slot. The planner is handed only a reference to it (${REFERENCE}) and answers with a call that names the reference (${CALL}). ` +
      'The policy layer resolves the reference where the effect happens: a file write you approve, the stdin of a program, or a processor. The bytes never go to the planner.',
    body,
  })
}

export const ART = {
  'readme-banner.svg': banner,
  'readme-how-it-works.svg': howItWorks,
}

function main() {
  const out = new URL('../../docs/assets/', import.meta.url)
  mkdirSync(out, { recursive: true })
  for (const [name, draw] of Object.entries(ART)) {
    writeFileSync(new URL(name, out), draw())
    console.log(`wrote: ${fileURLToPath(new URL(name, out))}`)
  }
}

// Run when this file is the program, and not when a test imports it. Through the real path both
// ways, for the reason `scripts/package.mjs` says.
if (process.argv[1] && import.meta.url === pathToFileURL(realpathSync(process.argv[1])).href) {
  main()
}
