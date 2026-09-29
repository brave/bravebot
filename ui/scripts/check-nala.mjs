/**
 * Fail if styles.css / export.css still contain hardcoded colours or px font sizes
 * outside a short allowlist. Keeps the Nala migration from drifting back to custom tokens.
 */
import { readFileSync, readdirSync } from 'node:fs'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const files = ['src/renderer/styles.css', 'src/renderer/export.css']

/** Layout-only values that are not colours or type sizes (e.g. column gutters). Empty for now. */
const ALLOW = [
  // Example: /--lights:\s*78px/,
]

const hex = /#[0-9a-fA-F]{3,8}\b/g
const rgb = /rgba?\([^)]*\)/g
const fontPx = /font-size:\s*[0-9.]+px/g
const fontShorthandPx = /\bfont\s*:[^;]*\b[0-9.]+px\b/g

let failed = false
for (const rel of files) {
  const text = readFileSync(join(root, rel), 'utf8')
  const lines = text.split('\n')
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]
    if (ALLOW.some((re) => re.test(line))) continue
    for (const re of [hex, rgb, fontPx, fontShorthandPx]) {
      re.lastIndex = 0
      if (re.test(line)) {
        console.error(`${rel}:${i + 1}: ${line.trim()}`)
        failed = true
      }
    }
  }
}

// Report-only until the token pass lands: these are counted so the migration can see them
// shrink, then promoted to failures (see docs/development.md).
const boxShadow = /box-shadow:\s*(?!none|var\(--leo-effect-elevation)[^;]*/g
const inlineIconPx = /--leo-icon-size['"]?\s*:\s*['"`]?[0-9.]+px/g
const spacingPx = /\b(?:padding|margin|gap)(?:-[a-z]+)?:[^;{}]*\b[0-9.]+px\b/g

function tsxFiles(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory() ? tsxFiles(join(dir, e.name)) : e.name.endsWith('.tsx') ? [join(dir, e.name)] : [],
  )
}

const report = { 'raw box-shadow': 0, 'inline icon px (tsx)': 0, 'px spacing': 0 }
for (const rel of files) {
  const text = readFileSync(join(root, rel), 'utf8')
  report['raw box-shadow'] += (text.match(boxShadow) ?? []).length
  report['px spacing'] += (text.match(spacingPx) ?? []).length
}
for (const file of tsxFiles(join(root, 'src/renderer'))) {
  report['inline icon px (tsx)'] += (readFileSync(file, 'utf8').match(inlineIconPx) ?? []).length
}
console.log(
  'check-nala (report-only): ' +
    Object.entries(report)
      .map(([k, n]) => `${k}: ${n}`)
      .join(', '),
)

if (failed) {
  console.error('\ncheck-nala: hardcoded colour or font-size found; use --leo-* tokens.')
  process.exit(1)
}
console.log('check-nala: ok')
