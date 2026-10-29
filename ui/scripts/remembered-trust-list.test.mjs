// How the permissions list draws a yes kept about the working directory.
//
// TRUST-24 says the list gives when the person trusted the directory, the file the yes is kept in,
// the git worktree root where the answer was kept about one, and how to withdraw it. The bridge sends
// `at`, `path` and `root`; this pins that the window draws all four.
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

const { RememberedHere, GRANT_PATH_WIDTH } = load('src/renderer/components/Permissions.tsx')
const { middleTruncate } = load('src/renderer/truncate.ts')
const { DIALOG_FRAME_CSS } = load('src/renderer/components/Modal.tsx')
const now = () => Math.floor(Date.now() / 1000)
const render = (kept) => renderToStaticMarkup(React.createElement(RememberedHere, { kept, busy: false, onForget() {} }))

test('a kept answer is listed with when it was given, the file it is kept in and a way to forget it', () => {
  const markup = render({ at: now() - 3 * 86400, path: '/home/someone/.bravebot/trusted', shownPath: '/home/someone/.bravebot/trusted' })

  assert.ok(markup.includes('Trusted 3 days ago'), markup)
  assert.ok(markup.includes('<code>/home/someone/.bravebot/trusted</code>'), markup)
  assert.ok(markup.includes('Forget'), markup)
})

test('two answers given at different times are listed with different times', () => {
  const recent = render({ at: now() - 2 * 3600, path: '/p/trusted', shownPath: '/p/trusted' })
  const old = render({ at: now() - 2 * 86400, path: '/p/trusted', shownPath: '/p/trusted' })

  assert.ok(recent.includes('Trusted 2 hours ago'), recent)
  assert.ok(old.includes('Trusted 2 days ago'), old)
})

test('an answer kept about a git worktree root names that root', () => {
  const markup = render({ at: now() - 60, path: '/h/.bravebot/trusted', shownPath: '/h/.bravebot/trusted', root: '/work/repo', shownRoot: '/work/repo' })

  assert.ok(markup.includes('data-test="remembered-root"') && markup.includes('for /work/repo'), markup)
})

test('an answer kept about no root names none', () => {
  const markup = render({ at: now() - 60, path: '/h/.bravebot/trusted', shownPath: '/h/.bravebot/trusted' })

  assert.ok(!markup.includes('remembered-root'), markup)
})

test('every dialog box is held to its width so a long line cannot grow a horizontal scrollbar', () => {
  assert.match(DIALOG_FRAME_CSS, /\.leo-dialog\s*\{[^}]*overflow-x:\s*hidden/)
  assert.match(DIALOG_FRAME_CSS, /\.body\s*\{[^}]*overflow-x:\s*hidden/)
  assert.match(DIALOG_FRAME_CSS, /min-width:\s*0/)
})

test('a long kept path is shortened in the middle and the full spelling stays on the tooltip', () => {
  const shownPath = '/Users/someone/Library/Application Support/bravebot-ui-b/trusted'
  const shownRoot = '/Users/someone/Library/Application Support/bravebot-ui-b'
  const markup = render({
    at: now() - 86400,
    path: shownPath,
    shownPath,
    root: shownRoot,
    shownRoot,
  })
  const cutPath = middleTruncate(shownPath, GRANT_PATH_WIDTH)
  const cutRoot = middleTruncate(shownRoot, GRANT_PATH_WIDTH)

  assert.notEqual(cutPath, shownPath)
  assert.ok(markup.includes(`>${cutPath}</code>`), markup)
  assert.ok(markup.includes(`data-tooltip="${shownPath}"`), markup)
  assert.ok(!markup.includes(`>${shownPath}</code>`), markup)
  assert.ok(markup.includes(`for ${cutRoot}`), markup)
  assert.ok(markup.includes(`data-tooltip="${shownRoot}"`), markup)
})
