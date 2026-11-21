// The desktop list of background sessions (BG-17) groups the bridge's rows under the BG-16
// headings, in the BG-16 order, with a count on each and no heading for an empty state.

import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'

function load() {
  const source = buildSync({ entryPoints: ['src/renderer/background-list.ts'], bundle: true, write: false, platform: 'node', format: 'cjs' }).outputFiles[0].text
  const module = { exports: {} }
  new Function('module', 'exports', source)(module, module.exports)
  return module.exports
}

const { grouped, heading, stateWords, attachCommand } = load()

const row = (id, state, held = null) => ({
  id: `${id}-0000`, short: id, name: `session ${id}`, state, held, updated: 0, directory: '/work', prompt: 'p',
})

test('the states come in BG-16 order, with a count on each heading', () => {
  // The bridge's order is deliberately scrambled here, so the order is grouped()'s own.
  const rows = [row('s1', 'stopped'), row('i1', 'idle'), row('w1', 'working'), row('r1', 'interrupted'), row('n1', 'needs-input', 'run'), row('w2', 'working')]
  const groups = grouped(rows)
  assert.deepEqual(groups.map((group) => group.state), ['needs-input', 'interrupted', 'working', 'idle', 'stopped'])
  assert.deepEqual(groups.map((group) => group.heading), ['Needs input', 'Interrupted', 'Working', 'Ready', 'Stopped'])
  assert.deepEqual(groups.map((group) => group.count), [1, 1, 2, 1, 1])
})

test('a state no session is in has no heading', () => {
  const groups = grouped([row('w1', 'working'), row('s1', 'stopped')])
  assert.deepEqual(groups.map((group) => group.state), ['working', 'stopped'])
  assert.deepEqual(grouped([]), [])
})

test('rows keep the order they came in within a state', () => {
  const groups = grouped([row('w3', 'working'), row('s1', 'stopped'), row('w1', 'working'), row('w2', 'working')])
  assert.deepEqual(groups[0].rows.map((each) => each.short), ['w3', 'w1', 'w2'])
  assert.equal(groups[0].count, groups[0].rows.length)
})

test('idle is headed Ready, and the other states take the Sessions screen words', () => {
  assert.equal(heading('idle'), 'Ready')
  assert.equal(heading('needs-input'), 'Needs input')
  assert.equal(heading('interrupted'), 'Interrupted')
  assert.equal(heading('working'), 'Working')
  assert.equal(heading('stopped'), 'Stopped')
})

test('a held prompt adds its kind to the state words, and nothing else does', () => {
  assert.equal(stateWords(row('n1', 'needs-input', 'run')), 'Needs input · run')
  assert.equal(stateWords(row('n2', 'needs-input')), 'Needs input')
  assert.equal(stateWords(row('i1', 'idle')), 'Ready')
})

test('the copied line attaches by the short id the row shows', () => {
  assert.equal(attachCommand(row('a1b2c3d4', 'stopped')), 'bravebot attach a1b2c3d4')
})
