// The fixture the acceptance drivers share, run without Electron.
//
// The drivers need a display. The bridge installer is run here in a fresh vm context against
// recording doubles for `ipcMain`, `BrowserWindow` and `dialog`: Playwright sends the function to
// Electron as source text, so a name the function takes from this module works in Node and fails
// there, and only a context that holds none of the module catches it.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { existsSync, readFileSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import vm from 'node:vm'
import {
  agentInfo,
  eventsFor,
  idA,
  idB,
  installAcceptanceBridge,
  installBridge,
  makeProfile,
  oneModelList,
  threeModelList,
} from './acceptance-fixture.mjs'

const plain = (value) => JSON.parse(JSON.stringify(value))

function bridge(options = {}) {
  const handlers = new Map()
  const events = []
  const dialog = {}
  const ipcMain = { removeHandler: (name) => handlers.delete(name), handle: (name, fn) => handlers.set(name, fn) }
  const BrowserWindow = {
    getAllWindows: () => [{ webContents: { send: (channel, payload) => events.push([channel, plain(payload)]) } }],
  }
  const context = vm.createContext({})
  const install = vm.runInContext(`(${installAcceptanceBridge})`, context)
  install(
    { ipcMain, BrowserWindow, dialog },
    { directory: '/work/project', idA: 'aaaa', idB: 'bbbb', settings: false, nativePicker: false, ...options },
  )
  const request = async (method, params) => plain(await handlers.get('bravebot:request')({}, method, params))
  const ux = (expression) => plain(vm.runInContext(`globalThis.ux${expression}`, context))
  return { handlers, events, dialog, request, ux, run: (code) => vm.runInContext(code, context) }
}

test('the bridge lists both sessions with the updated time in seconds', async () => {
  const { request } = bridge()
  const { ok } = await request('session.list')
  const now = Math.floor(Date.now() / 1000)
  assert.deepEqual(ok.sessions.map((row) => row.id), ['aaaa', 'bbbb'])
  for (const row of ok.sessions) assert.ok(Math.abs(row.updated - now) < 60, `updated ${row.updated} is seconds since the epoch`)
})

test('the bridge opens each session with its own record, and says something only in the first', async () => {
  const { request } = bridge()
  const first = (await request('session.open', { id: 'aaaa' })).ok
  const second = (await request('session.open', { id: 'bbbb' })).ok
  assert.equal(first.session, 's-aaaa')
  assert.equal(first.record.id, 'aaaa')
  assert.deepEqual(first.said.map((entry) => entry.kind), ['user', 'assistant'])
  assert.match(first.said[1].text, /console\.log\(message\)/)
  assert.equal(second.session, 's-bbbb')
  assert.equal(second.record.id, 'bbbb')
  assert.deepEqual(second.said, [])
})

test('settings.inspect is answered only when the driver asks for it, and follows whether the app is configured', async () => {
  const without = bridge()
  assert.deepEqual(await without.request('settings.inspect'), { ok: {} })
  const withIt = bridge({ settings: true })
  const configured = (await withIt.request('settings.inspect')).ok
  assert.equal(configured.configured, true)
  assert.equal(configured.problem, null)
  assert.equal(configured.network.trustsNothing, false)
  withIt.run('globalThis.ux.configured = false')
  const unconfigured = (await withIt.request('settings.inspect')).ok
  assert.equal(unconfigured.configured, false)
  assert.equal(unconfigured.problem, 'No model service configured.')
})

test('the folder is chosen through the bridge handler or through the native picker, never both', async () => {
  const handler = bridge()
  assert.ok(handler.handlers.has('bravebot:choose-directory'))
  assert.equal(handler.dialog.showOpenDialog, undefined)
  assert.equal(await handler.handlers.get('bravebot:choose-directory')({}), '/work/project')

  const picker = bridge({ nativePicker: true })
  assert.equal(picker.handlers.has('bravebot:choose-directory'), false)
  assert.deepEqual(plain(await picker.dialog.showOpenDialog()), { canceled: false, filePaths: ['/work/project'] })
})

