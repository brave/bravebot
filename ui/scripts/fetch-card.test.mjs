// What the fetch card puts in front of a person, and what an answer to it reaches.
//
// [FETCH-2](../../docs/specs/tools/fetch-url.md#FETCH-2) says every fetch asks, and that whoever
// is asked is shown the address and, separately, the host it reaches. This window is the third
// surface that asks, so it owes the same two lines, and the same two absences FETCH-1 and FETCH-3
// state: no answer here trusts what comes back, and no answer is remembered.
//
// The card is rendered through `react-dom/server` and the markup is what is asserted on, because
// what a person can press and what a click could follow are properties of the markup. The reply
// table is read by importing it and the allow-list by reading its source: the main process is not
// something a test can import without an Electron to run it in.

import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'

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

/** The card, and every answer pressing something on it sent. */
function draw(entry) {
  const sent = []
  const markup = renderToStaticMarkup(
    React.createElement(Row, {
      entry,
      onDecide: (...answer) => sent.push(answer),
      onAnswer() {},
      onFork() {},
      forkable: false,
    }),
  )
  return { markup, sent }
}

/** The question as the bridge sends it. */
const asked = (overrides = {}) =>
  t.askedFetch({
    request: 7,
    url: 'https://docs.example.com/api',
    host: 'docs.example.com',
    ambient: [],
    summary: 'fetch from docs.example.com',
    ...overrides,
  })

const buttons = (markup) => [...markup.matchAll(/<button[^>]*>(.*?)<\/button>/g)].map((found) => found[1])

test('the host is drawn on a line of its own, as the agent read it', () => {
  // An address written to be misread: a person skimming it takes the first name, and the request
  // goes to the second. The card draws what it was sent and works nothing out from the address.
  const { markup } = draw(asked({ url: 'https://example.com@evil.test/docs', host: 'evil.test' }))

  assert.ok(markup.includes('<code class="path" data-deciding="first">https://example.com@evil.test/docs</code>'), markup)
  assert.match(markup, /Talking to:<\/strong> <code>evil\.test<\/code>/)
})

test('nothing on the card is something a click follows or a load fetches', () => {
  const { markup } = draw(asked())

  for (const forbidden of ['<a ', 'href=', 'src=', '<img', '<iframe']) {
    assert.ok(!markup.includes(forbidden), `drew ${forbidden}: ${markup}`)
  }
})

test('the card says what a yes does not grant', () => {
  const { markup } = draw(asked())

  assert.ok(markup.includes('What comes back stays confined however you answer'), markup)
  assert.ok(markup.includes('Nothing is remembered'), markup)
})

test('a waiting fetch offers a refusal and one fetch, and no standing answer', () => {
  const { markup } = draw(asked())

  assert.deepEqual(buttons(markup), ['Don’t fetch', 'Fetch once'])
  assert.ok(!markup.includes('always'), markup)
})

test('a fetch that was answered says which way, and offers nothing further', () => {
  const approved = draw({ ...asked(), decision: 'approve' }).markup
  assert.ok(approved.includes('You allowed this fetch'), approved)
  assert.deepEqual(buttons(approved), [])

  const refused = draw({ ...asked(), decision: 'reject' }).markup
  assert.ok(refused.includes('You refused this fetch'), refused)
  assert.deepEqual(buttons(refused), [])
})

test('a fetch whose turn ended first cannot be answered', () => {
  // The question is no longer waiting, so a button here would send an approval of nothing. The
  // address and the host stay, because the record of what was asked is still true.
  const { markup } = draw({ ...asked(), interrupted: true })

  assert.deepEqual(buttons(markup), [])
  assert.ok(markup.includes('Nobody answered this'), markup)
  assert.ok(markup.includes('docs.example.com'), markup)
})

test('reaching a metadata service says what is being handed over', () => {
  const { markup } = draw(
    asked({
      url: 'http://169.254.169.254/latest/meta-data/',
      host: '169.254.169.254',
      ambient: [{ authority: 'metadata-service', named: '169.254.169.254' }],
    }),
  )

  assert.ok(markup.includes('spends access that is yours elsewhere'), markup)
  assert.ok(markup.includes('hands out the credentials of the role it runs as'), markup)
  assert.ok(markup.includes('class="confirm fetch spends"'), markup)
})

test('an ordinary host says nothing of the sort', () => {
  for (const ambient of [[], undefined]) {
    const { markup } = draw(asked({ ambient }))
    assert.ok(!markup.includes('spends access that is yours elsewhere'), markup)
    assert.ok(!markup.includes('spends"'), markup)
  }
})

test('only an answer to a fetch is drawn on a fetch', () => {
  const entries = [asked()]
  assert.equal(t.outstanding(entries).kind, 'fetch')

  for (const other of Object.keys(t.REPLY).filter((kind) => kind !== 'fetch')) {
    assert.equal(t.decide(entries, other, 7, 'approve')[0].decision, null, other)
  }
  assert.equal(t.decide(entries, 'fetch', 8, 'approve')[0].decision, null, 'another request')
  assert.equal(t.decide(entries, 'fetch', 7, 'approve')[0].decision, 'approve')
  assert.equal(t.outstanding(t.decide(entries, 'fetch', 7, 'reject')), null)
})

test('a turn ending leaves the fetch unanswerable and out of an export', () => {
  const ended = t.interruptPending([asked()])

  assert.equal(t.outstanding(ended), null)
  assert.equal(t.decide(ended, 'fetch', 7, 'approve')[0].decision, null)
  assert.deepEqual(t.conversation([asked()], true), [])
  assert.equal(t.plainText(asked()), null)
})

test('every question answered with a yes or a no has a method, an event and a way through', () => {
  // The table is where a kind is written down, and three other places have to agree with it for
  // an answer to arrive: the union that declares the entry, the reducer that draws the question
  // when the agent asks it, and the main process, which forwards no method it was not told of.
  const transcript = readFileSync('src/renderer/transcript.ts', 'utf8')
  const app = readFileSync('src/renderer/App.tsx', 'utf8')
  const main = readFileSync('src/main/index.ts', 'utf8')
  const allowed = main.slice(main.indexOf('const ALLOWED = new Set(['), main.indexOf('])', main.indexOf('const ALLOWED = new Set([')))

  assert.equal(t.REPLY.fetch, 'fetch.reply')
  for (const [kind, method] of Object.entries(t.REPLY)) {
    assert.equal(method, `${kind}.reply`)
    assert.ok(transcript.includes(`kind: '${kind}'`), `${kind} is not an entry`)
    assert.ok(app.includes(`case '${kind}.request':`), `${kind}.request is not drawn`)
    assert.ok(allowed.includes(`'${method}'`), `${method} is not forwarded to the agent`)
  }
})
