// The context-column panels a person has turned off, as kept in `bravebot-ui.json` under `hidden`
// (PANEL-20): what `parseHidden` accepts, that `parseState` carries the key, and that writing
// another key leaves it in the file.

import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'
import { mkdtempSync, readFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

const require = createRequire(import.meta.url)

function load(path, stubs = {}) {
  const source = buildSync({
    entryPoints: [path],
    bundle: true,
    write: false,
    platform: 'node',
    format: 'cjs',
    external: Object.keys(stubs),
  }).outputFiles[0].text
  const module = { exports: {} }
  const resolve = (name) => (name in stubs ? stubs[name] : require(name))
  new Function('require', 'module', 'exports', source)(resolve, module, module.exports)
  return module.exports
}

const { HIDEABLE, parseHidden, toggleHidden } = load('src/shared/panels.ts')
const { PANEL_NAMES, parseState } = load('src/shared/state.ts')

test('the hideable panels are the Overview panels, in column order', () => {
  assert.deepEqual([...HIDEABLE], PANEL_NAMES.filter((name) => name !== 'files'))
})

test('hidden panels come back in column order', () => {
  assert.deepEqual(parseHidden(['notes', 'plan']), ['plan', 'notes'])
  assert.deepEqual(parseHidden(['asides', 'writes', 'read']), ['read', 'writes', 'asides'])
})

test('a panel named twice is hidden once', () => {
  assert.deepEqual(parseHidden(['notes', 'notes', 'plan', 'notes']), ['plan', 'notes'])
})

test('an unknown name turns every panel back on', () => {
  assert.deepEqual(parseHidden(['plan', 'sidebar']), [])
  assert.deepEqual(parseHidden(['files']), [])
  assert.deepEqual(parseHidden(['plan', 1]), [])
  assert.deepEqual(parseHidden(['plan', null]), [])
})

test('a value that is not a list means every panel on', () => {
  for (const value of ['plan', null, undefined, {}, { 0: 'plan', length: 1 }, 3]) {
    assert.deepEqual(parseHidden(value), [], String(value))
  }
})

test('parseState defaults hidden to every panel on and keeps a stored list', () => {
  assert.deepEqual(parseState({}).hidden, [])
  assert.deepEqual(parseState(null).hidden, [])
  assert.deepEqual(parseState({ hidden: ['asides'] }).hidden, ['asides'])
  assert.deepEqual(parseState({ hidden: ['asides', 'sidebar'] }).hidden, [])
})

test('a bad hidden list costs that key and nothing beside it', () => {
  const state = parseState({ hidden: 'notes', theme: 'dark' })
  assert.deepEqual(state.hidden, [])
  assert.equal(state.theme, 'dark')
})

test('the read-modify-write of another key keeps hidden', () => {
  const state = parseState({ hidden: ['notes', 'plan'] })
  const next = parseState({ ...state, layout: { left: 200, right: 300, collapsed: { left: false, right: true } } })
  assert.deepEqual(next.hidden, ['plan', 'notes'])
  assert.equal(next.layout.right, 300)
})

test('writing the layout or the view to bravebot-ui.json leaves hidden in the file', () => {
  const userData = mkdtempSync(join(tmpdir(), 'bravebot-panels-'))
  const electron = { app: { getPath: () => userData } }
  const store = load('src/main/state.ts', { electron })
  store.allowStateWrites()

  store.putHidden(['writes', 'confined'])
  store.putLayout({ left: 240, right: 320, collapsed: { left: false, right: false } })
  store.putView({ grouped: true })
  store.putTheme('dark')

  const file = JSON.parse(readFileSync(join(userData, 'bravebot-ui.json'), 'utf8'))
  assert.deepEqual(file.hidden, ['writes', 'confined'])
  assert.equal(file.layout.right, 320)
  assert.deepEqual(store.readState().hidden, ['writes', 'confined'])
})

test('the state inherited on a first launch lists hidden, as it lists every other key', () => {
  const userData = mkdtempSync(join(tmpdir(), 'bravebot-panels-'))
  const store = load('src/main/state.ts', { electron: { app: { getPath: () => userData } } })
  const first = store.inherited()
  assert.ok(Object.hasOwn(first, 'hidden'), 'inherited() must list hidden: ' + JSON.stringify(first))
  assert.deepEqual(first.hidden, [])
  assert.deepEqual(Object.keys(first).sort(), Object.keys(parseState({})).sort())
})

// The column itself, rendered through `react-dom/server`, with the list App.tsx passes it.
const React = require('react')
const { renderToStaticMarkup } = require('react-dom/server')
const { Context } = (() => {
  const source = buildSync({
    entryPoints: ['src/renderer/components/Context.tsx'],
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
})()

const live = { handle: 'h', summary: { directory: '/work' }, entries: [], todos: [], quarantine: [], phase: null, tokens: 0, running: false }
const render = (props) => renderToStaticMarkup(React.createElement(Context, { live, ...props }))
const panelClass = (markup, name) => markup.match(new RegExp(`<section class="([^"]*)" id="panel-${name}"`))?.[1]

test('a hidden panel takes the off class and the others do not', () => {
  const markup = render({ hidden: ['writes'] })
  assert.equal(panelClass(markup, 'writes'), 'panel off', markup)
  assert.equal(panelClass(markup, 'plan'), 'panel ', markup)
  assert.ok(!markup.includes('id="context-all-off"'), markup)
})

test('with no list every Overview panel shows', () => {
  const markup = render({})
  for (const name of HIDEABLE) assert.equal(panelClass(markup, name), 'panel ', name)
  assert.ok(!markup.includes('id="context-all-off"'), markup)
})

test('every panel hidden leaves the column, its head and one line saying so', () => {
  const markup = render({ hidden: [...HIDEABLE] })
  for (const name of HIDEABLE) assert.equal(panelClass(markup, name), 'panel off', name)
  assert.ok(markup.includes('id="context-column"'), markup)
  assert.ok(/class="context-head[ "]/.test(markup), markup)
  assert.ok(markup.includes('id="context-all-off"'), markup)
  assert.ok(markup.includes('Every section is turned off. Use Choose sections to show one.'), markup)
  // A panel's own empty row, inside the same inset the panels give theirs.
  assert.match(markup, /class="panel-inner context-all-off"[^>]*><p class="panel-empty">/)
})

test('ticking or unticking a section toggles that one and keeps column order', () => {
  assert.deepEqual(toggleHidden([], 'writes'), ['writes'])
  assert.deepEqual(toggleHidden(['writes'], 'writes'), [])
  assert.deepEqual(toggleHidden(['notes'], 'plan'), ['plan', 'notes'])
  assert.deepEqual(toggleHidden(['plan', 'notes'], 'plan'), ['notes'])
})

// The rows of the Choose sections menu, as `leo-menu-item` elements in the markup.
const menuRows = (markup) => [...markup.matchAll(/<leo-menu-item([^>]*)>(.*?)<\/leo-menu-item>/g)].map(([, attrs, body]) => ({
  panel: attrs.match(/data-panel="([^"]*)"/)?.[1],
  checked: attrs.match(/aria-checked="([^"]*)"/)?.[1],
  interactive: attrs.includes('data-is-interactive="true"'),
  text: body.replace(/<[^>]*>/g, ''),
}))

