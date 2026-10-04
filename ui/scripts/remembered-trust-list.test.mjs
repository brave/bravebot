// How the permissions list draws a yes kept about the working directory.
//
// TRUST-24 says the list gives when the person said to remember it, the file it is kept in and how
// to withdraw it. The bridge sends `at` and `path`; this pins that the window draws all three.
//
// The component is rendered through `react-dom/server` and the markup is what is asserted on.

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

const { RememberedHere } = load('src/renderer/components/Permissions.tsx')
const now = () => Math.floor(Date.now() / 1000)
const render = (kept) => renderToStaticMarkup(React.createElement(RememberedHere, { kept, busy: false, onForget() {} }))

test('a kept answer is listed with when it was given, the file it is kept in and a way to forget it', () => {
  const markup = render({ at: now() - 3 * 86400, path: '/home/someone/.bravebot/trusted' })

  assert.ok(markup.includes('Remembered 3 days ago'), markup)
  assert.ok(markup.includes('<code>/home/someone/.bravebot/trusted</code>'), markup)
  assert.ok(markup.includes('Forget'), markup)
})

test('two answers given at different times are listed with different times', () => {
  const recent = render({ at: now() - 2 * 3600, path: '/p/trusted' })
  const old = render({ at: now() - 2 * 86400, path: '/p/trusted' })

  assert.ok(recent.includes('Remembered 2 hours ago'), recent)
  assert.ok(old.includes('Remembered 2 days ago'), old)
})
