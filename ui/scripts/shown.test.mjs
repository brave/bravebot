// That a decision card's approval waits on the rows it rests on (PROMPT-4).
//
// [PROMPT-4](../../docs/specs/prompting.md#PROMPT-4) says the window takes no approval until the
// rows the answer rests on have been on screen. A card marks those rows with `data-deciding`, and
// `shown.ts` counts them. These tests hold the marks to the rows, per kind, and the count to its
// rules. drive-shown.mjs holds the window to it with the cards drawn in a small window.

import test from 'node:test'
import assert from 'node:assert/strict'
import { build } from 'esbuild'
import { createRequire } from 'node:module'
import { readFileSync } from 'node:fs'
import { CASES } from './shown-cases.mjs'

const require = createRequire(import.meta.url)
const React = require('react')
const { renderToStaticMarkup } = require('react-dom/server')

// `react-dom/server` draws a Leo button as a bare `<leo-button>`, with none of the props that shut
// it, so here it is drawn as the plain button it stands for. A Leo alert loses its class the same
// way, and the class is what says a row is a warning, so it is drawn as a plain `div`.
const plainLeoButton = {
  name: 'plain-leo-button',
  setup(built) {
    built.onResolve({ filter: /^@brave\/leo\/react\/(button|alert)$/ }, ({ path }) => ({ path: path.split('/').pop(), namespace: 'plain-leo-button' }))
    built.onLoad({ filter: /^button$/, namespace: 'plain-leo-button' }, () => ({
      loader: 'js',
      resolveDir: process.cwd(),
      contents: `import { createElement } from 'react'
        export default function Button({ className, isDisabled, onClick, children }) {
          return createElement('button', { className, disabled: !!isDisabled, onClick }, children)
        }`,
    }))
    built.onLoad({ filter: /^alert$/, namespace: 'plain-leo-button' }, () => ({
      loader: 'js',
      resolveDir: process.cwd(),
      contents: `import { createElement } from 'react'
        export default function Alert({ className, children }) {
          return createElement('div', { className }, children)
        }`,
    }))
  },
}

async function load(path) {
  const source = (await build({
    entryPoints: [path],
    bundle: true,
    write: false,
    platform: 'node',
    format: 'cjs',
    jsx: 'automatic',
    external: ['react', 'react-dom', 'react/jsx-runtime'],
    plugins: [plainLeoButton],
  })).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
}

const { Row } = await load('src/renderer/components/Transcript.tsx')
const t = await load('src/renderer/transcript.ts')
const { Ledger, rowsOf, mayAnswer, leftWords, NOT_MEASURED } = await load('src/renderer/shown.ts')

