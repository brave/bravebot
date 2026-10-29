// A long prompt folds so the reply stays on screen. Copy and Fork stay outside the fold.

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

const { Row, runSummary } = load('src/renderer/components/Transcript.tsx')
const t = load('src/renderer/transcript.ts')

const draw = (entry, revealed = false) =>
  renderToStaticMarkup(
    React.createElement(Row, { entry, onDecide() {}, onAnswer() {}, onFork() {}, forkable: true, revealed }),
  )

const labels = (markup) => [...markup.matchAll(/<(?:leo-)?button[^>]*>(.*?)<\/(?:leo-)?button>/g)].map((found) => found[1])

test('a short prompt is shown in full and has no fold', () => {
  const markup = draw(t.userSaid('Refactor the cache.'))
  assert.ok(markup.includes('Refactor the cache.'), markup)
  assert.ok(!markup.includes('folded'), markup)
  assert.ok(!labels(markup).includes('Show more'), markup)
  assert.ok(markup.includes('Copy message') || markup.includes('aria-label="Fork from here"'), markup)
})

test('a long prompt folds behind Show more, and Copy and Fork stay outside the fold', () => {
  const text = 'Please review this.\n'.repeat(20)
  const markup = draw(t.userSaid(text))
  assert.ok(markup.includes('bubble-text folded'), markup)
  assert.ok(labels(markup).includes('Show more'), markup)
  const foldAt = markup.indexOf('folded')
  const copyAt = markup.indexOf('message-actions')
  assert.ok(foldAt >= 0 && copyAt > foldAt, 'Copy and Fork must sit after the folded text')
})

test('a long prompt that find has opened is expanded', () => {
  const markup = draw(t.userSaid('Please review this.\n'.repeat(20)), true)
  assert.ok(!markup.includes('bubble-text folded'), markup)
  assert.ok(labels(markup).includes('Show less'), markup)
})

test('a tool-run title leads with the verbs it counted', () => {
  const read = (n) => ({ kind: 'tool', id: `r${n}`, activity: { verb: 'Read', shownTarget: `f${n}`, note: 'ok', failed: false, waitedSeconds: 0, why: null } })
  const edit = { kind: 'tool', id: 'e1', activity: { verb: 'Edit', shownTarget: 'g', note: 'ok', failed: false, waitedSeconds: 0, why: null } }
  assert.equal(runSummary([read(1), read(2), read(3), edit]), 'Read 3, Edit 1')
  assert.equal(runSummary([{ kind: 'user', id: 'u', text: 'hi' }]), 'Worked through 1 step')
})
