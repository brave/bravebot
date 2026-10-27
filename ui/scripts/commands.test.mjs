import test from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { buildSync } from 'esbuild'
const require = createRequire(import.meta.url)
function load(path) {
  const source = buildSync({ entryPoints: [path], bundle: true, write: false, platform: 'node', format: 'cjs' }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
}
const { NOTHING_OPEN, isEnabled, command } = load('src/shared/commands.ts')

test('the release notes can be shown with nothing open, since they belong to no session', () => {
  const notes = command('help.changelog')
  assert.equal(notes.label, 'Release Notes…')
  assert.equal(isEnabled(notes.requires, NOTHING_OPEN), true)
  assert.equal(isEnabled(notes.requires, { ...NOTHING_OPEN, hasSession: true, running: true }), true)
})

test('a note is offered on an open chat with no turn running, which holds the record until it ends', () => {
  const note = command('session.note')
  assert.equal(note.label, 'Add Note…')
  assert.equal(isEnabled(note.requires, NOTHING_OPEN), false)
  assert.equal(isEnabled(note.requires, { ...NOTHING_OPEN, hasSession: true }), true)
  assert.equal(isEnabled(note.requires, { ...NOTHING_OPEN, hasSession: true, running: true }), false)
})