test('the Choose sections menu lists each Overview panel in column order, by its heading', () => {
  const markup = render({ hidden: ['writes'], onHidden: () => {} })
  assert.ok(markup.includes('data-test="context-sections"'), markup)
  assert.ok(markup.includes('Choose sections'), markup)
  const rows = menuRows(markup)
  assert.deepEqual(rows.map((row) => row.panel), [...HIDEABLE])
  assert.deepEqual(rows.map((row) => row.text), ['Plan', 'Files read', 'Changes', 'Confined content', 'Notes', 'Asides'])
  assert.deepEqual(rows.map((row) => row.checked), ['true', 'true', 'false', 'true', 'true', 'true'])
  // A choice leaves the menu open, so each row opts out of Leo's close on select.
  assert.ok(rows.every((row) => row.interactive), markup)
})

test('the menu row for the read panel says Calls made when the heading does', () => {
  const replayedLive = { ...live, entries: [{ kind: 'replayed-tool', id: 'r1', text: 'read src/a.rs' }] }
  const markup = renderToStaticMarkup(React.createElement(Context, { live: replayedLive, onHidden: () => {} }))
  assert.equal(menuRows(markup).find((row) => row.panel === 'read')?.text, 'Calls made')
})

test('every section off still leaves the Choose sections menu in the head', () => {
  const markup = render({ hidden: [...HIDEABLE], onHidden: () => {} })
  assert.ok(markup.includes('data-test="context-sections"'), markup)
  assert.ok(menuRows(markup).every((row) => row.checked === 'false'), markup)
})

test('a fresh file has every panel on', () => {
  const userData = mkdtempSync(join(tmpdir(), 'bravebot-panels-'))
  const store = load('src/main/state.ts', { electron: { app: { getPath: () => userData } } })
  assert.deepEqual(store.readState().hidden, [])
  store.allowStateWrites()
  store.putLayout({ left: 240, right: 320, collapsed: { left: false, right: false } })
  const file = JSON.parse(readFileSync(join(userData, 'bravebot-ui.json'), 'utf8'))
  assert.deepEqual(file.hidden, [])
})
