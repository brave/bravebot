// What the desktop's host-client window draws from the Rust view (HOSTCLIENT-2): the transcript
// made from rows, the card for a question this client may not answer, and what a refused send says.
// The window is drawn from state only, so the rows here are the ones the view holds, as the shared
// client delivers them, and the cards are rendered through `react-dom/server`.

import test from 'node:test'
import assert from 'node:assert/strict'
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

const { entriesFromRows, SETTLED } = load('src/renderer/prototype/entries.ts')
const { refusal, sentAnswer, trustOf } = load('src/renderer/prototype/PrototypeApp.tsx')
const { Row } = load('src/renderer/components/Transcript.tsx')
const { withSending, withAnswerOut, withDraft } = load('src/renderer/prototype/keyed.ts')

const confirm = {
  request: 4, path: 'out.txt', shownPath: 'out.txt', intent: 'create', untrusted: false, existing: false,
  added: 1, removed: 0, exact: true, changes: [{ kind: 'added', text: 'written' }], credentials: [], remark: null,
  writtenSinceCheckout: false, lineEndings: null, hiddenNote: null,
}
const rows = (resolved) => [
  { id: 1, turn: 1, kind: 'prompt', event: null, data: { kind: 'user', text: 'from the phone' }, resolved: false },
  { id: 2, turn: 1, kind: 'narration', event: 'narration', data: { text: 'looking' }, resolved: false },
  { id: 3, turn: 1, kind: 'approval', event: 'confirm.request', data: confirm, resolved },
]
const ALL = ['confirm', 'run', 'fetch', 'ask', 'trust']

test('a prompt another client sent is in the transcript from the view alone, with a stable key per row', () => {
  const entries = entriesFromRows(rows(false), { here: new Map(), answers: ALL })
  assert.deepEqual(entries.map((entry) => entry.kind), ['user', 'narration', 'confirm'])
  assert.equal(entries[0].text, 'from the phone')
  assert.deepEqual(entries.map((entry) => entry.id), ['row-1', 'row-2', 'row-3'])
  assert.deepEqual(entriesFromRows(rows(false), { here: new Map(), answers: ALL }).map((entry) => entry.id), entries.map((entry) => entry.id))
})

test('a question waiting that this client may answer is a card with buttons, and one it may not is drawn as one to answer elsewhere', () => {
  const waiting = entriesFromRows(rows(false), { here: new Map(), answers: ALL })[2]
  assert.equal(waiting.interrupted, undefined)
  const markup = renderToStaticMarkup(React.createElement(Row, { entry: waiting, onDecide() {}, onAnswer() {}, onFork() {}, forkable: false }))
  assert.match(markup, /Apply this change|Create this file/)

  const elsewhere = entriesFromRows(rows(false), { here: new Map(), answers: ['run'] })[2]
  assert.equal(elsewhere.interrupted, true)
  const drawn = renderToStaticMarkup(React.createElement(Row, { entry: elsewhere, onDecide() {}, onAnswer() {}, onFork() {}, forkable: false, interruptedNote: 'Answer this on another device' }))
  assert.match(drawn, /Answer this on another device/)
  assert.doesNotMatch(drawn, /Create this file/, 'no button to answer what this client may not')
})

test('a question that is no longer waiting says it was settled, unless this window answered it and then says what it did', () => {
  const settled = entriesFromRows(rows(true), { here: new Map(), answers: ALL })[2]
  assert.equal(settled.interrupted, true)
  assert.equal(settled.decision, null, 'no one is said to have approved it')
  const markup = renderToStaticMarkup(React.createElement(Row, { entry: settled, onDecide() {}, onAnswer() {}, onFork() {}, forkable: false, interruptedNote: SETTLED }))
  assert.match(markup, /No longer waiting/)
  assert.doesNotMatch(markup, /You approved/)

  const mine = entriesFromRows(rows(true), { here: new Map([[4, 'approve']]), answers: ALL })[2]
  assert.equal(mine.decision, 'approve')
  assert.equal(mine.interrupted, undefined)
})

test('a question waiting in a view that has ended is not offered, whatever the client may answer', () => {
  const entry = entriesFromRows(rows(false), { here: new Map(), answers: ALL, ended: true })[2]
  assert.equal(entry.interrupted, true)
})

