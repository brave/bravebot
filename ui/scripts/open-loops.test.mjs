// OPEN-3 on the desktop: the bridge's open-loops line becomes one transcript entry under the turn
// it ended, and only when the bridge sent a line.
import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'
import { fileURLToPath } from 'node:url'
const require = createRequire(import.meta.url)
// Resolved from this file, so the test runs from the repository root as well as from `ui/`.
const ui = fileURLToPath(new URL('..', import.meta.url))
const bundled = (entry) => {
  const source = buildSync({ absWorkingDir: ui, entryPoints: [entry], bundle: true, write: false, platform: 'node', format: 'cjs' }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
}
const { apply } = bundled('src/renderer/apply.ts')
const { sessionStatus } = bundled('src/renderer/session-list.ts')

const live = () => ({ entries: [], turns: {}, archived: 0, summary: { id: 's1' }, awaitingOrdinal: null, running: true })
const folded = (data, event = 'turn.done') => {
  let state = live()
  apply({ event, session: 's1', data }, (update) => { state = update(state) }, () => {}, () => {})
  return state
}
const fold = (data, event) => folded(data, event).entries
const done = (extra) => ({ turn: 1, reply: 'Wrote notes.md', model: 'm', steps: 1, clean: true, tokens: 1, outputTokens: 1, notices: [], trust: { rules: [] }, id: 's1', archived: 0, ...extra })
const failed = (extra) => ({ turn: 1, kind: 'cancelled', message: 'stopped', ...extra })

test('turn.done with a line appends one open-loops entry after the reply', () => {
  const entries = fold(done({ openLoops: 'open: changes not built (planner)' }))
  assert.deepEqual(entries.map((entry) => entry.kind), ['assistant', 'open-loops'])
  assert.equal(entries[1].text, 'open: changes not built (planner)')
  assert.equal(entries[1].turn, 1)
  assert.equal(fold(done({ openLoops: 'open: nothing' }))[1].text, 'open: nothing')
})

test('turn.done with no line appends no open-loops entry', () => {
  assert.deepEqual(fold(done({ openLoops: null })).map((entry) => entry.kind), ['assistant'])
  assert.deepEqual(fold(done({})).map((entry) => entry.kind), ['assistant'])
})

test('turn.error carries the line the same way', () => {
  const entries = fold(failed({ openLoops: 'open: changes not built (planner)' }), 'turn.error')
  assert.deepEqual(entries.map((entry) => entry.kind), ['error', 'open-loops'])
  assert.equal(entries[1].text, 'open: changes not built (planner)')
  assert.deepEqual(fold(failed({ openLoops: null }), 'turn.error').map((entry) => entry.kind), ['error'])
  assert.deepEqual(fold(failed({}), 'turn.error').map((entry) => entry.kind), ['error'])
})

test('a turn that failed reads as failed with its open-loops line after the error', () => {
  const state = folded(failed({ openLoops: 'open: changes not built (planner)' }), 'turn.error')
  assert.equal(state.entries.at(-1).kind, 'open-loops')
  assert.equal(sessionStatus(state), 'failed')
  assert.equal(sessionStatus(folded(done({ openLoops: 'open: nothing' }))), undefined)
})
