// The words Token Spend… and Context Breakdown… show (CMD-15, CMD-19), from what the bridge's
// session.report.cost and session.report.context answer, and the Last Request… dialog (TRACE-9)
// drawn from what session.report.request answers.
import test from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { build, buildSync } from 'esbuild'
const require = createRequire(import.meta.url)
function load(path) {
  const source = buildSync({ entryPoints: [path], bundle: true, write: false, platform: 'node', format: 'cjs' }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
}
const { costNotice, contextNotice, statusNotice, statsRows, statsScope, tokens } = load('src/renderer/reports.ts')
const React = require('react')
const { renderToStaticMarkup } = require('react-dom/server')

// Session Status… (CMD-8), from what session.report.status answers.
const status = (overrides) => ({
  project: '/work/app',
  folders: ['/work/app', '/work/lib'],
  model: 'local/test',
  definition: null,
  permissionMode: 'acceptEdits',
  sandboxMode: 'workspace-write',
  sessionId: 'abc123',
  title: 'Fix the parser',
  turns: 2,
  tokens: 2400,
  rulesHeldByTurn: false,
  rules: { paths: [], commands: [] },
  ...overrides,
})

test('the status names the session, where it works, its modes and what it has spent', () => {
  const notice = statusNotice(status())
  assert.equal(notice.title, 'Session status')
  const lines = notice.body.split('\n')
  assert.match(lines[0], /^Session +Fix the parser$/)
  assert.match(lines[1], /^Session id +abc123$/)
  assert.match(lines[2], /^Directory +\/work\/app$/)
  assert.match(lines[3], /^Also open +\/work\/lib$/)
  assert.match(lines[4], /^Model +local\/test$/)
  assert.match(lines[5], /^Permissions +Accept edits$/)
  assert.match(lines[6], /^Sandbox +Workspace write$/)
  assert.match(lines[7], /^Spent +2 turns · 2\.4k tokens$/)
  assert.doesNotMatch(notice.body, /Agent/)
  assert.match(statusNotice(status({ title: null, sessionId: null })).body, /^Session +untitled, nothing sent yet\nDirectory/)
})

test('the status says the goal and its counts, and nothing of a goal where none is set (GOAL-13)', () => {
  assert.doesNotMatch(statusNotice(status()).body, /Goal|goal/)
  const goal = { condition: 'cargo test exits 0', rounds: 0, left: 10, paused: false, stalled: false, elapsedSeconds: 42, spentTokens: 900 }
  const lines = statusNotice(status({ goal })).body.split('\n')
  const at = lines.findIndex((line) => line.startsWith('Goal '))
  assert.match(lines[at], /^Goal +cargo test exits 0$/)
  assert.match(lines[at + 1], /^ +not judged yet · 10 of 10 rounds left$/)
  assert.match(lines[at + 2], /^ +set 42s ago · 900 tokens spent$/)
  const later = statusNotice(status({ goal: { ...goal, rounds: 3, left: 7, elapsedSeconds: 185, spentTokens: 12400 } })).body
  assert.match(later, /\n +sent back 3 times · 7 of 10 rounds left\n +set 3m 05s ago · 12\.4k tokens spent\n/)
  assert.match(statusNotice(status({ goal: { ...goal, rounds: 2, left: 8, paused: true } })).body, /\n +sent back 2 times · 8 of 10 rounds left · paused, \/goal resume arms it\n/)
  assert.match(statusNotice(status({ goal: { ...goal, rounds: 3, left: 7, stalled: true } })).body, /\n +sent back 3 times · 7 of 10 rounds left · stopped: rounds made no tool call\n/)
})

test('between turns the status lists the rules', () => {
  const empty = statusNotice(status()).body
  assert.match(empty, /\nTrust +nothing vouched for/)
  assert.match(empty, /\nPrograms +every run is put to you/)
  assert.doesNotMatch(empty, /held by the running turn/)
  const listed = statusNotice(status({
    rules: {
      paths: [{ path: '', shownPath: '', integrity: 'trusted' }, { path: 'vendor', shownPath: 'vendor', integrity: 'untrusted' }],
      commands: [{ display: 'cargo test', shownDirectory: '.' }],
    },
  })).body.split('\n')
  const at = listed.findIndex((line) => line.startsWith('Trust '))
  assert.match(listed[at], /^Trust +Project root · trusted$/)
  assert.match(listed[at + 1], /^ +vendor · untrusted$/)
  assert.match(listed[at + 2], /^Trusted commands +cargo test · in \.$/)
})

test('mid-turn the status says the rules are held by the running turn and states neither', () => {
  const notice = statusNotice(status({ rulesHeldByTurn: true, rules: null }))
  assert.match(notice.body, /\nTrust +held by the running turn$/m)
  assert.match(notice.body, /\nPrograms +held by the running turn$/)
  assert.match(notice.subtitle, /running turn holds the trust and the vouched programs\. They are shown once it ends\./)
  assert.doesNotMatch(notice.body, /nothing vouched for|every run is put to you|Trusted commands/)
  assert.equal(statusNotice(status()).subtitle, undefined)
})

// `Modal` draws through a portal, which one server pass cannot render, so it is drawn in place.
const inPlaceModal = {
  name: 'in-place-modal',
  setup(built) {
    built.onResolve({ filter: /^\.\/Modal$/ }, () => ({ path: 'modal', namespace: 'in-place-modal' }))
    built.onLoad({ filter: /.*/, namespace: 'in-place-modal' }, () => ({
      loader: 'js',
      resolveDir: process.cwd(),
      contents: `import { createElement } from 'react'
        export function Modal({ title, subtitle, children, actions }) { return createElement('div', { className: 'modal', 'data-title': title, 'data-subtitle': subtitle }, children, actions) }`,
    }))
  },
}
async function requestDialog() {
  const built = await build({
    entryPoints: ['src/renderer/components/RequestDialog.tsx'],
    bundle: true, write: false, platform: 'node', format: 'cjs', jsx: 'automatic',
    external: ['react', 'react-dom', 'react/jsx-runtime'],
    plugins: [inPlaceModal],
  })
  const module = { exports: {} }
  new Function('require', 'module', 'exports', built.outputFiles[0].text)(require, module, module.exports)
  return module.exports.RequestDialog
}
const drawn = async (report) => renderToStaticMarkup(React.createElement(await requestDialog(), { report, onClose() {} }))

// TRACE-9: Last Request… before any turn says there is no request yet.
test('the last request before any turn says no request has been sent yet', async () => {
  const markup = await drawn({ none: true, instructionFiles: [], spans: [] })
  assert.match(markup, /No request has been sent to the model in this session yet\./)
  assert.doesNotMatch(markup, /request-span/)
})

// TRACE-9: the instruction files with their bytes come first, then each span under its label.
test('the last request lists the instruction files with their bytes, then each span under its label', async () => {
  const markup = await drawn({
    none: false,
    instructionFiles: [{ path: 'AGENTS.md', bytes: 14 }, { path: 'docs/AGENTS.md', bytes: 3 }],
    spans: [
      { role: 'system', label: 'driver', text: 'You are bravebot.' },
      { role: 'system', label: 'trusted file AGENTS.md', text: 'Keep it short.' },
      { role: 'user', label: 'typed', text: 'list the files' },
      { role: 'tool', label: 'ref:1', text: 'The result is held as ref:1.' },
    ],
  })
  const files = markup.indexOf('Instruction files loaded: AGENTS.md (14 bytes), docs/AGENTS.md (3 bytes)')
  assert.ok(files >= 0, markup)
  const heads = ['system · driver', 'system · trusted file AGENTS.md', 'user · typed', 'tool · ref:1'].map((head) => markup.indexOf(head))
  assert.ok(heads.every((at, index) => at > files && (index === 0 || at > heads[index - 1])), `in order: ${heads}`)
  assert.match(markup, /<pre class="request-span-text">You are bravebot\.<\/pre>/)
})

test('a request whose system prompt loaded no instruction file says so', async () => {
  const markup = await drawn({ none: false, instructionFiles: [], spans: [{ role: 'system', label: 'driver', text: 'x' }] })
  assert.match(markup, /Instruction files loaded: none/)
})

// TRACE-9 and TRACE-11: a span's text is shown as plain text. The bridge sends an escape and a bell
// as their pictures (`printable_spelled`); the dialog shows those pictures and no raw control,
// and markup in a tool result is shown as characters and not drawn.
test('a span holding an escape and a bell shows them escaped, and markup in it as text', async () => {
  const markup = await drawn({
    none: false,
    instructionFiles: [],
    spans: [{ role: 'tool', label: 'tool result (trusted)', text: 'list ␛[31mthe␇ files <img src=x onerror=alert(1)> **bold**' }],
  })
  assert.match(markup, /list ␛\[31mthe␇ files/)
  assert.doesNotMatch(markup, /[\x1b\x07]/)
  assert.match(markup, /&lt;img src=x onerror=alert\(1\)&gt; \*\*bold\*\*/)
  assert.doesNotMatch(markup, /<img|<strong/)
})

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

// Token Spend Across Chats… (CLI-37), from what session.stats answers.
const figures = (tokens) => ({
  sessions: 1, turns: 2, tokens, read: 0, written: 0, unsplit: tokens, cacheRead: 0, cacheWritten: 0, hitRatePermille: null, hitRate: '-',
})
const stats = (models) => ({
  directory: null, days: null, total: figures(900), models, unreadable: 0, withoutCache: { sessions: 0, silent: 0, older: 0, note: null },
})

test('the spend rows keep the model order the bridge sent and put Total last', () => {
  const rows = statsRows(stats([{ model: 'big', ...figures(600) }, { model: 'small', ...figures(300) }]))
  assert.deepEqual(rows.map((row) => row.label), ['big', 'small', 'Total'])
  assert.deepEqual(rows.map((row) => row.total), [false, false, true])
  assert.equal(rows[0].tokens, 600)
  assert.equal(rows[2].tokens, 900)
  assert.equal(rows[2].unsplit, 900)
  assert.equal('model' in rows[0], false)
})

test('a model row of null is labelled unknown, and Total still comes last', () => {
  const rows = statsRows(stats([{ model: 'big', ...figures(600) }, { model: null, ...figures(300) }]))
  assert.deepEqual(rows.map((row) => row.label), ['big', 'unknown', 'Total'])
  assert.equal(rows[1].total, false)
})

test('with no model rows the table is the Total row alone', () => {
  assert.deepEqual(statsRows(stats([])).map((row) => [row.label, row.total]), [['Total', true]])
})

// A model answered by two services is two rows the bridge sent, and the dialog must say which is
// which: a row drawn by model name alone would show two rows of the same name. The service's word
// and the hit rate are the ones the bridge sent: the gateway row carries a word and a rate in
// another catalog's form, so a renderer that worded either itself from `service` or
// `hitRatePermille` would show `gateway` or `0.0%` there instead.
test('a model served by two services is two rows that name the service as the bridge words it, and Total names none', () => {
  const rows = statsRows(stats([
    { model: 'shared', service: 'bedrock', serviceWord: 'bedrock', ...figures(600), cacheRead: 300, cacheWritten: 40, hitRatePermille: 500, hitRate: '50.0%' },
    { model: 'shared', service: 'gateway', serviceWord: 'passerelle', ...figures(200), hitRatePermille: 0, hitRate: '0,0 %' },
    { model: 'shared', service: 'aichat', serviceWord: 'brave', ...figures(60) },
    { model: null, service: null, serviceWord: 'unknown', ...figures(40) },
  ]))
  assert.deepEqual(rows.map((row) => [row.label, row.service, row.hitRate]), [
    ['shared', 'bedrock', '50.0%'], ['shared', 'passerelle', '0,0 %'], ['shared', 'brave', '-'], ['unknown', 'unknown', '-'], ['Total', '', '-'],
  ])
  const shown = rows.filter((row) => !row.total).map((row) => `${row.label} ${row.service}`)
  assert.equal(new Set(shown).size, shown.length, 'no two rows read alike')
  assert.equal(rows[0].cacheRead, 300)
  assert.equal(rows[0].cacheWritten, 40)
  assert.equal(rows[0].hitRatePermille, 500)
  assert.equal(rows[4].service, '')
})

test('the scope sentence says which sessions and which projects are counted', () => {
  assert.equal(statsScope(null, null), 'Every recorded session, in every project')
  assert.equal(statsScope(7, null), 'Sessions last written in the last 7 days, in every project')
  assert.equal(statsScope(1, null), 'Sessions last written in the last day, in every project')
  assert.equal(statsScope(null, '/work/app'), 'Every recorded session in /work/app')
  assert.equal(statsScope(30, '/work/app'), 'Sessions last written in the last 30 days, in /work/app')
  assert.equal(statsScope(1, '/work/app'), 'Sessions last written in the last day, in /work/app')
  for (const directory of [null, '/work/app']) {
    assert.doesNotMatch(statsScope(1, directory), /1 days/)
  }
})
