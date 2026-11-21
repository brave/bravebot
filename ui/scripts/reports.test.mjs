// The words Token Spend… and Context Breakdown… show (CMD-15, CMD-19), from what the bridge's
// session.report.cost and session.report.context answer.
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
const { costNotice, contextNotice, tokens } = load('src/renderer/reports.ts')

test('a count of tokens is written as the terminal writes it', () => {
  assert.equal(tokens(1), '1 token')
  assert.equal(tokens(999), '999 tokens')
  assert.equal(tokens(12_345), '12.3k tokens')
})

test('a session that has spent nothing says so', () => {
  const notice = costNotice({ total: 0, turns: 0, rows: [], unattributed: 0, running: 0 })
  assert.equal(notice.title, 'Token spend')
  assert.equal(notice.body, 'Nothing spent yet.')
})

test('spend is listed by turn with each share, and what no turn accounts for gets its own row', () => {
  const notice = costNotice({
    total: 3000,
    turns: 2,
    rows: [{ turn: 0, tokens: 200, share: 6 }, { turn: 1, tokens: 1500, share: 50 }, { turn: 2, tokens: 800, share: 26 }],
    unattributed: 500,
    running: 0,
  })
  const lines = notice.body.split('\n')
  assert.match(lines[0], /^This session +2 turns · 3\.0k tokens$/)
  assert.match(lines[1], /^Before turn 1 +200 tokens +6%$/)
  assert.match(lines[2], /^Turn 1 +1\.5k tokens +50%$/)
  assert.match(lines[3], /^Turn 2 +800 tokens +26%$/)
  assert.match(lines[4], /^Not recorded against any turn +500 tokens$/)
  assert.equal(notice.subtitle, undefined)
})

test('spend asked mid-turn says the running turn is counted', () => {
  const notice = costNotice({ total: 40, turns: 1, rows: [{ turn: 1, tokens: 40, share: 100 }], unattributed: 0, running: 40 })
  assert.match(notice.subtitle, /running turn/)
  assert.match(notice.body, /Turn 1 +40 tokens +100%/)
})

test('a measured context is broken down by section and called approximate', () => {
  const notice = contextNotice({
    state: 'measured',
    total: 4000,
    budget: 10_000,
    sections: [{ section: 'system', tokens: 1000, share: 25 }, { section: 'results', tokens: 3000, share: 75 }],
  })
  assert.equal(notice.title, 'Context breakdown')
  assert.match(notice.subtitle, /^Approximate/)
  const lines = notice.body.split('\n')
  assert.match(lines[0], /^Last request +4\.0k tokens · context 40%$/)
  assert.match(lines[1], /^System prompt +1\.0k tokens +25%$/)
  assert.match(lines[2], /^Tool results +3\.0k tokens +75%$/)
  assert.equal(lines.length, 3)
})

test('each empty state of the context is named rather than shown as an empty table', () => {
  const empty = (state) => contextNotice({ state, total: 0, budget: 0, sections: [] })
  assert.match(empty('not-measured').body, /has not been measured yet/)
  assert.match(empty('compacted').body, /was compacted/)
  assert.match(contextNotice({ state: 'no-request', total: 900, budget: 10_000, sections: [] }).body, /no request of its own/)
  for (const state of ['not-measured', 'compacted', 'no-request']) {
    assert.equal(empty(state).subtitle, undefined, state)
  }
})
