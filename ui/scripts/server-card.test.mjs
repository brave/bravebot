// What the language server card puts in front of a person.
//
// [LSP-5](../../docs/specs/tools/lsp.md#LSP-5) says starting a server is put to the person, and
// that what is being approved is said plainly at the prompt: a process that runs for the session
// with their own access, which for some languages runs code out of the dependency tree. This
// window is the second surface that asks, so it owes the same plain statement, and the same
// absence: no answer here is a claim about what the server reports.
//
// The card is rendered through `react-dom/server` and the markup is what is asserted on, because
// what a person is told and what they can press are properties of the markup.

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

const { Row } = load('src/renderer/components/Transcript.tsx')
const t = load('src/renderer/transcript.ts')

const draw = (entry) =>
  renderToStaticMarkup(
    React.createElement(Row, { entry, onDecide() {}, onAnswer() {}, onFork() {}, forkable: false }),
  )

/** The question as the bridge sends it. */
const asked = (overrides = {}) =>
  t.askedServer({
    request: 9,
    language: 'Rust',
    program: '/home/someone/.cargo/bin/rust-analyzer',
    workspace: '/home/someone/project',
    runsBuildTooling: true,
    summary: 'start the Rust language server',
    ...overrides,
  })

const buttons = (markup) => [...markup.matchAll(/<button[^>]*>(.*?)<\/button>/g)].map((found) => found[1])

test('the card says what would run and what it would read', () => {
  const markup = draw(asked())

  assert.ok(markup.includes('Rust language server'), markup)
  assert.match(markup, /Program:<\/strong> <code>\/home\/someone\/\.cargo\/bin\/rust-analyzer<\/code>/)
  assert.match(markup, /Indexes:<\/strong> <code>\/home\/someone\/project<\/code>/)
})

test('a server that runs build tooling says that code from dependencies will run', () => {
  const markup = draw(asked({ runsBuildTooling: true }))

  assert.ok(markup.includes('code from your dependencies runs with your own access'), markup)
  assert.ok(markup.includes('It is not confined'), markup)
  assert.ok(markup.includes('class="confirm server builds"'), markup)
  // The two statements are about different servers, and one that builds is not one that only reads.
  assert.ok(!markup.includes('Nothing is written to your project'), markup)
})

test('a server that only reads does not claim that anything is built', () => {
  const markup = draw(asked({ runsBuildTooling: false }))

  assert.ok(markup.includes('Nothing is written to your project'), markup)
  assert.ok(!markup.includes('code from your dependencies'), markup)
  assert.ok(!markup.includes('builds"'), markup)
})

test('the card says how long a yes lasts and what it does not grant', () => {
  for (const runsBuildTooling of [true, false]) {
    const markup = draw(asked({ runsBuildTooling }))
    assert.ok(markup.includes('stays running for this conversation and stops when the conversation closes'), markup)
    assert.ok(markup.includes('stays on the same footing however you answer'), markup)
  }
})

test('a waiting server offers a refusal and a start for the conversation, and nothing standing', () => {
  const markup = draw(asked())

  assert.deepEqual(buttons(markup), ['Don’t start', 'Start for this conversation'])
  assert.ok(!markup.includes('always'), markup)
})

test('a server that was answered says which way, and offers nothing further', () => {
  const approved = draw({ ...asked(), decision: 'approve' })
  assert.ok(approved.includes('You started this server for the conversation'), approved)
  assert.deepEqual(buttons(approved), [])

  const refused = draw({ ...asked(), decision: 'reject' })
  assert.ok(refused.includes('You refused this server'), refused)
  assert.deepEqual(buttons(refused), [])
})

test('a server whose turn ended first cannot be started from its card', () => {
  // The question is no longer waiting, so a button here would be a yes nothing receives. What
  // would have run stays on the card, and so does the warning: ending a turn changes whether a
  // question can be answered and not what it was about.
  const markup = draw({ ...asked(), interrupted: true })

  assert.deepEqual(buttons(markup), [])
  assert.ok(markup.includes('Nobody answered this'), markup)
  assert.ok(markup.includes('/home/someone/.cargo/bin/rust-analyzer'), markup)
  assert.ok(markup.includes('code from your dependencies runs with your own access'), markup)
})

test('nothing on the card is something a click follows or a load fetches', () => {
  const markup = draw(asked())

  for (const forbidden of ['<a ', 'href=', 'src=', '<img', '<iframe']) {
    assert.ok(!markup.includes(forbidden), `drew ${forbidden}: ${markup}`)
  }
})

test('only an answer to a server is drawn on a server', () => {
  const entries = [asked()]
  assert.equal(t.outstanding(entries).kind, 'server')
  assert.equal(t.REPLY.server, 'server.reply')

  for (const other of Object.keys(t.REPLY).filter((kind) => kind !== 'server')) {
    assert.equal(t.decide(entries, other, 9, 'approve')[0].decision, null, other)
  }
  assert.equal(t.decide(entries, 'server', 10, 'approve')[0].decision, null, 'another request')
  assert.equal(t.decide(entries, 'server', 9, 'approve')[0].decision, 'approve')
})

test('a turn ending leaves the server unanswerable and out of an export', () => {
  const ended = t.interruptPending([asked()])

  assert.equal(t.outstanding(ended), null)
  assert.equal(t.decide(ended, 'server', 9, 'approve')[0].decision, null)
  assert.deepEqual(t.conversation([asked()], true), [])
  assert.equal(t.plainText(asked()), null)
})
