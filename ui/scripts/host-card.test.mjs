// What the window shows when a confined program asked for hosts the allowed-hosts list does not
// cover.
//
// [SANDBOX-24](../../docs/specs/sandboxing.md#SANDBOX-24) says the person is asked, shown each
// host, and that a yes covers the rest of the session and writes no rule.
//
// The card is rendered through `react-dom/server` and the markup is what is asserted on.

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

const draw = (entry) =>
  renderToStaticMarkup(
    React.createElement(Row, { entry, onDecide() {}, onAnswer() {}, onFork() {}, forkable: false }),
  )

const HOSTS = ['registry.example.com', '10.0.0.1']

const asked = (overrides = {}) => t.askedHost({ request: 8, hosts: HOSTS, ...overrides })

const buttons = (markup) => [...markup.matchAll(/<button[^>]*>(.*?)<\/button>/g)].map((found) => found[1])

test('the card lists every host as the program named it, one to a row', () => {
  const markup = draw(asked())

  assert.ok(markup.includes('2 hosts'), markup)
  const drawn = [...markup.matchAll(/<li><code>(.*?)<\/code><\/li>/g)].map((found) => found[1])
  assert.deepEqual(drawn, HOSTS)
})

test('one host is counted as one', () => {
  const markup = draw(asked({ hosts: [HOSTS[0]] }))

  assert.ok(markup.includes('1 host<'), markup)
  assert.ok(markup.includes('this host'), markup)
})

test('the card says how long a yes lasts and what still refuses', () => {
  const markup = draw(asked())

  assert.ok(markup.includes('until this conversation ends'), markup)
  assert.ok(markup.includes('unless sandbox.network.deniedHosts names'), markup)
  assert.ok(markup.includes('Nothing is saved'), markup)
})

test('a waiting question offers to keep the hosts blocked or allow them, and nothing standing', () => {
  const markup = draw(asked())

  assert.deepEqual(buttons(markup), ['Keep them blocked', 'Allow for this session'])
  assert.ok(!markup.includes('always'), markup)
})

test('a question that was answered says which way, and offers nothing further', () => {
  const allowed = draw({ ...asked(), decision: 'approve' })
  assert.ok(allowed.includes('You allowed these hosts for this session'), allowed)
  assert.deepEqual(buttons(allowed), [])

  const blocked = draw({ ...asked(), decision: 'reject' })
  assert.ok(blocked.includes('You kept these hosts blocked'), blocked)
  assert.deepEqual(buttons(blocked), [])
})

test('a question whose turn ended first cannot be answered from its card', () => {
  const markup = draw({ ...asked(), interrupted: true })

  assert.deepEqual(buttons(markup), [])
  assert.ok(markup.includes('Nobody answered this'), markup)
  for (const host of HOSTS) assert.ok(markup.includes(host), 'the hosts stay on the card')
})

test('a host is drawn as text and never as markup', () => {
  const forged = '<img src="https://example.com/x.png"><a href="https://example.com">x</a>'
  const markup = draw(asked({ hosts: [forged] }))

  for (const forbidden of ['<img', '<a ', '<a>']) {
    assert.ok(!markup.includes(forbidden), `drew ${forbidden}: ${markup}`)
  }
})

test('only an answer to this question is drawn on it', () => {
  const entries = [asked()]
  assert.equal(t.outstanding(entries).kind, 'host')

  for (const other of Object.keys(t.REPLY).filter((kind) => kind !== 'host')) {
    assert.equal(t.decide(entries, other, 8, 'approve')[0].decision, null, other)
  }
  assert.equal(t.decide(entries, 'host', 9, 'approve')[0].decision, null, 'another request')
  assert.equal(t.decide(entries, 'host', 8, 'approve')[0].decision, 'approve')
})

test('the window sends the answer on a channel the main process lets through', () => {
  assert.equal(t.REPLY.host, 'host.reply')

  // The renderer may call only the methods the main process lists, so a reply channel missing
  // from the list is an answer that is dropped and a turn that waits on it.
  const main = readFileSync('src/main/index.ts', 'utf8')
  const start = main.indexOf('const ALLOWED = new Set([')
  const end = main.indexOf('])', start)
  assert.ok(start >= 0 && end > start, 'the allow-list is not where this test looks for it')
  const allowed = new Set([...main.slice(start, end).matchAll(/'([a-z.-]+)'/g)].map((found) => found[1]))
  assert.ok(allowed.has('host.reply'), 'host.reply is not in the allow-list')
})
