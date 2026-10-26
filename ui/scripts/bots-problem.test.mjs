// When a problem with the bots file is put on the banner that other actions clear.
import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const source = buildSync({ entryPoints: ['src/renderer/bots-problem.ts'], bundle: true, write: false, platform: 'node', format: 'cjs' }).outputFiles[0].text
const module = { exports: {} }
new Function('require', 'module', 'exports', source)(require, module, module.exports)
const { problemToShow } = module.exports

const BROKEN = '/home/someone/.bravebot/bots.json: expected value at line 1'

test('a problem that has not been shown is shown, and none is shown when there is none', () => {
  assert.equal(problemToShow(BROKEN, null, false), BROKEN)
  assert.equal(problemToShow(BROKEN, 'another problem', false), BROKEN)
  assert.equal(problemToShow(null, BROKEN, false), null)
  assert.equal(problemToShow(null, null, true), null)
})

test('a problem already shown is not put up again by a read nobody asked to see the list for', () => {
  assert.equal(problemToShow(BROKEN, BROKEN, false), null)
})

test('a problem still in force is shown again when the person goes to the list', () => {
  assert.equal(problemToShow(BROKEN, BROKEN, true), BROKEN)
})
