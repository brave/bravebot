/**
 * Leo ships `svelte-react.ts` as a type dependency of its React `.d.ts` files.
 * Our strict settings (`noUncheckedIndexedAccess`, `noUnusedLocals`) then fail
 * typecheck on that upstream file. Soften the three spots so `tsc` can pass.
 */
import { existsSync, readFileSync, renameSync, writeFileSync } from 'node:fs'
import { join, dirname } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const replacements = [
  ['const event = match[1].toLowerCase()', 'const event = match[1]!.toLowerCase()'],
  ['el.removeEventListener(removed, lastValue.current[removed])', 'el.removeEventListener(removed, lastValue.current[removed]!)'],
  ['>(tag: string, component: typeof HTMLElement) {', '>(tag: string, _component: typeof HTMLElement) {'],
]

// pnpm hard-links installed files to its content-addressed store, so writing into the file in
// place would rewrite the store's copy for every project that shares it. The patched text goes
// to a new file that replaces the link.
export function patchLeoTypes(target) {
  if (!existsSync(target)) {
    throw new Error(`Cannot patch Leo types: ${target} is missing`)
  }

  let source = readFileSync(target, 'utf8')
  const before = source

  for (const [original, patched] of replacements) {
    if (source.includes(patched)) continue
    if (!source.includes(original)) {
      throw new Error(`Cannot patch Leo types: expected source not found: ${original}`)
    }
    source = source.replace(original, patched)
  }

  if (source !== before) {
    const staged = `${target}.patching`
    writeFileSync(staged, source)
    renameSync(staged, target)
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const root = join(dirname(fileURLToPath(import.meta.url)), '..')
  patchLeoTypes(join(root, 'node_modules/@brave/leo/src/components/svelte-react.ts'))
}