const ENTRY = {
  confirm: t.asked, run: t.askedRun, output: t.askedOutput, vet: t.askedVet, vouch: t.askedVouch, fetch: t.askedFetch,
  server: t.askedServer, manifest: t.askedManifest, exposure: t.askedExposure, 'mcp-server': t.askedMcpServer,
  'mcp-tools': t.askedMcpTools, 'mcp-call': t.askedMcpCall, 'mcp-move': t.askedMcpMove, host: t.askedHost, ask: t.askedQuestions,
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
    const drawn = buttons(markup)
    assert.ok(drawn.some(({ attributes }) => /class="approve/.test(attributes)), `${name}: no approving button is drawn\n${markup}`)
    for (const { attributes, label } of drawn) {
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

test('a row is hidden where any box in it is, so a clipped piece of it is not read', () => {
  const rows = rowsOf([
    { top: 0, bottom: 20, left: 0, right: 50 },
    { top: 0, bottom: 20, left: 50, right: 90, hidden: true },
    { top: 20, bottom: 40, left: 0, right: 30 },
  ])
  assert.deepEqual(rows.map((row) => !!row.hidden), [true, false])
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

// Rows that carry no token: those that exist only when a request says so, and the sentences that close a card. The cases above set none of these fields, so a
// mark dropped from one of the rows they draw would fail nothing there: no token sits in it.
const CONDITIONAL = {
  confirm: {
    request: { ...CASES.confirm.request, untrusted: true, exact: false, writtenSinceCheckout: true, hiddenNote: '2 characters in the lines this changes draw as nothing' },
    phrases: [
      'Read it as you would a stranger’s patch',
      '2 characters in the lines this changes draw as nothing',
      'too dissimilar to diff exactly',
      'after the checkout was made',
    ],
  },
  run: {
    request: {
      ...CASES.run.request, releasesPrivate: true, plan: 'grep zqr31 notes.md', confinement: null,
      notSandboxed: 'Nothing confines these programs.', ambient: [{ authority: 'agent-socket', named: 'zqr34' }],
    },
    phrases: ['Execution plan:', 'Nothing confines these programs.', 'hands your own data to the program', 'This spends access that is yours elsewhere'],
  },
  output: {
    request: CASES.output.request,
    phrases: ['The planner has not seen this', '2 characters in this text draw as nothing'],
  },
  vet: {
    request: CASES.vet.request,
    phrases: ['1 character in this text draws as nothing'],
  },
  // The sentences that say what a yes does come after the last token in their cards.
  exposure: { request: CASES.exposure.request, phrases: ['Keeping it back keeps this file’s text from the model'] },
  host: { request: CASES.host.request, phrases: ['A yes lets every command the model runs'] },
}

test('the warnings and closing sentences no case token sits in are rows the card marks as deciding', () => {
  for (const [name, { request, phrases }] of Object.entries(CONDITIONAL)) {
    const markup = draw(ENTRY[name](request))
    const runs = texts(markup)
    for (const phrase of phrases) {
      const holding = runs.filter((run) => run.text.includes(phrase))
      assert.equal(holding.length, 1, `${name}: "${phrase}" is drawn once\n${markup}`)
      assert.equal(holding[0].deciding, 'all', `${name}: "${phrase}" is drawn outside every row the card marks as deciding\n${markup}`)
    }
  }
})

test('a write, an output or a vetted read the agent sent no sentence for says nothing about characters that draw as nothing', () => {
  for (const kind of ['confirm', 'output', 'vet']) {
    for (const [hidden, hiddenNote] of [[undefined, undefined], [0, null]]) {
      const markup = draw(ENTRY[kind]({ ...CASES[kind].request, hidden, hiddenNote }))
      assert.ok(!markup.includes('draw as nothing') && !markup.includes('draws as nothing'), `${kind}: a count of ${hidden} drew the sentence\n${markup}`)
      assert.ok(!markup.includes('hidden-characters'), `${kind}: a count of ${hidden} drew the row\n${markup}`)
    }
  }
})

test('the sentence the agent sent is drawn as sent, with no wording of the card’s own', () => {
  for (const kind of ['confirm', 'output', 'vet']) {
    const note = 'Zq9 sentence the agent worded, in whatever language'
    const markup = draw(ENTRY[kind]({ ...CASES[kind].request, hiddenNote: note }))
    assert.equal(texts(markup).filter((run) => run.text === note && run.deciding === 'all').length, 1, `${kind}: the sentence is drawn once, as a deciding row\n${markup}`)
    assert.ok(!markup.includes('draw as nothing') && !markup.includes('draws as nothing'), `${kind}: the card put words of its own beside the agent's\n${markup}`)
  }
})

// The overlay drawn while a file is dragged over the transcript takes no pointer, so a hit test
// passes through it and only the mark tells `shown.ts` that it covers the rows under it.
// drive-shown.mjs holds the window to it in a real renderer; this fails without one.
test('the overlay drawn while a file is dragged over the transcript is marked as a veil', () => {
  const source = readFileSync('src/renderer/components/Transcript.tsx', 'utf8')
  const overlay = source.match(/<div [^>]*data-test="drop-target"[^>]*>/)?.[0]
  assert.ok(overlay, 'the drop overlay is no longer found by its data-test')
  assert.match(overlay, /data-veil="own"/, `the drop overlay takes no pointer and is not marked as a veil: ${overlay}`)
})

test('every warning a decision card draws is inside a row it marks as deciding, or holds one', () => {
  const drawn = [...Object.values(CASES).map(({ kind, request }) => [kind, request]), ...Object.entries(CONDITIONAL).map(([kind, { request }]) => [kind, request])]
  let warnings = 0
  for (const [kind, request] of drawn) {
    const markup = draw(ENTRY[kind](request))
    const open = []
    const looked = []
    for (const found of markup.matchAll(/<(\/?)([a-zA-Z][\w-]*)([^>]*)>/g)) {
      const [, closing, tag, attributes] = found
      if (closing) {
        const index = open.findLastIndex((element) => element.tag === tag)
        if (index >= 0) open.length = index
        continue
      }
      if (VOID.has(tag)) continue
      const deciding = /data-deciding="/.test(attributes)
      // A warning that holds the marked row itself, like the secret alert's title, is marked where it counts.
      if (deciding) for (const outer of open) outer.holds = true
      const element = { tag, deciding, holds: false, inside: open.some((outer) => outer.deciding), at: found.index }
      open.push(element)
      if (/class="[^"]*\b(warn|card-alert)\b/.test(attributes)) looked.push(element)
    }
    for (const element of looked) {
      assert.ok(element.inside || element.deciding || element.holds, `${kind}: a warning is drawn outside every row the card marks as deciding\n${markup.slice(element.at, element.at + 200)}`)
    }
    warnings += looked.length
  }
  assert.ok(warnings >= 8, `only ${warnings} warnings were looked at`)
})
