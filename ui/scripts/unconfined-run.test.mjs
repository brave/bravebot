// The run card for a line the planner asked to run with no sandbox (SANDBOX-29, PROMPT-2).
//
// The bridge sends `unconfined`, and the two sentences the terminal says for such a line, worded
// from its catalog. The card draws them as sent, keeps no copy of its own, and offers no standing
// answer, since nothing about such a line is remembered. A run without the flag draws neither.

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
  renderToStaticMarkup(React.createElement(Row, { entry, onDecide() {}, onAnswer() {}, onFork() {}, forkable: false }))

// Words the window holds nowhere, so finding them proves the card drew the bridge's text.
const UNCONFINED_NOTE = 'zzbridge: the planner asked for this one line to run with no sandbox'
const NOT_REMEMBERED_NOTE = 'zzbridge: neither the line nor the request is remembered'
const NOT_SANDBOXED = 'zzbridge: this is not sandboxed'

const request = (unconfined) => ({
  request: 1,
  line: 'make release',
  plan: '/usr/bin/make release',
  stages: [{ program: 'make', resolved: '/usr/bin/make', shownBinary: '/usr/bin/make', args: ['release'], display: 'make release' }],
  directory: '/home/someone/project',
  shownDirectory: '/home/someone/project',
  releasesPrivate: false,
  ambient: [],
  confinement: null,
  notSandboxed: NOT_SANDBOXED,
  unconfined,
  unconfinedNote: unconfined ? UNCONFINED_NOTE : null,
  notRememberedNote: unconfined ? NOT_REMEMBERED_NOTE : null,
  canBeRemembered: !unconfined,
  vouches: [{ program: '/usr/bin/make', args: ['release'], display: 'make release' }],
  summary: 'run 1 step in /home/someone/project',
})

const element = (markup, name) => {
  const found = markup.match(new RegExp(`<(\\w+)[^>]*data-test="${name}"[^>]*>([\\s\\S]*?)</\\1>`))
  return found && { attributes: found[0].slice(0, found[0].indexOf('>')), body: found[2] }
}

test('an unconfined run draws the bridge’s warning and not-remembered sentence, as rows the approval waits on', () => {
  const drawn = draw(t.askedRun(request(true)))

  const warning = element(drawn, 'unconfined')
  assert.ok(warning, drawn)
  assert.match(warning.attributes, /data-deciding="all"/)
  assert.ok(warning.body.includes(UNCONFINED_NOTE), warning.body)
  assert.ok(warning.body.startsWith('<leo-alert'), warning.body)

  const notRemembered = element(drawn, 'not-remembered')
  assert.ok(notRemembered, drawn)
  assert.match(notRemembered.attributes, /class="permission-scope"/)
  assert.match(notRemembered.attributes, /data-deciding="all"/)
  assert.equal(notRemembered.body, NOT_REMEMBERED_NOTE)

  assert.ok(element(drawn, 'not-sandboxed')?.body.includes(NOT_SANDBOXED), drawn)
  assert.equal(drawn.split(UNCONFINED_NOTE).length, 2, 'the warning is drawn once')
})

test('an unconfined run offers no standing answer', () => {
  const drawn = draw(t.askedRun(request(true)))
  assert.ok(!drawn.includes('Trust command and output'), drawn)
  assert.ok(!drawn.includes('Remembered approval:'), drawn)
  assert.ok(drawn.includes('Run once'), drawn)
  assert.ok(drawn.includes('Don’t run'), drawn)
})

test('a run without the flag draws neither sentence', () => {
  const drawn = draw(t.askedRun(request(false)))
  assert.equal(element(drawn, 'unconfined'), null, drawn)
  assert.equal(element(drawn, 'not-remembered'), null, drawn)
  assert.ok(!drawn.includes('zzbridge: the planner'), drawn)
  assert.ok(!drawn.includes('zzbridge: neither'), drawn)
  assert.ok(drawn.includes('Trust command and output'), drawn)
})

test('a flag with no sentences sent draws no empty warning', () => {
  const drawn = draw(t.askedRun({ ...request(true), unconfinedNote: null, notRememberedNote: null }))
  assert.equal(element(drawn, 'unconfined'), null, drawn)
  assert.equal(element(drawn, 'not-remembered'), null, drawn)
})

test('a cancelled unconfined card can still be found by its warning', () => {
  assert.ok(t.searchableText(t.askedRun(request(true))).includes(UNCONFINED_NOTE))
})
