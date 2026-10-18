import test from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { buildSync } from 'esbuild'
const require = createRequire(import.meta.url)
function load(path) {
  const source = buildSync({ entryPoints: [path], bundle: true, write: false, platform: 'node', format: 'cjs' }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
}
const { elapsed, spentLine, timeLine, cacheLine, turnRows } = load('src/renderer/usage.ts')

const usage = (more = {}) => ({
  turns: 2, tokens: 1000, byTurn: [{ turn: 1, tokens: 600 }, { turn: 2, tokens: 333 }], unattributed: 67,
  timing: { wallMs: 0, inferenceMs: 0, toolsMs: 0, stalledMs: 0, overheadMs: 0 }, ...more,
})
const timing = (wallMs, inferenceMs = 0, toolsMs = 0, stalledMs = 0) =>
  ({ wallMs, inferenceMs, toolsMs, stalledMs, overheadMs: 0 })

test('elapsed floors to the second and pads seconds once there are minutes', () => {
  assert.equal(elapsed(999), '0s')
  assert.equal(elapsed(59_999), '59s')
  assert.equal(elapsed(60_000), '1m 00s')
  assert.equal(elapsed(252_400), '4m 12s')
})

test('a session that has spent nothing says so', () => {
  assert.equal(spentLine(usage({ turns: 0, tokens: 0, byTurn: [], unattributed: 0 })), 'Nothing spent yet')
  assert.equal(spentLine(usage({ turns: 1 })), `1 turn · ${(1000).toLocaleString()} tokens`)
  assert.equal(spentLine(usage()), `2 turns · ${(1000).toLocaleString()} tokens`)
})

test('the time line names only the parts that came to a second', () => {
  assert.equal(timeLine(usage({ timing: timing(900, 800) })), null)
  assert.equal(timeLine(usage({ timing: timing(5_000, 500, 400) })), '5s working')
  assert.equal(
    timeLine(usage({ timing: timing(252_000, 181_000, 65_000, 3_000) })),
    '4m 12s working: 3m 01s model, 1m 05s tools, 3s waiting for you',
  )
  assert.equal(timeLine(usage({ timing: timing(10_000, 9_000) })), '10s working: 9s model')
})

test('the cache line names only what happened', () => {
  assert.equal(cacheLine(usage()), null)
  assert.equal(cacheLine(usage({ cached: { read: 0, written: 0 } })), null)
  assert.equal(cacheLine(usage({ cached: { read: 90, written: 0 } })), 'Last turn’s cache: 90 read')
  assert.equal(cacheLine(usage({ cached: { read: 90, written: 12 } })), 'Last turn’s cache: 90 read, 12 written')
})

test('the rows add up to the total, with what no turn accounts for last', () => {
  const rows = turnRows(usage())
  assert.deepEqual(rows.map((r) => r.label), ['Turn 1', 'Turn 2', 'Not attributed to a turn'])
  assert.deepEqual(rows.map((r) => r.share), [60, 33, 6])
  assert.equal(rows[0].tokens, (600).toLocaleString())
})

test('turn zero is what was spent before the first turn, and nothing unattributed adds no row', () => {
  const rows = turnRows(usage({ byTurn: [{ turn: 0, tokens: 100 }, { turn: 1, tokens: 900 }], unattributed: 0 }))
  assert.deepEqual(rows.map((r) => r.label), ['Before the first turn', 'Turn 1'])
})

test('a total of nothing gives no share rather than dividing by it', () => {
  const rows = turnRows(usage({ tokens: 0, byTurn: [{ turn: 1, tokens: 0 }], unattributed: 0 }))
  assert.deepEqual(rows.map((r) => r.share), [null])
})
