// What the window draws for a name that would draw as nothing, or as another name.
//
// [PROMPT-1](../../docs/specs/prompting.md#PROMPT-1) says a person cannot endorse a name they were
// not shown. The agent spells such a name once, in `shownPath`, and every card and list that names
// a path draws that field. A card that drew `path` would show `note.md ` as `note.md`.
//
// The cards are rendered through `react-dom/server` and the markup is what is asserted on.

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
const { PathPermissions } = load('src/renderer/components/Permissions.tsx')
const t = load('src/renderer/transcript.ts')

const draw = (entry) =>
  renderToStaticMarkup(
    React.createElement(Row, { entry, onDecide() {}, onAnswer() {}, onFork() {}, forkable: false }),
  )

// The name as the planner wrote it, and the form the agent sends for a person to read.
const WRITTEN = 'note.md '
const SPELLED = '"note.md␣"'

// The same name as markup holds it, which writes a quotation mark in text or an attribute as an entity.
const SPELLED_MARKUP = '&quot;note.md␣&quot;'
const BLANK_MARKUP = '&quot;␣&quot;'
const ROOT_MARKUP = '&quot;&quot;'

const write = (overrides = {}) =>
  t.asked({
    request: 1, path: 'note.md', shownPath: 'note.md', intent: 'create', untrusted: false, existing: false,
    added: 1, removed: 0, exact: true, changes: [{ kind: 'added', text: 'a line' }], ...overrides,
  })

const exposure = (overrides = {}) =>
  t.askedExposure({
    request: 2, path: 'note.md', shownPath: 'note.md', credentials: ['an API key at line 1'],
    summary: 'let the model read note.md, which holds 1 credential', ...overrides,
  })

const vouch = (overrides = {}) =>
  t.askedVouch({
    request: 3, path: 'note.md', shownPath: 'note.md', rule: '/work/note.md', shownRule: '/work/note.md',
    preview: 'a line', truncated: false, ...overrides,
  })

const spelled = { path: WRITTEN, shownPath: SPELLED }

test('a write card draws the name as spelled, and does not draw the name as written', () => {
  const markup = draw(write(spelled))
  assert.ok(markup.includes(`<code class="path" data-tooltip="${SPELLED_MARKUP}">${SPELLED_MARKUP}</code>`), markup)
  assert.ok(!markup.includes(`>${WRITTEN}<`), markup)
})

test('a write card that has been answered still folds to the name as spelled', () => {
  const markup = draw({ ...write(spelled), decision: 'approve' })
  assert.ok(markup.includes(SPELLED_MARKUP), markup)
  assert.ok(!markup.includes(`${WRITTEN}</code>`), markup)
})

test('a credential-read card draws the name as spelled', () => {
  const markup = draw(exposure(spelled))
  assert.ok(markup.includes(`<code class="path">${SPELLED_MARKUP}</code>`), markup)
  assert.ok(!markup.includes(`>${WRITTEN}<`), markup)
})

test('a vouch card that has been answered still folds to the name as spelled', () => {
  const markup = draw({ ...vouch(spelled), decision: 'approve' })
  assert.ok(markup.includes(SPELLED_MARKUP), markup)
  assert.ok(!markup.includes(`${WRITTEN}</code>`), markup)
})

test('the permissions list draws each rule as spelled, and the root as the project root', () => {
  const markup = renderToStaticMarkup(React.createElement(PathPermissions, {
    paths: [
      { path: '', shownPath: '""', integrity: 'trusted' },
      { path: WRITTEN, shownPath: SPELLED, integrity: 'trusted' },
      { path: ' ', shownPath: '"␣"', integrity: 'untrusted' },
    ],
    busy: false,
    onRevoke() {},
  }))
  assert.ok(markup.includes(`<code>${SPELLED_MARKUP}</code>`), markup)
  assert.ok(markup.includes(`<code>${BLANK_MARKUP}</code>`), markup)
  assert.ok(markup.includes('<code>Project root</code>'), markup)
  assert.ok(!markup.includes(`<code>${ROOT_MARKUP}</code>`), markup)
  assert.ok(!markup.includes(`<code>${WRITTEN}</code>`), markup)
  assert.ok(!markup.includes('<code> </code>'), markup)
})
