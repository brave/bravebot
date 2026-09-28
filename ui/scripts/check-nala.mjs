/**
 * Fail if styles.css / export.css still contain hardcoded colours or px font sizes
 * outside a short allowlist. Keeps the Nala migration from drifting back to custom tokens.
 */
import { readFileSync } from 'node:fs'
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

let failed = false
for (const rel of files) {
  const text = readFileSync(join(root, rel), 'utf8')
  const lines = text.split('\n')
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]
    if (ALLOW.some((re) => re.test(line))) continue
    for (const re of [hex, rgb, fontPx]) {
      re.lastIndex = 0
      if (re.test(line)) {
        console.error(`${rel}:${i + 1}: ${line.trim()}`)
        failed = true
      }
    }
  }
}

if (failed) {
  console.error('\ncheck-nala: hardcoded colour or font-size found; use --leo-* tokens.')
  process.exit(1)
}
console.log('check-nala: ok')
