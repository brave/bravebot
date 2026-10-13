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

const ROWS = [
  {
    surface: 'file preview', file: 'FilePreview', props: { session: 's', path: 'notes.txt', onClose() {} },
    words: 'Loading preview…', empty: 'A text preview is not available',
  },
  { surface: 'bot memory', file: 'BotMemory', props: { slug: 'helper', directory: '/p' }, words: 'Loading…', empty: 'Nothing remembered yet.' },
  { surface: 'file watch list', file: 'Watches', props: { session: 's', onClose() {} }, words: 'Loading watches…', empty: 'No files watched' },
]

const bundles = new Map()

/** The three components, built once for each set of plugins. */
function components(plugins) {
  const key = plugins.map((plugin) => plugin.name).join()
  if (!bundles.has(key)) bundles.set(key, build({
    stdin: {
      contents: ROWS.map((row) => `export { ${row.file} } from './src/renderer/components/${row.file}.tsx'`).join('\n'),
      resolveDir: process.cwd(),
      loader: 'ts',
    },
    bundle: true,
    write: false,
    platform: 'node',
    format: 'cjs',
    jsx: 'automatic',
    external: ['react', 'react-dom', 'react/jsx-runtime'],
    plugins,
  }).then((built) => {
    const module = { exports: {} }
    new Function('require', 'module', 'exports', built.outputFiles[0].text)(require, module, module.exports)
    return module.exports
  }))
  return bundles.get(key)
}

async function paint(row, plugins) {
  const component = (await components(plugins))[row.file]
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

  test(`the ${row.surface} does not show its empty state while loading, before its ring shows`, async () => {
    const markup = await paint(row, [inPlaceModal])

    assert.ok(!markup.includes(row.empty), markup)
  })
}

test('a row marked data-delayed is hidden by the stylesheets', () => {
  const styles = readdirSync('src/renderer/styles').filter((name) => name.endsWith('.css'))
    .map((name) => readFileSync(`src/renderer/styles/${name}`, 'utf8')).join('\n')

  assert.match(styles, /\[data-delayed\]\s*\{\s*visibility:\s*hidden;?\s*\}/)
})
