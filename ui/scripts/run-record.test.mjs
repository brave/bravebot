// How a saved manifest run is drawn when it is read back.
//
// [MANIFEST-11](../../docs/specs/manifest.md#MANIFEST-11) says a run's record has no conversation
// and cannot be picked up. So the window reads one and offers no way to type into it.
//
// The view is rendered through `react-dom/server` and the markup is what is asserted on.

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

const { RunRecord } = load('src/renderer/components/RunRecord.tsx')

/** A saved run, as `manifest.read` answers. */
const saved = (manifest = {}, record = {}) => ({
  record: {
    id: 'run-123', directory: '/home/someone/project', branch: 'main', title: 'write the release notes',
    started: 1, updated: 1, turns: 1, tokens: 1234, build: '0.11.0', front: 'desktop', ...record,
  },
  model: 'a-model',
  manifest: {
    goal: 'Write the notes from the changelog.',
    proposed: '{"steps": []}',
    plan: 'Plan, fixed before anything runs:\n  1. [act] write NOTES.md\n',
    steps: ['1. [act] write NOTES.md: new file, 1 line'],
    failure: null,
    ...manifest,
  },
})

const draw = (run, onNew = () => {}) => renderToStaticMarkup(React.createElement(RunRecord, { run, onNew }))
const buttons = (markup) => [...markup.matchAll(/<button[^>]*>(.*?)<\/button>/g)].map((found) => found[1])

test('a saved run says it is read only and offers nothing to type into', () => {
  const markup = draw(saved())

  assert.ok(markup.includes('Plan run · read only'), markup)
  assert.ok(markup.includes('it cannot be continued'), markup)
  for (const control of ['<textarea', '<input', 'contenteditable']) {
    assert.ok(!markup.includes(control), `drew ${control}: ${markup}`)
  }
  assert.deepEqual(buttons(markup), ['New chat here'])
})

test('a saved run shows the task, the goal, the plan and the steps that ran', () => {
  const markup = draw(saved())

  assert.ok(markup.includes('write the release notes'), markup)
  assert.ok(markup.includes('Write the notes from the changelog.'), markup)
  assert.ok(markup.includes('1. [act] write NOTES.md\n'), markup)
  assert.ok(markup.includes('write NOTES.md: new file, 1 line'), markup)
})

test('a run that finished says so, and says its result was not saved', () => {
  const markup = draw(saved({ failure: null }))

  assert.ok(markup.includes('The run finished.'), markup)
  assert.ok(markup.includes('was not saved'), markup)
  assert.ok(!markup.includes('The run stopped.'), markup)
})

test('a run that stopped says why, and says when no step ran', () => {
  const markup = draw(saved({ failure: 'the plan was not approved, so nothing ran', steps: [] }))

  assert.ok(markup.includes('The run stopped.'), markup)
  assert.ok(markup.includes('the plan was not approved, so nothing ran'), markup)
  assert.ok(markup.includes('No step ran.'), markup)
  assert.ok(!markup.includes('The run finished.'), markup)
})

test('a plan that could not be used shows what the planner proposed', () => {
  const markup = draw(saved({ plan: null, proposed: 'Sure! Here is the plan', failure: 'the manifest is not JSON', steps: [] }))

  assert.ok(markup.includes('What the planner proposed, which could not be used'), markup)
  assert.ok(markup.includes('Sure! Here is the plan'), markup)
})

test('what a record holds is drawn as text and never as markup', () => {
  const forged = '<img src="https://example.com/x.png"><a href="https://example.com">x</a><b>bold</b>'
  const markup = draw(saved({ goal: forged, plan: forged, steps: [forged], failure: forged }, { title: forged }))

  // An element, not the characters of one: the text is drawn escaped, so it still holds the
  // words `href` and `src`, and what must not exist is a tag made from them.
  for (const forbidden of ['<img', '<a ', '<a>', '<b>']) {
    assert.ok(!markup.includes(forbidden), `drew ${forbidden}: ${markup}`)
  }
  assert.ok(markup.includes('&lt;img src=&quot;https://example.com/x.png&quot;&gt;'), markup)
  assert.ok(markup.includes('&lt;b&gt;bold&lt;/b&gt;'), markup)
})

test('starting again names the project the run was in', () => {
  // The button is given the record's own directory. A static render cannot press it, so the
  // handler the component builds is read from its source.
  const source = readFileSync('src/renderer/components/RunRecord.tsx', 'utf8')
  assert.match(source, /onClick=\{\(\) => onNew\(record\.directory\)\}/)
})

test('a run is read and never opened as a session', () => {
  // `session.open` makes a session, and the bridge refuses one for a run. The window asks for
  // `manifest.read` before it looks at anything else about the row.
  const app = readFileSync('src/renderer/App.tsx', 'utf8')
  const show = app.slice(app.indexOf('const showSession = useCallback('))
  const reads = show.indexOf("call<RunRecord>('manifest.read'")
  const opens = show.indexOf("call<OpenedSession>('session.open'")
  assert.ok(reads > 0 && opens > 0, 'the two calls are not where this test looks for them')
  assert.ok(reads < opens, 'a row is opened before it is asked whether it is a run')
  assert.ok(show.slice(0, reads).includes('if (summary.manifest) {'), 'the read is not what a run row does')

  const main = readFileSync('src/main/index.ts', 'utf8')
  const allowed = main.slice(main.indexOf('const ALLOWED = new Set(['), main.indexOf('])', main.indexOf('const ALLOWED = new Set([')))
  assert.ok(allowed.includes("'manifest.read'"), 'manifest.read is not forwarded to the agent')
})