test('the chosen folder follows a change to ux.choose in both modes', async () => {
  const handler = bridge()
  handler.run("globalThis.ux.choose += '-copy'")
  assert.equal(await handler.handlers.get('bravebot:choose-directory')({}), '/work/project-copy')

  const picker = bridge({ nativePicker: true })
  picker.run("globalThis.ux.choose += '-copy'")
  assert.deepEqual(plain(await picker.dialog.showOpenDialog()).filePaths, ['/work/project-copy'])
})

test('a sent turn is recorded and announced to the session it was sent to, from either entry point', async () => {
  const { handlers, events, request, ux } = bridge()
  assert.deepEqual(await request('turn.send', { session: 's-bbbb', prompt: 'one' }), { ok: { turn: 1 } })
  assert.deepEqual(plain(await handlers.get('bravebot:bots:send')({}, { session: 's-new', slug: 'review-bot', prompt: 'two' })), { ok: { turn: 2 } })
  assert.deepEqual(ux('.sent').map((sent) => sent.prompt), ['one', 'two'])
  assert.deepEqual(events, [
    ['bravebot:event', { event: 'turn.started', data: { turn: 1 }, session: 's-bbbb' }],
    ['bravebot:event', { event: 'turn.started', data: { turn: 2 }, session: 's-new' }],
  ])
})

test('cancelling names the session that was cancelled, and a new conversation gets its fixed id', async () => {
  const { events, request } = bridge()
  await request('turn.cancel', { session: 's-aaaa' })
  await request('turn.cancel', { session: 's-new' })
  const [existing, created] = events.map(([, payload]) => payload)
  assert.equal(existing.session, 's-aaaa')
  assert.equal(existing.data.kind, 'cancelled')
  assert.equal(existing.data.id, 'aaaa')
  assert.equal(created.session, 's-new')
  assert.equal(created.data.id, '33333333-3333-4333-8333-333333333333')
})

test('revoking a path leaves the command grant and revoking a command leaves the path grants', async () => {
  const { request } = bridge()
  const afterPath = (await request('permissions.revoke', { kind: 'path', path: 'private' })).ok
  assert.deepEqual(afterPath.paths.map((grant) => grant.path), [''])
  assert.equal(afterPath.commands.length, 1)
  const afterCommand = (await request('permissions.revoke', { kind: 'command' })).ok
  assert.deepEqual(afterCommand.commands, [])
  assert.deepEqual(afterCommand.paths.map((grant) => grant.path), [''])
  assert.deepEqual((await request('permissions.list')).ok, afterCommand)
})

test('a method the bridge does not know is answered with an empty result', async () => {
  const { request } = bridge()
  assert.deepEqual(await request('no.such.method'), { ok: {} })
})

test('installBridge hands the installer to the app with the ids and both options defaulted off', async () => {
  const calls = []
  const app = { evaluate: async (...args) => calls.push(args) }
  await installBridge(app, { directory: '/work/project' })
  await installBridge(app, { directory: '/work/project', settings: true, nativePicker: true })
  assert.equal(calls[0][0], installAcceptanceBridge)
  assert.deepEqual(calls[0][1], { directory: '/work/project', idA, idB, settings: false, nativePicker: false })
  assert.deepEqual(calls[1][1], { directory: '/work/project', idA, idB, settings: true, nativePicker: true })
})

