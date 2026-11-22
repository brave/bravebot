// The desktop list of background sessions (BG-17) draws the groups the bridge builds, which carry
// the BG-16 headings, order and counts; the renderer only reads the sessions out of them in order
// and makes the line a row copies.

import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'

function load() {
  const source = buildSync({ entryPoints: ['src/renderer/background-list.ts'], bundle: true, write: false, platform: 'node', format: 'cjs' }).outputFiles[0].text
  const module = { exports: {} }
  new Function('module', 'exports', source)(module, module.exports)
  return module.exports
}

const { sessionsOf, attachCommand } = load()

const row = (id, state) => ({
  id: `${id}-0000`, short: id, name: `session ${id}`, state, held: null, words: state, updated: 0, directory: '/work', prompt: 'p',
})

const group = (state, ...sessions) => ({ state, heading: state, count: sessions.length, sessions })

test('the sessions are read in the order of the groups the bridge sent, and of the rows in each', () => {
  const groups = [group('needs-input', row('n1', 'needs-input')), group('working', row('w2', 'working'), row('w1', 'working'))]
  assert.deepEqual(sessionsOf(groups).map((each) => each.short), ['n1', 'w2', 'w1'])
  assert.deepEqual(sessionsOf([]), [])
})

test('the copied line attaches by the short id the row shows', () => {
  assert.equal(attachCommand(row('a1b2c3d4', 'stopped')), 'bravebot attach a1b2c3d4')
})
