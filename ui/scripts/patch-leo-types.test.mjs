import { test } from 'node:test'
import assert from 'node:assert/strict'
import { linkSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { patchLeoTypes } from './patch-leo-types.mjs'

const UPSTREAM = [
  'const event = match[1].toLowerCase()',
  'el.removeEventListener(removed, lastValue.current[removed])',
  '>(tag: string, component: typeof HTMLElement) {',
].join('\n')

function fixture(t) {
  const dir = mkdtempSync(join(tmpdir(), 'patch-leo-'))
  t.after(() => rmSync(dir, { recursive: true, force: true }))
  const stored = join(dir, 'stored.ts')
  const installed = join(dir, 'installed.ts')
  writeFileSync(stored, UPSTREAM)
  linkSync(stored, installed)
  return { stored, installed }
}

test('the installed file is patched', (t) => {
  const { installed } = fixture(t)
  patchLeoTypes(installed)
  const patched = readFileSync(installed, 'utf8')
  assert.match(patched, /match\[1\]!\.toLowerCase\(\)/)
  assert.match(patched, /_component: typeof HTMLElement/)
})

test('the file the install was linked from is left as it was', (t) => {
  const { stored, installed } = fixture(t)
  patchLeoTypes(installed)
  assert.equal(readFileSync(stored, 'utf8'), UPSTREAM)
})

test('a second run changes nothing and a changed upstream is refused', (t) => {
  const { installed } = fixture(t)
  patchLeoTypes(installed)
  const once = readFileSync(installed, 'utf8')
  patchLeoTypes(installed)
  assert.equal(readFileSync(installed, 'utf8'), once)
  writeFileSync(installed, 'export {}\n')
  assert.throws(() => patchLeoTypes(installed), /expected source not found/)
})
