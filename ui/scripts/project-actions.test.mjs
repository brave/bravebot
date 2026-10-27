// What the window draws for a project's import list, and what it says after a purge.
//
// An imported title is the first thing a person typed into another program, so it is that program's
// text: the list draws it as text. The list is rendered through `react-dom/server` and the markup is
// asserted on, because whether a title became an element is a property of the markup. A purge
// that left records behind is reported as an error, never as a toast that says it worked.
import test from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { buildSync } from 'esbuild'

const require = createRequire(import.meta.url)
const React = require('react')
const { renderToStaticMarkup } = require('react-dom/server')

function load(path) {
  const source = buildSync({
    entryPoints: [path], bundle: true, write: false, platform: 'node', format: 'cjs', jsx: 'automatic',
    external: ['react', 'react-dom', 'react/jsx-runtime'],
  }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
}

const { ImportList, afterCopy } = load('src/renderer/components/ImportSessions.tsx')
const { purgeReport, turnRunningIn } = load('src/renderer/purge.ts')

const found = (sourceId, title, imported = false) => ({ sourceId, id: `claude-code-${sourceId}`, title, updated: 1759752002, imported })
const draw = (rows, chosen = new Set()) =>
  renderToStaticMarkup(React.createElement(ImportList, { found: rows, chosen, copying: false, onPick() {} }))

test('a title holding markup is drawn as the characters it is', () => {
  const markup = draw([found('a', '<img src=x onerror=alert(1)>'), found('b', '**bold** and `code`')])
  assert.ok(!/<img/i.test(markup), `a title became an element: ${markup}`)
  assert.ok(markup.includes('&lt;img src=x onerror=alert(1)&gt;'), markup)
  assert.ok(!/<(strong|b|em|code)\b/.test(markup), `a title was read as Markdown: ${markup}`)
  assert.ok(markup.includes('**bold** and `code`'), markup)
})

// Leo sets a checkbox's state on the element once it mounts, so whether it is ticked and shut is
// held by drive-project-actions.mjs in a real window, and only the mark is asserted here.
test('a session copied before is marked Imported, and the others are not', () => {
  const markup = draw([found('old', 'copied before', true), found('new', 'not yet')])
  const rows = markup.split('<li').slice(1)
  assert.equal(rows.length, 2)
  assert.match(rows[0], /data-imported="true"/)
  assert.match(rows[0], /<leo-label>Imported<\/leo-label>/)
  assert.doesNotMatch(rows[1], /data-imported|Imported/)
})

test('a purge that kept any named chat is an error, and one that kept none is a toast', () => {
  assert.deepEqual(purgeReport({ deleted: ['a', 'b'], failed: [], held: [] }), { done: 'Deleted 2 chats' })
  assert.deepEqual(purgeReport({ deleted: ['a'], failed: [], held: [] }), { done: 'Deleted 1 chat' })
  assert.deepEqual(purgeReport({ deleted: ['a'], failed: ['b'], held: [] }), { problem: '1 chat deleted. 1 chat could not be deleted and was kept.' })
  assert.deepEqual(purgeReport({ deleted: [], failed: ['b'], held: ['c'] }), { problem: '0 chats deleted. 1 chat could not be deleted and was kept. 1 chat still in use by a background session was kept.' })
  assert.deepEqual(purgeReport({ deleted: ['a'], failed: [], held: ['c', 'd'] }), { problem: '1 chat deleted. 2 chats still in use by a background session were kept.' })
})

test('a purge is refused before anything closes while a chat of the project is answering', () => {
  const open = (directory, running) => ({ summary: { directory }, running })
  assert.equal(turnRunningIn([open('/p', false), open('/q', true)], '/p'), false)
  assert.equal(turnRunningIn([open('/p', false), open('/p', true)], '/p'), true)
  assert.equal(turnRunningIn([], '/p'), false)
})

test('after a copy that partly failed, what went in is marked imported and no longer chosen', () => {
  const rows = [found('a', 'one'), found('b', 'two'), found('c', 'three')]
  const after = afterCopy(rows, new Set(['a', 'b', 'c']), {
    copied: ['claude-code-a'], already: ['claude-code-b'], failed: [{ sourceId: 'c', message: 'disk full' }],
  })
  assert.deepEqual(after.found.map((one) => one.imported), [true, true, false])
  assert.deepEqual([...after.chosen], ['c'])
  assert.equal(rows[0].imported, false, 'the list it was given was changed')
})