test('what this window answered to the planner\'s questions is on its card once the question is settled', () => {
  const ask = { request: 6, questions: [{ header: 'Which', question: 'Which one?', options: [], multiple: false }] }
  const asked = [{ id: 1, turn: 1, kind: 'approval', event: 'ask.request', data: ask, resolved: true }]
  const typed = [{ typed: 'this one' }]
  const mine = entriesFromRows(asked, { here: new Map([[6, typed]]), answers: ALL })[0]
  assert.deepEqual(mine.answers, typed)
  assert.equal(mine.interrupted, undefined)
  assert.equal(entriesFromRows(asked, { here: new Map(), answers: ALL })[0].interrupted, true)
})

test('a row whose payload is not an object, and a kind this window cannot draw, are lines and not a failure', () => {
  const entries = entriesFromRows([
    { id: 1, turn: 1, kind: 'reply', event: 'turn.done', data: 'text', resolved: false },
    { id: 2, turn: 1, kind: 'approval', event: 'output.request', data: { request: 9 }, resolved: false },
    { id: 3, turn: 1, kind: 'prompt', event: null, data: { kind: 'user' }, resolved: false },
  ], { here: new Map(), answers: ALL })
  assert.deepEqual(entries.map((entry) => entry.kind), ['narration', 'narration', 'narration'])
})

test('the startup question is put only while the view says it is open and this client may answer it', () => {
  const asked = { directory: '/work', shownDirectory: '/work', shownKeeping: null, confinedLines: null }
  const session = (status, startupTrust = asked) => ({ id: 's1', startupTrust, view: { status } })
  assert.deepEqual(trustOf({ answers: ALL }, session('awaiting_trust')), { directory: '/work', keeping: null, confinedLines: null })
  // Another client that may answer it did, so the view moved on while this client still holds the question.
  assert.equal(trustOf({ answers: ALL }, session('idle')), null)
  assert.equal(trustOf({ answers: ['confirm'] }, session('awaiting_trust')), null, 'a client that may not answer it is not asked')
  assert.equal(trustOf({ answers: ALL }, session('awaiting_trust', null)), null)
  // The sentence about what a yes also grants is not drawn: this client's yes cannot say it was shown.
  const worded = { ...asked, confinedLines: 'A yes also lets a command run here without asking.' }
  assert.equal(trustOf({ answers: ALL }, session('awaiting_trust', worded)).confinedLines, null)
})

test('a refused send says why in words, and leaves the person to send it again', () => {
  assert.match(refusal({ ok: false, code: 'turn_in_flight', message: 'x' }), /already running.*stays here/)
  assert.match(refusal({ ok: false, code: 'no_such_request', message: 'x' }), /another client answered/)
  assert.equal(refusal({ ok: false, code: 'weird', message: 'the host said this' }), 'the host said this')
})

test('an answer to the planner\'s questions is sent in the one shape the host\'s client takes', () => {
  assert.deepEqual(sentAnswer({ chosen: [1] }), { chosen: [1] })
  assert.deepEqual(sentAnswer({ typed: 'x' }), { typed: 'x' })
  assert.equal(sentAnswer({}), null)
})

test('what was typed, and what was sent and not yet answered, is kept for each session apart from the others', () => {
  let drafts = new Map()
  drafts = withDraft(drafts, 's1', () => 'a note for the first')
  drafts = withDraft(drafts, 's2', () => 'a note for the second')
  drafts = withDraft(drafts, 's1', (now) => `${now}, and more`)
  assert.equal(drafts.get('s1'), 'a note for the first, and more')
  assert.equal(drafts.get('s2'), 'a note for the second')

  let sending = new Set()
  sending = withSending(sending, 's1', true)
  assert.deepEqual([...sending], ['s1'], 'a message out is remembered with its session')
  assert.deepEqual([...withSending(sending, 's2', true)].sort(), ['s1', 's2'])
  assert.deepEqual([...withSending(sending, 's1', false)], [])

  let out = new Map()
  out = withAnswerOut(out, 's1', 1, true)
  out = withAnswerOut(out, 's2', 1, true)
  assert.deepEqual([...out.get('s1')], [1], 'the same question number in two sessions is two questions')
  out = withAnswerOut(out, 's1', 1, false)
  assert.deepEqual([...out.get('s1')], [])
  assert.deepEqual([...out.get('s2')], [1])
})
