/**
 * Keeps the renderer on Nala: every colour, type size, shadow and icon comes from Leo.
 *
 * Fails on, across every stylesheet module:
 * - hardcoded colours (hex, rgb) and px font sizes;
 * - a raw `box-shadow` that is not `none`, a token, or a token-coloured focus ring;
 * - a `var(--leo-…)` that Leo does not define, which is a typo that silently paints nothing;
 * - the same selector declared twice in one module and one at-rule;
 * - more raw px spacing, or more size-only `--leo-typography-*-font-size` reads, than the
 *   ratchets below allow (they may only go down);
 * - a raw `<svg` in the renderer outside the bot avatar illustration;
 * - a unicode glyph used as an icon, in JSX text or CSS `content:`.
 */
import { readFileSync, readdirSync } from 'node:fs'
import { join, dirname, relative } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const renderer = join(root, 'src/renderer')

/** Ratchets: the counts as of the last module that landed. Lower them; never raise them. */
const PX_SPACING_MAX = 102
const TYPOGRAPHY_SIZE_MAX = 44

function walk(dir, test) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory() ? walk(join(dir, e.name), test) : test(e.name) ? [join(dir, e.name)] : [],
  )
}

const cssFiles = walk(renderer, (name) => name.endsWith('.css'))
const tsxFiles = walk(renderer, (name) => name.endsWith('.tsx') || name.endsWith('.ts'))

const leoTokens = new Set(
  [...readFileSync(join(root, 'node_modules/@brave/leo/tokens/css/variables.css'), 'utf8').matchAll(/(--leo-[a-z0-9-]+)\s*:/g)].map((m) => m[1]),
)
/** Families whose every name is a Leo token. Component knobs (`--leo-button-color`) are not in the file. */
const tokenFamily = /var\((--leo-(?:color|font|spacing|radius|effect|duration|easing|typography|gradient|elevation)-[a-z0-9-]+)/g

const hex = /#[0-9a-fA-F]{3,8}\b/g
const rgb = /rgba?\([^)]*\)/g
const fontPx = /font-size:\s*[0-9.]+px/g
const fontShorthandPx = /\bfont\s*:[^;]*\b[0-9.]+px\b/g
const spacingPx = /\b(?:padding|margin|gap|inset|top|left|right|bottom)(?:-[a-z]+)?:[^;{}]*\b[1-9][0-9.]*px\b/g
const typographySize = /var\(--leo-typography-[a-z0-9-]+-font-size\)/g
const shadowValue = /box-shadow\s*:\s*([^;]+);?/g
const ring = /^(inset\s+)?0 0 0 [0-9.]+px (var\(--[a-z0-9-]+\)|transparent)$/
const shadowOk = (value) => value.split(/,(?![^(]*\))/).every((part) => {
  const one = part.trim()
  return one === 'none' || /^var\(--[a-z0-9-]+\)$/.test(one) || ring.test(one)
})
const glyphs = /[↑↓✓▸›⋯↗]/
const cssContentArrow = /content\s*:\s*['"][^'"]*[→←↑↓✓▸›⋯↗]/

/** Prose arrows in sentences, not icons. `file:substring`. */
const GLYPH_ALLOW = ['AgentSettings.tsx:home → project']

const problems = []
const fail = (file, line, message) => problems.push(`${relative(root, file)}:${line}: ${message}`)
let pxSpacing = 0
let typographySizes = 0

const stripComments = (text) => text.replace(/\/\*[\s\S]*?\*\//g, (c) => c.replace(/[^\n]/g, ' '))

for (const file of cssFiles) {
  const text = stripComments(readFileSync(file, 'utf8'))
  const lines = text.split('\n')
  lines.forEach((line, i) => {
    for (const re of [hex, rgb, fontPx, fontShorthandPx]) {
      re.lastIndex = 0
      if (re.test(line)) fail(file, i + 1, `hardcoded colour or font size: ${line.trim()}`)
    }
    for (const m of line.matchAll(shadowValue)) {
      if (!shadowOk(m[1].trim())) fail(file, i + 1, `raw box-shadow (use --shadow-* or --leo-effect-*): ${m[1].trim()}`)
    }
    for (const m of line.matchAll(tokenFamily)) {
      if (!leoTokens.has(m[1])) fail(file, i + 1, `unknown Leo token ${m[1]}`)
    }
    if (cssContentArrow.test(line)) fail(file, i + 1, 'glyph icon in CSS content; use a Leo Icon')
    pxSpacing += (line.match(spacingPx) ?? []).length
    typographySizes += (line.match(typographySize) ?? []).length
  })

  // Duplicate selectors, per at-rule context. Legacy is the pile the modules are cut from.
  if (file.endsWith('legacy.css')) continue
  const seen = new Map()
  const stack = []
  let buffer = ''
  let line = 1
  for (const char of text) {
    if (char === '\n') line++
    if (char === '{') {
      const selector = buffer.trim().replace(/\s+/g, ' ')
      buffer = ''
      const context = stack.join(' ⟩ ')
      stack.push(selector)
      if (selector.startsWith('@') || /^(from|to|[0-9.]+%)/.test(selector)) continue
      const key = `${context}::${selector}`
      if (seen.has(key)) fail(file, line, `selector declared twice (first at line ${seen.get(key)}): ${selector}`)
      else seen.set(key, line)
    } else if (char === '}') {
      stack.pop()
      buffer = ''
    } else if (char === ';') {
      buffer = ''
    } else {
      buffer += char
    }
  }
}

for (const file of tsxFiles) {
  const text = readFileSync(file, 'utf8')
  const name = relative(renderer, file)
  text.split('\n').forEach((line, i) => {
    const code = line.replace(/\/\/.*$/, '')
    const trimmed = code.trim()
    if (trimmed.startsWith('*') || trimmed.startsWith('/*') || trimmed.startsWith('{/*')) return
    if (/<svg\b/.test(code) && !name.endsWith('BotAvatar.tsx')) fail(file, i + 1, 'raw <svg>; use a Leo Icon')
    if (glyphs.test(code) && !GLYPH_ALLOW.some((allow) => {
      const [where, what] = allow.split(':')
      return name.endsWith(where) && line.includes(what)
    })) fail(file, i + 1, `glyph used as an icon: ${trimmed.slice(0, 80)}`)
    for (const m of code.matchAll(tokenFamily)) {
      if (!leoTokens.has(m[1])) fail(file, i + 1, `unknown Leo token ${m[1]}`)
    }
  })
}

if (pxSpacing > PX_SPACING_MAX) problems.push(`raw px spacing: ${pxSpacing}, over the ratchet of ${PX_SPACING_MAX}; use --leo-spacing-*`)
if (typographySizes > TYPOGRAPHY_SIZE_MAX) problems.push(`size-only --leo-typography-*-font-size reads: ${typographySizes}, over the ratchet of ${TYPOGRAPHY_SIZE_MAX}; use a --type-* role`)

console.log(`check-nala: px spacing ${pxSpacing}/${PX_SPACING_MAX}, typography sizes ${typographySizes}/${TYPOGRAPHY_SIZE_MAX}`)
if (problems.length) {
  console.error(problems.join('\n'))
  console.error(`\ncheck-nala: ${problems.length} problem${problems.length === 1 ? '' : 's'}; use Leo tokens, icons and the --type-*/--shadow-* roles.`)
  process.exit(1)
}
console.log('check-nala: ok')
