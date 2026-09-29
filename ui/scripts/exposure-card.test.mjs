// What the window shows when a read would expose a credential.
//
// [CRED-15](../../docs/specs/credential-protection.md#CRED-15) says the person is asked, told the
// path and the finding, and never the value. It says an answer covers the file for the session
// and writes no rule. These tests hold the card to that.
//
// The card is rendered through `react-dom/server` and the markup is what is asserted on.

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

const FINDINGS = ['an AWS access key id at .env:1, AKIA…MPLE', 'a private key at .env:4, ----…----']

/** The question as the bridge sends it. */
const asked = (overrides = {}) =>
  t.askedExposure({
    request: 6,
    path: '.env',
    credentials: FINDINGS,
    summary: 'let the model read .env, which holds 2 credentials',
    ...overrides,
  })

const buttons = (markup) => [...markup.matchAll(/<button[^>]*>(.*?)<\/button>/g)].map((found) => found[1])

test('the card names the file and every finding, as the agent wrote them', () => {
  const markup = draw(asked())

  assert.ok(markup.includes('<code class="path">.env</code>'), markup)
  assert.ok(markup.includes('2 findings'), markup)
  const drawn = [...markup.matchAll(/<li><code>(.*?)<\/code><\/li>/g)].map((found) => found[1])
  assert.deepEqual(drawn, FINDINGS)
})

test('the card says that sending the file discloses the value', () => {
  const markup = draw(asked())

  assert.ok(markup.includes('goes to whoever performs inference'), markup)
  assert.ok(markup.includes('Sending the file discloses that value'), markup)
  assert.ok(markup.includes('without any of the value'), markup)
})

test('the card says how long an answer lasts and what it does not change', () => {
  const markup = draw(asked())

  assert.ok(markup.includes('covers this file until this conversation closes'), markup)
  assert.ok(markup.includes('It is not saved'), markup)
  assert.ok(markup.includes('does not change whether the file is trusted'), markup)
})

test('a waiting question offers to keep the file back or send it, and nothing standing', () => {
  const markup = draw(asked())

  // Keeping it back is first, as the refusal is on every card.
  assert.deepEqual(buttons(markup), ['Keep it back', 'Send it anyway'])
  assert.ok(!markup.includes('always'), markup)
})

test('a question that was answered says which way, and offers nothing further', () => {
  const sent = draw({ ...asked(), decision: 'approve' })
  assert.ok(sent.includes('You sent this file to the model'), sent)
  assert.deepEqual(buttons(sent), [])

  const kept = draw({ ...asked(), decision: 'reject' })
  assert.ok(kept.includes('You kept this file back'), kept)
  assert.deepEqual(buttons(kept), [])
})

test('a question whose turn ended first cannot be answered from its card', () => {
  const markup = draw({ ...asked(), interrupted: true })

  assert.deepEqual(buttons(markup), [])
  assert.ok(markup.includes('Nobody answered this'), markup)
  assert.ok(markup.includes(FINDINGS[0]), 'the findings stay on the card')
})

test('one finding is counted as one', () => {
  const markup = draw(asked({ credentials: [FINDINGS[0]] }))

  assert.ok(markup.includes('1 finding<'), markup)
})

test('a finding is drawn as text and never as markup', () => {
  const forged = '<img src="https://example.com/x.png"><a href="https://example.com">x</a>'
  const markup = draw(asked({ path: forged, credentials: [forged] }))

  for (const forbidden of ['<img', '<a ', '<a>']) {
    assert.ok(!markup.includes(forbidden), `drew ${forbidden}: ${markup}`)
  }
  assert.ok(markup.includes('&lt;img src=&quot;https://example.com/x.png&quot;&gt;'), markup)
})

test('only an answer to this question is drawn on it', () => {
  const entries = [asked()]
  assert.equal(t.outstanding(entries).kind, 'exposure')
  assert.equal(t.REPLY.exposure, 'exposure.reply')

  for (const other of Object.keys(t.REPLY).filter((kind) => kind !== 'exposure')) {
    assert.equal(t.decide(entries, other, 6, 'approve')[0].decision, null, other)
  }
  assert.equal(t.decide(entries, 'exposure', 7, 'approve')[0].decision, null, 'another request')
  assert.equal(t.decide(entries, 'exposure', 6, 'approve')[0].decision, 'approve')
})

test('a finding is left out of an export and cannot be copied as a message', () => {
  // A finding is a record of where a credential is. It stays on the card it was drawn on.
  assert.deepEqual(t.conversation([asked()], true), [])
  assert.equal(t.plainText(asked()), null)
})
