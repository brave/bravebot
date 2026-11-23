// How the permissions list draws a remembered command (RUN-8, RUN-9).
//
// An entry is keyed on the tree it was vouched in as well as on the command, so one command vouched
// for in two trees is two grants. The bridge sends the tree of each, and this pins that the window
// draws it, keys the two rows apart, and revokes with the tree of the row that was pressed.
//
// The component is rendered through `react-dom/server` for its markup, and called as a function for
// the element tree, whose Revoke buttons are pressed by calling their `onClick`.

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

const { CommandPermissions } = load('src/renderer/components/Permissions.tsx')

const command = (directory, shownDirectory) => ({
  program: '/bin/sh',
  startedAs: '/bin/sh',
  args: ['check.sh'],
  directory,
  shownDirectory,
  display: '/bin/sh check.sh',
})
const root = command('/w', '.')
const sub = command('/w/sub', 'sub')

const render = (commands, onRevoke = () => {}) => renderToStaticMarkup(React.createElement(CommandPermissions, { commands, busy: false, onRevoke }))

// The `<li>` rows of the list, as elements, and the Revoke button each holds.
const rows = (commands, onRevoke = () => {}) => {
  const list = CommandPermissions({ commands, busy: false, onRevoke })
  return React.Children.toArray(list.props.children).map((row) => {
    const found = []
    const visit = (node) => {
      if (!node || typeof node !== 'object') return
      if (typeof node.props?.onClick === 'function') found.push(node)
      React.Children.forEach(node.props?.children, visit)
    }
    visit(row)
    return { key: row.key, revoke: found[0] }
  })
}

test('two commands that differ only in the tree draw the tree each was vouched in', () => {
  const markup = render([root, sub])

  assert.equal(markup.split('/bin/sh check.sh').length - 1, 2, markup)
  assert.ok(markup.includes('data-test="command-tree">in .</span>'), markup)
  assert.ok(markup.includes('data-test="command-tree">in sub</span>'), markup)
})

test('two commands that differ only in the tree are not keyed alike', () => {
  const [first, second] = rows([root, sub])

  assert.ok(first.key && second.key)
  assert.notEqual(first.key, second.key)
})

test('Revoke sends the tree of the row it is on', () => {
  const sent = []
  const [first, second] = rows([root, sub], (selected) => sent.push(selected))

  second.revoke.props.onClick()
  first.revoke.props.onClick()

  assert.deepEqual(sent, [
    { program: '/bin/sh', startedAs: '/bin/sh', args: ['check.sh'], directory: '/w/sub' },
    { program: '/bin/sh', startedAs: '/bin/sh', args: ['check.sh'], directory: '/w' },
  ])
})

test('a command whose tree has no text spelling cannot be revoked from the window, and the row says why', () => {
  const [row] = rows([command(null, '/odd')])

  assert.equal(row.revoke.props.isDisabled, true)
  assert.ok(render([command(null, '/odd')]).includes('data-test="command-not-revocable"'))
  assert.ok(!render([root]).includes('command-not-revocable'))
})
