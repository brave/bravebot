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
const { costNotice, contextNotice, tokens } = load('src/renderer/reports.ts')
const React = require('react')
const { renderToStaticMarkup } = require('react-dom/server')

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
