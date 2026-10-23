// The pictures in the README, drawn from the pixel figures the desktop app gives each bot.
//
// A bot's face is a function of its seed (src/renderer/avatar/pixels.ts), so the bots here are
// faces real bots have, not a redrawing of them. `node scripts/readme-art.mjs` writes
// docs/assets/readme-banner.svg, readme-how-it-works.svg and readme-plan-first.svg.
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

const BACKGROUND = '#0b0c14'
const PANEL = '#14161f'
const EDGE = '#363a4d'
const INK = '#eceaf8'
const MUTED = '#a6abd0'
const ORANGE = '#ff5601'
const DANGER = '#ff5d73'
const SAFE = '#5be28a'

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

const CHECK = ['......#', '.....##', '#...##.', '##.##..', '.###...', '..#....']

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
<rect width="${width}" height="${height}" rx="24" fill="${BACKGROUND}"/>
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
    { x: 40, y: 36, width: width - 80, height: 124 },
  ]
  const body = `${stars(90, keepOut, width, height)}
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

const words = (x, y, content, { size = 22, colour = INK, anchor = 'middle', mono = false, weight = 600 } = {}) =>
  `<text x="${x}" y="${y}" text-anchor="${anchor}" font-family="${mono ? MONO : SANS}" font-size="${size}" font-weight="${weight}" fill="${colour}">${escape(content)}</text>`

/** Lines of a caption, centred on `x`, the first line's baseline on `y`. */
const caption = (x, y, lines, colour = INK) => lines.map((line, i) => words(x, y + i * 30, line, { colour })).join('')

/** A line from `x1` to `x2` at height `y` with an arrowhead on the right end. */
const arrow = (x1, y, x2, colour) =>
  `<rect x="${x1}" y="${y - 2}" width="${x2 - x1 - 10}" height="4" fill="${colour}"/><polygon points="${x2},${y} ${x2 - 16},${y - 10} ${x2 - 16},${y + 10}" fill="${colour}"/>`

/** A reference, as a tag sitting on an arrow. */
const chip = (cx, cy, content) => {
  const wide = content.length * 15 + 32
  return `<rect x="${cx - wide / 2}" y="${cy - 21}" width="${wide}" height="42" rx="21" fill="${PANEL}" stroke="${MUTED}" stroke-width="2"/>${words(cx, cy + 8, content, { mono: true, size: 24, weight: 700 })}`
}

const card = (x, y, wide, high, stroke = EDGE) => `<rect x="${x}" y="${y}" width="${wide}" height="${high}" rx="16" fill="${PANEL}" stroke="${stroke}" stroke-width="2"/>`

/** The rows of a grid as one rect per run of `#`. */
function pixels(grid, { x, y, cell, fill }) {
  const rects = []
  grid.forEach((row, r) => {
    for (const run of row.matchAll(/#+/g)) {
      rects.push(`<rect x="${x + run.index * cell}" y="${y + r * cell}" width="${run[0].length * cell}" height="${cell}" fill="${fill}"/>`)
    }
  })
  return rects.join('')
}

/** A to-do list: each row is `todo`, `now` or `done`, with a box and a line of text. */
function checklist(x, y, rows) {
  const widths = [120, 100, 112, 80]
  return rows
    .map((state, i) => {
      const top = y + i * 32
      const mark =
        state === 'done'
          ? `<rect x="${x}" y="${top}" width="24" height="24" rx="4" fill="${SAFE}"/>${pixels(CHECK, { x: x + 3, y: top + 4, cell: 3, fill: BACKGROUND })}`
          : `<rect x="${x + 1}" y="${top + 1}" width="22" height="22" rx="4" fill="none" stroke="${state === 'now' ? ORANGE : MUTED}" stroke-width="3"/>${state === 'now' ? `<rect x="${x + 8}" y="${top + 8}" width="8" height="8" fill="${ORANGE}"/>` : ''}`
      return `${mark}<rect x="${x + 38}" y="${top + 8}" width="${widths[i % widths.length]}" height="8" fill="${MUTED}" fill-opacity="${state === 'done' ? 0.45 : 0.8}"/>`
    })
    .join('')
}

/** A web page's instructions are stopped, and what reaches the planner is a reference. */
export function howItWorks() {
  const width = 1280
  const height = 490

  const page =`${card(48, 130, 158, 140, DANGER)}
<rect x="48" y="130" width="158" height="26" rx="16" fill="${DANGER}" fill-opacity=".35"/>
${[14, 30, 46].map((dx) => `<rect x="${48 + dx}" y="${140}" width="8" height="8" fill="${INK}" fill-opacity=".7"/>`).join('')}
<rect x="66" y="176" width="122" height="8" fill="${MUTED}" fill-opacity=".6"/>
<rect x="66" y="196" width="92" height="8" fill="${MUTED}" fill-opacity=".6"/>
<rect x="66" y="216" width="122" height="8" fill="${DANGER}"/>
<rect x="66" y="236" width="70" height="8" fill="${MUTED}" fill-opacity=".6"/>
${words(127, 326, 'A web page')}`

  const blocked = `<rect x="232" y="104" width="312" height="48" rx="24" fill="${DANGER}" fill-opacity=".18" stroke="${DANGER}" stroke-width="2"/>
${words(388, 136, 'Ignore your instructions', { weight: 700 })}
${arrow(214, 172, 552, DANGER)}
<rect x="560" y="108" width="14" height="128" fill="${DANGER}"/>
<circle cx="567" cy="172" r="24" fill="${DANGER}" stroke="${BACKGROUND}" stroke-width="4" shape-rendering="auto"/>
<path d="M556 161 L578 183 M578 161 L556 183" stroke="${INK}" stroke-width="6" stroke-linecap="square"/>`

  const reference = `${arrow(214, 252, 632, MUTED)}${chip(400, 252, 'ref:2')}`
  const planner = `${bot('readme-42', { centre: 700, baseline: 280, cell: 12, gaze: -1, nods: false, d: 1, g: 3 })}${words(700, 326, 'Planner')}`
  const starts = `${arrow(776, 232, 900, MUTED)}${chip(838, 232, 'ref:2')}`
  const processors = ['readme-30', 'readme-4', 'readme-25']
    .map((seed, i) => bot(seed, { centre: 975 + i * 85, baseline: 280, cell: 7, nods: false, d: 1 + i * 1.9, g: i * 2.7 }))
    .join('')

  const body = `${page}
${blocked}
${reference}
${planner}
${starts}
${processors}
${words(1060, 326, 'Processors')}
${caption(300, 396, ['Instructions in a web page', 'never reach the planner.'])}
${caption(700, 396, ['The planner is handed', 'a reference to the page.'])}
${caption(1060, 396, ['Processors work on it.', 'They hold no tools, no', 'memory, no conversation.'])}`

  return svg({
    width,
    height,
    title: 'How bravebot keeps instructions in a web page away from the planner',
    description:
      'A web page holds instructions. They are blocked and never reach the planner. The planner is handed a reference to the page, ref:2, instead. ' +
      'It starts processors that work on the reference, and a processor holds no tools, no memory and no conversation.',
    body,
  })
}

/** Manifest mode: the planner plans the whole run, a person approves it, and a driver runs it. */
export function planFirst() {
  const width = 1280
  const height = 400
  const wide = 360
  const gap = 60
  const left = (width - (3 * wide + 2 * gap)) / 2
  const at = (i) => left + i * (wide + gap)
  const top = 40
  const centreOf = (i) => at(i) + wide / 2

  const plan = `${bot('readme-42', { centre: at(0) + 82, baseline: 176, cell: 9, gaze: 1, nods: false, d: 2, g: 1 })}${checklist(at(0) + 170, 76, ['todo', 'todo', 'todo', 'todo'])}`
  const approve = pixels(CHECK, { x: centreOf(1) - 49, y: 84, cell: 14, fill: SAFE })
  const run = checklist(at(2) + 101, 76, ['done', 'done', 'now', 'todo'])

  const cards = [
    { art: plan, title: 'Plan first', lines: ['The planner writes the', 'whole plan before it has', 'read anything.'] },
    { art: approve, title: 'Approve', lines: ['You approve the plan once,', 'before the first step runs.'] },
    { art: run, title: 'Execute', lines: ['A driver runs the steps.', 'No model is in the control', 'path. Nothing is re-planned.'] },
  ]
  const body = cards
    .map(
      ({ art, title, lines }, i) => `${card(at(i), top, wide, 320)}
${art}
${words(centreOf(i), 232, title, { size: 28, weight: 700 })}
${caption(centreOf(i), 270, lines, MUTED)}`,
    )
    .join('\n')
  const arrows = [0, 1].map((i) => arrow(at(i) + wide + 8, 200, at(i + 1) - 8, ORANGE)).join('')

  return svg({
    width,
    height,
    title: 'Manifest mode: plan first, then execute',
    description:
      'Manifest mode in three steps. The planner writes the whole plan before it has read anything. You approve the plan once, before the first step runs. ' +
      'A driver runs the steps with no model in the control path, and nothing is re-planned.',
    body: `${body}\n${arrows}`,
  })
}

export const ART = {
  'readme-banner.svg': banner,
  'readme-how-it-works.svg': howItWorks,
  'readme-plan-first.svg': planFirst,
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
