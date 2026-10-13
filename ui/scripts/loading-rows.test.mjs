// When a loading row shows its ring.
//
// The design skill holds a loading state to showing its ring only once loading has lasted 150ms
// (`useDelayedFlag`), so an answer that arrives sooner draws nothing on its way past. The row is
// still laid out from the moment loading starts, hidden by `data-delayed`, so nothing moves when it
// shows. The components are rendered through `react-dom/server`, whose one pass is the first paint:
// effects never run, so the delay has not passed. A second build replaces the hook with one whose
// delay has passed.
//
// The model menu and the file search set their loading flag inside an effect, which a server render
// never runs, so their rows are not reached here.

import test from 'node:test'
import assert from 'node:assert/strict'
import { build } from 'esbuild'
import { createRequire } from 'node:module'
import { readFileSync, readdirSync } from 'node:fs'
import { resolve } from 'node:path'

const require = createRequire(import.meta.url)
const React = require('react')
const { renderToStaticMarkup } = require('react-dom/server')

// `Modal` draws through a portal into `document.body`, which `react-dom/server` cannot render, so
// here it is drawn in place.
const inPlaceModal = {
  name: 'in-place-modal',
  setup(built) {
    built.onResolve({ filter: /^\.\/Modal$/ }, () => ({ path: 'modal', namespace: 'in-place-modal' }))
    built.onLoad({ filter: /.*/, namespace: 'in-place-modal' }, () => ({
      loader: 'js',
      resolveDir: process.cwd(),
      contents: `import { createElement } from 'react'
        export function Modal({ children }) { return createElement('div', { className: 'modal' }, children) }`,
    }))
  },
}

const delayPassed = {
  name: 'delay-passed',
  setup(built) {
    built.onResolve({ filter: /^\.\.\/hooks$/ }, () => ({ path: 'hooks', namespace: 'delay-passed' }))
    built.onLoad({ filter: /.*/, namespace: 'delay-passed' }, () => ({
      loader: 'ts',
      resolveDir: process.cwd(),
      contents: `export * from ${JSON.stringify(resolve('src/renderer/hooks.ts'))}
        export function useDelayedFlag(value: boolean): boolean { return value }`,
    }))
  },
}

async function load(path, plugins) {
  const source = (await build({
    entryPoints: [path],
    bundle: true,
    write: false,
    platform: 'node',
    format: 'cjs',
    jsx: 'automatic',
    external: ['react', 'react-dom', 'react/jsx-runtime'],
    plugins,
  })).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
}

const ROWS = [
  { surface: 'file preview', file: 'FilePreview', props: { session: 's', path: 'notes.txt', onClose() {} }, words: 'Loading preview…' },
  { surface: 'bot memory', file: 'BotMemory', props: { slug: 'helper', directory: '/p' }, words: 'Loading…' },
  { surface: 'file watches', file: 'Watches', props: { session: 's', onClose() {} }, words: 'Loading watches…' },
]

async function paint(row, plugins) {
  const component = (await load(`src/renderer/components/${row.file}.tsx`, plugins))[row.file]
  return renderToStaticMarkup(React.createElement(component, row.props))
}

/** The opening tag of the status row that holds `words`, or null when no such row is drawn. */
function statusRow(markup, words) {
  for (const match of markup.matchAll(/<p\b[^>]*>(?:(?!<\/p>).)*<\/p>/g)) {
    if (match[0].includes('role="status"') && match[0].includes(words)) return match[0].slice(0, match[0].indexOf('>') + 1)
  }
  return null
}

for (const row of ROWS) {
  test(`the ${row.surface} loading row is laid out and hidden until loading has lasted 150ms`, async () => {
    const markup = await paint(row, [inPlaceModal])
    const tag = statusRow(markup, row.words)

    assert.ok(tag, markup)
    assert.ok(tag.includes('data-delayed="true"'), tag)
  })

  test(`the ${row.surface} loading row shows once loading has lasted 150ms`, async () => {
    const markup = await paint(row, [inPlaceModal, delayPassed])
    const tag = statusRow(markup, row.words)

    assert.ok(tag, markup)
    assert.ok(!tag.includes('data-delayed'), tag)
  })
}

test('a preview still loading does not say the file is unavailable before its ring shows', async () => {
  const markup = await paint(ROWS[0], [inPlaceModal])

  assert.ok(!markup.includes('A text preview is not available'), markup)
})

test('a row marked data-delayed is hidden by the stylesheets', () => {
  const styles = readdirSync('src/renderer/styles').filter((name) => name.endsWith('.css'))
    .map((name) => readFileSync(`src/renderer/styles/${name}`, 'utf8')).join('\n')

  assert.match(styles, /\[data-delayed\]\s*\{\s*visibility:\s*hidden;?\s*\}/)
})
