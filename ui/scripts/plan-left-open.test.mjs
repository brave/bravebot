// How the Plan section draws a row the planner has not settled: a spinning ring and a bold line
// while a turn is actually working on it, and a dimmed, hollow "left open" mark once the turn has
// ended with the row still `active` or `pending` (brave/bravebot#2896). A ring that outlives the
// turn reads as work still going beside an answer that says the work is finished.
//
// The rows are rendered through `react-dom/server` and the markup is what is asserted on.

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

const { Context, todoRowLook } = load('src/renderer/components/Context.tsx')

const live = (todos, running) =>
  ({ handle: 'h', summary: { directory: '/work' }, entries: [], todos, quarantine: [], phase: null, tokens: 0, running })

const rowsOf = (markup) => [...markup.matchAll(/<li class="([^"]*)">.*?<\/li>/g)]

test('an active row rings and reads as in progress while a turn is running', () => {
  const markup = renderToStaticMarkup(React.createElement(Context, {
    live: live([{ content: 'do thing', status: 'active' }], true),
  }))
  assert.ok(markup.includes('<li class="active">'), markup)
  assert.ok(markup.includes('<leo-progressring></leo-progressring>'), markup)
  assert.ok(markup.includes('<span class="visually-hidden">, in progress</span>'), markup)
  assert.ok(!markup.includes('left-open'), markup)
})

test('an active row left open by a finished turn draws as left open, not as still running', () => {
  const markup = renderToStaticMarkup(React.createElement(Context, {
    live: live([{ content: 'do thing', status: 'active' }], false),
  }))
  assert.ok(markup.includes('<li class="left-open">'), markup)
  assert.ok(!markup.includes('leo-progressring'), 'the ring does not outlive the turn: ' + markup)
  assert.ok(markup.includes('<span class="visually-hidden">, left open</span>'), markup)
})

test('a pending row left open by a finished turn draws the same as an active one left open', () => {
  const markup = renderToStaticMarkup(React.createElement(Context, {
    live: live([{ content: 'next thing', status: 'pending' }], false),
  }))
  assert.ok(markup.includes('<li class="left-open">'), markup)
  assert.ok(markup.includes('<span class="visually-hidden">, left open</span>'), markup)
})

test('a pending row while a turn is running keeps its own look, not left-open', () => {
  const markup = renderToStaticMarkup(React.createElement(Context, {
    live: live([{ content: 'next thing', status: 'pending' }], true),
  }))
  assert.ok(markup.includes('<li class="pending">'), markup)
  assert.ok(!markup.includes('left-open'), markup)
})

test('a done or cancelled row draws the same whether or not a turn is running', () => {
  const todos = [{ content: 'done thing', status: 'done' }, { content: 'skip thing', status: 'cancelled' }]
  const running = renderToStaticMarkup(React.createElement(Context, { live: live(todos, true) }))
  const idle = renderToStaticMarkup(React.createElement(Context, { live: live(todos, false) }))
  assert.deepEqual(rowsOf(running).map((m) => m[1]), ['done', 'cancelled'])
  assert.deepEqual(rowsOf(idle).map((m) => m[1]), ['done', 'cancelled'])
})

test('todoRowLook: only an active row mid-turn gets the ring; anything else open once idle is left open', () => {
  assert.deepEqual(todoRowLook({ content: 'x', status: 'active' }, true), { rowClass: 'active', marker: 'active', hiddenSuffix: ', in progress' })
  assert.deepEqual(todoRowLook({ content: 'x', status: 'active' }, false), { rowClass: 'left-open', marker: 'open', hiddenSuffix: ', left open' })
  assert.deepEqual(todoRowLook({ content: 'x', status: 'pending' }, true), { rowClass: 'pending', marker: 'open', hiddenSuffix: '' })
  assert.deepEqual(todoRowLook({ content: 'x', status: 'pending' }, false), { rowClass: 'left-open', marker: 'open', hiddenSuffix: ', left open' })
  assert.deepEqual(todoRowLook({ content: 'x', status: 'done' }, false), { rowClass: 'done', marker: 'done', hiddenSuffix: ', done' })
  assert.deepEqual(todoRowLook({ content: 'x', status: 'done' }, true), { rowClass: 'done', marker: 'done', hiddenSuffix: ', done' })
  assert.deepEqual(todoRowLook({ content: 'x', status: 'cancelled' }, false), { rowClass: 'cancelled', marker: 'cancelled', hiddenSuffix: ', cancelled' })
})
