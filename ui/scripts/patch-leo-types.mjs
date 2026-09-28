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
if (!existsSync(target)) process.exit(0)

let source = readFileSync(target, 'utf8')
const before = source

source = source.replace(
  'const event = match[1].toLowerCase()',
  'const event = match[1]!.toLowerCase()',
)
source = source.replace(
  'el.removeEventListener(removed, lastValue.current[removed])',
  'el.removeEventListener(removed, lastValue.current[removed]!)',
)
source = source.replace(
  '>(tag: string, component: typeof HTMLElement) {',
  '>(tag: string, _component: typeof HTMLElement) {',
)

if (source !== before) writeFileSync(target, source)
