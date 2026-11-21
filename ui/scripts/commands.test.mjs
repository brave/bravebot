import test from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { buildSync } from 'esbuild'
import { readFileSync } from 'node:fs'
const require = createRequire(import.meta.url)
// `stubs` stands in for packages the module imports, which are then left out of the bundle.
function load(path, stubs = {}) {
  const external = Object.keys(stubs)
  const source = buildSync({ entryPoints: [path], bundle: true, write: false, platform: 'node', format: 'cjs', external }).outputFiles[0].text
  const module = { exports: {} }
  const resolve = (name) => (Object.hasOwn(stubs, name) ? stubs[name] : require(name))
  new Function('require', 'module', 'exports', source)(resolve, module, module.exports)
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

test('token spend, the context breakdown and the last request are offered on an open chat, during a turn as well as after it', () => {
  for (const [id, label] of [['session.cost', 'Token Spend…'], ['session.context', 'Context Breakdown…'], ['session.request', 'Last Request…']]) {
    const report = command(id)
    assert.equal(report.label, label)
    assert.equal(isEnabled(report.requires, NOTHING_OPEN), false, `${id} with nothing open`)
    assert.equal(isEnabled(report.requires, { ...NOTHING_OPEN, hasSession: true }), true, `${id} between turns`)
    assert.equal(isEnabled(report.requires, { ...NOTHING_OPEN, hasSession: true, running: true }), true, `${id} mid-turn`)
  }
})

test('the menu places the reports in the Chat menu', () => {
  const menu = readFileSync('src/main/menu.ts', 'utf8')
  const chat = menu.slice(menu.indexOf("label: 'Chat'"), menu.indexOf("role: 'windowMenu'"))
  assert.match(chat, /item\('session\.cost'\)/)
  assert.match(chat, /item\('session\.context'\)/)
  assert.match(chat, /item\('session\.request'\)/)
})

test('the router sends each report command to its own action', () => {
  let listener = null
  globalThis.window = { bravebot: { onCommand: (handler) => { listener = handler; return () => {} } } }
  const react = { useRef: (current) => ({ current }), useEffect: (effect) => { effect() } }
  const { useCommandRouter } = load('src/renderer/commands.ts', { react })
  const called = []
  const actions = new Proxy({}, { get: (_, name) => () => called.push(name) })
  useCommandRouter(actions)
  listener('session.cost')
  listener('session.context')
  listener('session.request')
  assert.deepEqual(called, ['cost', 'context', 'request'])
  delete globalThis.window
})
