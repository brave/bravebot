import test from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { buildSync } from 'esbuild'
import { readFileSync } from 'node:fs'
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

test('Compact Conversation… is offered on an open chat with no turn running, and the menu and router reach it (CMD-34)', () => {
  const compact = command('session.compact')
  assert.equal(compact.label, 'Compact Conversation…')
  assert.equal(compact.requires, 'forkable')
  assert.equal(compact.accelerator, undefined, 'it asks for a focus first, so it has no shortcut')
  assert.equal(isEnabled(compact.requires, NOTHING_OPEN), false)
  assert.equal(isEnabled(compact.requires, { ...NOTHING_OPEN, hasSession: true }), true)
  assert.equal(isEnabled(compact.requires, { ...NOTHING_OPEN, hasSession: true, running: true }), false)
  assert.match(readFileSync('src/main/menu.ts', 'utf8'), /item\('session\.compact'\)/)
  assert.match(readFileSync('src/renderer/commands.ts', 'utf8'), /case 'session\.compact':\s*return act\.compact\(\)/)
  const allowed = readFileSync('src/main/index.ts', 'utf8')
  assert.match(allowed, /'session\.compact',/)
  assert.match(allowed, /'session\.compact\.cancel',/)
})

test('Hand Off to New Chat… is offered on an open chat with no turn running, and the menu and router reach it (SESSION-34, CMD-34)', () => {
  const handoff = command('session.handoff')
  assert.equal(handoff.label, 'Hand Off to New Chat…')
  assert.equal(handoff.requires, 'forkable')
  assert.equal(handoff.accelerator, undefined, 'it asks for the goal first, so it has no shortcut')
  assert.equal(isEnabled(handoff.requires, NOTHING_OPEN), false)
  assert.equal(isEnabled(handoff.requires, { ...NOTHING_OPEN, hasSession: true }), true)
  assert.equal(isEnabled(handoff.requires, { ...NOTHING_OPEN, hasSession: true, running: true }), false)
  assert.match(readFileSync('src/main/menu.ts', 'utf8'), /item\('session\.handoff'\)/)
  assert.match(readFileSync('src/renderer/commands.ts', 'utf8'), /case 'session\.handoff':\s*return act\.handoff\(\)/)
  const allowed = readFileSync('src/main/index.ts', 'utf8')
  assert.match(allowed, /'session\.handoff',/)
  assert.match(allowed, /'session\.handoff\.cancel',/)
})
