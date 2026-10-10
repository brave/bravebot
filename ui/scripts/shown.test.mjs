// That a decision card's approval waits on the rows it rests on (PROMPT-4).
//
// [PROMPT-4](../../docs/specs/prompting.md#PROMPT-4) says the window takes no approval until the
// rows the answer rests on have been on screen. A card marks those rows with `data-deciding`, and
// `shown.ts` counts them. These tests hold the marks to the rows, per kind, and the count to its
// rules. drive-shown.mjs holds the window to it with the cards drawn in a small window.

import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'
import { CASES } from './shown-cases.mjs'

const require = createRequire(import.meta.url)
const React = require('react')
const { renderToStaticMarkup } = require('react-dom/server')

function load(path) {
  const source = buildSync({
    entryPoints: [path],
    bundle: true,
    write: false,
    platform: 'node',
    format: 'cjs',
    jsx: 'automatic',
    external: ['react', 'react-dom', 'react/jsx-runtime'],
  }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
}

const { Row } = load('src/renderer/components/Transcript.tsx')
const t = load('src/renderer/transcript.ts')
const { Ledger, rowsOf, mayAnswer, leftWords, NOT_MEASURED } = load('src/renderer/shown.ts')

const ENTRY = {
  confirm: t.asked, run: t.askedRun, output: t.askedOutput, vet: t.askedVet, vouch: t.askedVouch, fetch: t.askedFetch,
  server: t.askedServer, manifest: t.askedManifest, exposure: t.askedExposure, 'mcp-server': t.askedMcpServer,
  'mcp-tools': t.askedMcpTools, 'mcp-call': t.askedMcpCall, 'mcp-move': t.askedMcpMove, ask: t.askedQuestions,
}

const draw = (entry) =>
  renderToStaticMarkup(React.createElement(Row, { entry, onDecide() {}, onAnswer() {}, onFork() {}, forkable: true }))

const VOID = new Set(['area', 'base', 'br', 'col', 'embed', 'hr', 'img', 'input', 'link', 'meta', 'source', 'track', 'wbr'])

/** The text of some markup, run by run, each with the `data-deciding` of the nearest element marked. */
function texts(markup) {
  const open = []
  const runs = []
  let at = 0
  for (const found of markup.matchAll(/<(\/?)([a-zA-Z][\w-]*)([^>]*)>/g)) {
    if (found.index > at) runs.push({ text: markup.slice(at, found.index), deciding: open.findLast((element) => element.deciding)?.deciding ?? null })
    at = found.index + found[0].length
    const [, closing, name, attributes] = found
    if (closing) {
      const index = open.findLastIndex((element) => element.name === name)
      if (index >= 0) open.length = index
    } else if (!VOID.has(name)) {
      open.push({ name, deciding: attributes.match(/data-deciding="([a-z]+)"/)?.[1] ?? null })
    }
  }
  return runs
}

const buttons = (markup) => [...markup.matchAll(/<button([^>]*)>(.*?)<\/button>/g)].map(([, attributes, label]) => ({ attributes, label }))

test('every kind of question has a case, so a new kind with no deciding rows fails here', () => {
  assert.deepEqual(new Set(Object.values(CASES).map((found) => found.kind)), new Set([...Object.keys(t.REPLY), 'ask']))
  for (const [name, found] of Object.entries(CASES)) {
    assert.ok(found.tokens.length > 0 || (found.none && found.none.length > 20), `${name} waits on nothing, for no stated reason`)
  }
})

test('the rows an approval rests on are the rows each card marks as deciding', () => {
  for (const [name, found] of Object.entries(CASES)) {
    const markup = draw(ENTRY[found.kind](found.request))
    const runs = texts(markup)
    for (const token of [...found.tokens, ...found.standing]) {
      const holding = runs.filter((run) => run.text.includes(token))
      // Once: an approval waits on a row, and a word drawn twice would let the copy stand in for it.
      assert.equal(holding.reduce((sum, run) => sum + run.text.split(token).length - 1, 0), 1, `${name}: ${token} is drawn once\n${markup}`)
      const [run] = holding
      assert.ok(run.deciding, `${name}: ${token} is drawn outside every row the card marks as deciding\n${markup}`)
      assert.equal(run.deciding === 'standing', found.standing.includes(token), `${name}: ${token} is marked ${run.deciding}`)
    }
    if (found.none) assert.ok(!markup.includes('data-deciding'), `${name} marks a row it says nothing waits on`)
  }
})

test('before anything is measured no approval is open, and a refusal always is', () => {
  // `react-dom/server` measures nothing, which is the state a card is in before its first frame.
  for (const [name, found] of Object.entries(CASES)) {
    if (found.none) continue
    const markup = draw(ENTRY[found.kind](found.request))
    for (const { attributes, label } of buttons(markup)) {
      const approving = /class="approve/.test(attributes)
      assert.equal(/ disabled=""/.test(attributes), approving, `${name}: "${label}" ${approving ? 'is open' : 'waits'}`)
    }
    assert.ok(!markup.includes('class="shown-left"'), `${name}: a count is drawn before anything was counted`)
  }
})

const looked = (key, rows, { standing = false, hidden = false } = {}) => ({ key, standing, hidden, rows: rows.map(([width, seen]) => ({ width, seen })) })

test('a row counts once all of it has been in view, in one look or across several', () => {
  const ledger = new Ledger()
  assert.deepEqual(ledger.take(500, [looked('a', [[300, [0, 150]]])]), { left: 1, standing: 0 })
  assert.deepEqual(ledger.take(500, [looked('a', [[300, [150, 300]]])]), { left: 0, standing: 0 })
  assert.equal(ledger.read('a', 1, 0, 500, 300), true)
})

test('pieces of a row with a gap between them do not add up to the row', () => {
  const ledger = new Ledger()
  ledger.take(500, [looked('a', [[300, [0, 100]]])])
  assert.deepEqual(ledger.take(500, [looked('a', [[300, [200, 300]]])]), { left: 1, standing: 0 })
})

test('a change in the card’s width starts the count again', () => {
  const ledger = new Ledger()
  assert.deepEqual(ledger.take(500, [looked('a', [[300, [0, 300]]]), looked('b', [[200, [0, 200]]])]), { left: 0, standing: 0 })
  // Text that wraps differently is different rows: what was read at one width is not those rows.
  assert.deepEqual(ledger.take(480, [looked('a', [[300, null]]), looked('b', [[200, null]])]), { left: 2, standing: 0 })
  assert.equal(ledger.read('a', 1, 0, 500, 300), false)
})

test('a change in how many rows an element draws starts that element again, and only it', () => {
  const ledger = new Ledger()
  ledger.take(500, [looked('a', [[300, [0, 300]]]), looked('b', [[200, [0, 200]]])])
  assert.deepEqual(ledger.take(500, [looked('a', [[300, null], [120, null]]), looked('b', [[200, null]])]), { left: 2, standing: 0 })
  assert.equal(ledger.read('b', 1, 0, 500, 200), true)
})

test('rows only a standing answer waits on are counted apart', () => {
  const ledger = new Ledger()
  assert.deepEqual(ledger.take(500, [looked('a', [[300, [0, 300]]]), looked('s', [[300, null], [300, null]], { standing: true })]), { left: 0, standing: 2 })
})

test('an element with text and no row drawn is a row not read', () => {
  const ledger = new Ledger()
  assert.deepEqual(ledger.take(500, [looked('a', [], { hidden: true })]), { left: 1, standing: 0 })
})

test('boxes that share most of their height are one row, and boxes that only touch are two', () => {
  const rows = rowsOf([
    { top: 20, bottom: 40, left: 0, right: 30 },
    { top: 0, bottom: 20, left: 0, right: 50 },
    { top: 2, bottom: 18, left: 50, right: 90 },
  ])
  assert.deepEqual(rows, [{ top: 0, bottom: 20, left: 0, right: 90 }, { top: 20, bottom: 40, left: 0, right: 30 }])
})

test('an answer is open once measured with nothing left, and a standing one waits on its own rows too', () => {
  const measured = (left, standing) => ({ measured: true, left, standing, note: 'n' })
  assert.equal(mayAnswer(NOT_MEASURED), false)
  assert.equal(mayAnswer(measured(1, 0)), false)
  assert.equal(mayAnswer(measured(0, 1)), true)
  assert.equal(mayAnswer(measured(0, 1), true), false)
  assert.equal(mayAnswer(measured(0, 0), true), true)
  assert.equal(leftWords(NOT_MEASURED), null)
  assert.equal(leftWords(measured(1, 3)), '1 more line to read before approving')
  assert.equal(leftWords(measured(0, 3)), '3 more lines to read before remembering')
  assert.equal(leftWords(measured(0, 0)), null)
})
