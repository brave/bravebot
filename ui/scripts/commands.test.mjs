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

test('the session status, token spend, the context breakdown and the last request are offered on an open chat, during a turn as well as after it', () => {
  for (const [id, label] of [['session.status', 'Session Status…'], ['session.cost', 'Token Spend…'], ['session.context', 'Context Breakdown…'], ['session.request', 'Last Request…']]) {
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
  assert.match(chat, /item\('session\.status'\)/)
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
  listener('session.status')
  listener('session.cost')
  listener('session.context')
  listener('session.request')
  assert.deepEqual(called, ['status', 'cost', 'context', 'request'])
  delete globalThis.window
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

test('Continue in Background… is offered on an open chat with no turn running, and the menu and router reach it (BG-2, CMD-34)', () => {
  const background = command('session.background')
  assert.equal(background.label, 'Continue in Background…')
  assert.equal(background.requires, 'forkable')
  assert.equal(background.accelerator, undefined, 'it asks to confirm first, so it has no shortcut')
  assert.equal(isEnabled(background.requires, NOTHING_OPEN), false)
  assert.equal(isEnabled(background.requires, { ...NOTHING_OPEN, hasSession: true }), true)
  assert.equal(isEnabled(background.requires, { ...NOTHING_OPEN, hasSession: true, running: true }), false)
  assert.match(readFileSync('src/main/menu.ts', 'utf8'), /item\('session\.background'\)/)
  assert.match(readFileSync('src/renderer/commands.ts', 'utf8'), /case 'session\.background':\s*return act\.background\(\)/)
  assert.match(readFileSync('src/main/index.ts', 'utf8'), /'session\.background',/)
})

// One bundle for both, so the error class thrown is the one `backgrounding.ts` checks for.
const { continueInBackground, shortId, RequestFailed } = (() => {
  const contents = "export * from './src/renderer/backgrounding.ts'\nexport { RequestFailed } from './src/renderer/rewind.ts'"
  const source = buildSync({ stdin: { contents, resolveDir: process.cwd(), loader: 'ts' }, bundle: true, write: false, platform: 'node', format: 'cjs' }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
})()

test('a chat handed to the background closes and the toast names the start of its id, as bravebot sessions shows it', async () => {
  const id = '0123456789abcdef-0000'
  const asked = []
  const came = await continueInBackground(async (session) => { asked.push(session); return { handedOver: id, mode: 'ask' } }, 'h1')
  assert.deepEqual(asked, ['h1'])
  assert.equal(came.closed, true)
  assert.equal(shortId(id), '01234567')
  assert.equal(came.toast.body, 'It runs in ask mode. Join it with: bravebot attach 01234567')
  assert.doesNotMatch(came.toast.body, /89abcdef/)
})

test('a refusal leaves the chat open and shows the bridge sentence, and a failure after the bridge closed it lets go of it', async () => {
  const sentence = 'Continue in Background is not available in bypass or auto mode.'
  const refused = await continueInBackground(async () => { throw new RequestFailed('bad_request', sentence) }, 'h1')
  assert.deepEqual(refused, { closed: false, problem: sentence })
  const late = await continueInBackground(async () => { throw new RequestFailed('internal', 'The chat was closed, and the background session did not start in time. Open it again from the list.') }, 'h1')
  assert.equal(late.closed, true)
  assert.match(late.problem, /Open it again from the list/)
})
