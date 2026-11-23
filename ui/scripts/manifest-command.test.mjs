// `/manifest` typed into the composer (CMD-1, MANIFEST-9), as the desktop window reads it.
//
// The bridge plans and runs the task (crates/ui-bridge/src/manifest.rs). What the window owns is
// which lines start a run at all. The faults these reject are a longer word taken for the command,
// and a line that only mentions the command being taken for it.
import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const load = (contents) => {
  const source = buildSync({ stdin: { contents, resolveDir: process.cwd(), loader: 'ts' }, bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'] }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
}

const shared = load("export * from './src/shared/manifest'")

test('the command word with a task gives the trimmed task', () => {
  assert.equal(shared.manifestTask('/manifest t'), 't')
  assert.equal(shared.manifestTask('/manifest fix the docs'), 'fix the docs')
  assert.equal(shared.manifestTask('  /manifest   fix the docs  '), 'fix the docs')
})

test('the command word alone gives an empty task', () => {
  assert.equal(shared.manifestTask('/manifest'), '')
  assert.equal(shared.manifestTask('  /manifest  '), '')
})

test('any other line is not the command', () => {
  assert.equal(shared.manifestTask('/manifestx'), null)
  assert.equal(shared.manifestTask('/manifestx t'), null)
  assert.equal(shared.manifestTask('say /manifest'), null)
  assert.equal(shared.manifestTask('say /manifest x'), null)
  assert.equal(shared.manifestTask('manifest x'), null)
  assert.equal(shared.manifestTask(''), null)
})