test('makeProfile writes the Review Bot into the profile and its memory into the project', () => {
  const { profile, directory } = makeProfile()
  try {
    const state = JSON.parse(readFileSync(join(profile, 'bravebot-ui.json'), 'utf8'))
    assert.deepEqual(state.recents, [directory])
    assert.equal(state.bots.length, 1)
    assert.equal(state.bots[0].slug, 'review-bot')
    assert.equal(state.bots[0].name, 'Review Bot')
    assert.equal(state.bots[0].directory, directory)
    assert.equal(state.bots[0].session, idA)
    assert.deepEqual(state.bots[0].conversations, [idA, idB])
    assert.equal(readFileSync(join(directory, '.bravebot-ui/bots/review-bot.md'), 'utf8'), '# Preferences\n\n- Keep reviews concise.\n')
    assert.notEqual(profile, directory)
    assert.equal(existsSync(`${directory}-copy`), false)
  } finally {
    rmSync(profile, { recursive: true, force: true })
    rmSync(directory, { recursive: true, force: true })
  }
})

test('makeProfile keeps the conversations it is given and makes the copy only when asked', () => {
  const other = '44444444-4444-4444-8444-444444444444'
  const { profile, directory } = makeProfile({ conversations: [idA, other], copy: true })
  try {
    const state = JSON.parse(readFileSync(join(profile, 'bravebot-ui.json'), 'utf8'))
    assert.deepEqual(state.bots[0].conversations, [idA, other])
    assert.equal(existsSync(`${directory}-copy`), true)
  } finally {
    rmSync(profile, { recursive: true, force: true })
    rmSync(directory, { recursive: true, force: true })
    rmSync(`${directory}-copy`, { recursive: true, force: true })
  }
})

test('makeProfile gives each call a profile and a project of its own', () => {
  const first = makeProfile()
  const second = makeProfile()
  try {
    assert.notEqual(first.profile, second.profile)
    assert.notEqual(first.directory, second.directory)
  } finally {
    for (const path of [first.profile, first.directory, second.profile, second.directory]) rmSync(path, { recursive: true, force: true })
  }
})

function withUx(emitted, run) {
  const saved = globalThis.ux
  globalThis.ux = { emit: (event, data, session) => emitted.push({ event, data, session }) }
  return Promise.resolve(run()).finally(() => {
    if (saved === undefined) delete globalThis.ux
    else globalThis.ux = saved
  })
}

// The fake runs the function it is given in this process, which is what the app does with it.
const appThatRuns = { evaluate: async (fn, arg) => fn({}, arg) }

test('emit sends the event to the first conversation unless it is told another', async () => {
  const emitted = []
  await withUx(emitted, async () => {
    const { emit } = eventsFor(appThatRuns)
    await emit('narration', { text: 'a' })
    await emit('narration', { text: 'b' }, 's-bbbb')
  })
  assert.deepEqual(emitted, [
    { event: 'narration', data: { text: 'a' }, session: `s-${idA}` },
    { event: 'narration', data: { text: 'b' }, session: 's-bbbb' },
  ])
})

test('done finishes the turn of the session it names, with the reply it is given', async () => {
  const emitted = []
  await withUx(emitted, async () => {
    const { done } = eventsFor(appThatRuns)
    await done()
    await done('s-bbbb', 'Another reply.')
  })
  assert.equal(emitted.length, 2)
  assert.equal(emitted[0].event, 'turn.done')
  assert.equal(emitted[0].session, `s-${idA}`)
  assert.equal(emitted[0].data.id, idA)
  assert.equal(emitted[0].data.reply, 'Review complete.')
  assert.equal(emitted[1].session, 's-bbbb')
  assert.equal(emitted[1].data.id, 'bbbb')
  assert.equal(emitted[1].data.reply, 'Another reply.')
})

test('the shared answers survive being sent to Electron and name a default model they list', () => {
  for (const value of [agentInfo, threeModelList, oneModelList]) assert.deepEqual(plain(value), value)
  for (const list of [threeModelList, oneModelList]) {
    assert.ok(list.models.some((model) => model.id === list.defaultModel), 'the default model is one of the models')
  }
  assert.equal(threeModelList.models.filter((model) => model.premium).length, 1)
  assert.ok(threeModelList.models.some((model) => !model.capabilities.includes('tools')), 'one model offers no tools')
  assert.equal(oneModelList.models.length, 1)
})
