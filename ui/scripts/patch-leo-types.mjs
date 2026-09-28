/**
 * Leo ships `svelte-react.ts` as a type dependency of its React `.d.ts` files.
 * Our strict settings (`noUncheckedIndexedAccess`, `noUnusedLocals`) then fail
 * typecheck on that upstream file. Soften the three spots so `tsc` can pass.
 */
import { readFileSync, writeFileSync, existsSync } from 'node:fs'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const target = join(root, 'node_modules/@brave/leo/src/components/svelte-react.ts')
if (!existsSync(target)) {
  throw new Error(`Cannot patch Leo types: ${target} is missing`)
}

let source = readFileSync(target, 'utf8')
const before = source

const replacements = [
  ['const event = match[1].toLowerCase()', 'const event = match[1]!.toLowerCase()'],
  ['el.removeEventListener(removed, lastValue.current[removed])', 'el.removeEventListener(removed, lastValue.current[removed]!)'],
  ['>(tag: string, component: typeof HTMLElement) {', '>(tag: string, _component: typeof HTMLElement) {'],
]

for (const [original, patched] of replacements) {
  if (source.includes(patched)) continue
  if (!source.includes(original)) {
    throw new Error(`Cannot patch Leo types: expected source not found: ${original}`)
  }
  source = source.replace(original, patched)
}

if (source !== before) writeFileSync(target, source)
