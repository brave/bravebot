// The desktop as a client of the host (HOSTCLIENT-1): what the main process asks of the shared
// client and what it tells the window. The host here is a script that answers as the real host
// does; the Rust process tests (`crates/ui-bridge/tests/host_process.rs`) and the driver
// (`drive-prototype.mjs`) hold the real one. These hold the halves that need no window and no host:
// which arguments make this process a client, which actions the window may ask for, and what a
// refusal, a busy send and a view that ended look like in the state it is told.

import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)

function load(path) {
  const source = buildSync({
    entryPoints: [path],
    bundle: true,
    write: false,
    platform: 'node',
    format: 'cjs',
    external: ['electron'],
  }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
}

const { hostClientOptions, HOST_CLIENT_FLAG } = load('src/main/prototype/options.ts')
const { HostClient } = load('src/main/prototype/client.ts')
const { parseAction, LONGEST_MESSAGE } = load('src/shared/prototype.ts')
const contract = JSON.parse(readFileSync('../packages/agent-client/test-fixtures/wire-contract.json', 'utf8'))

const flags = (extra = []) => ['electron', '.', HOST_CLIENT_FLAG, '--host-client-name', 'desk', '--host-client-workspace', '/work', ...extra]

test('the desktop is a host client only when it is given the flag, with the name and folder the person gave', () => {
  assert.equal(hostClientOptions(['electron', '.']), null)
  assert.equal(hostClientOptions(['electron', '.', '--host-client-name', 'desk']), null, 'the name alone does not make it one')
  assert.deepEqual(hostClientOptions(flags()), { name: 'desk', workspace: '/work' })
  for (const wrong of [
    ['electron', '.', HOST_CLIENT_FLAG, '--host-client-workspace', '/work'],
    ['electron', '.', HOST_CLIENT_FLAG, '--host-client-name', 'Desk', '--host-client-workspace', '/work'],
    ['electron', '.', HOST_CLIENT_FLAG, '--host-client-name', 'desk'],
    ['electron', '.', HOST_CLIENT_FLAG, '--host-client-name', 'desk', '--host-client-workspace', 'relative/dir'],
  ]) {
    assert.ok(hostClientOptions(wrong) instanceof Error, wrong.join(' '))
  }
})

test('the window may ask for these actions and no others, and a field it should not have makes it none', () => {
  assert.deepEqual(parseAction({ kind: 'new' }), { kind: 'new' })
  assert.deepEqual(parseAction({ kind: 'send', session: 's1', text: 'hello' }), { kind: 'send', session: 's1', text: 'hello' })
  assert.deepEqual(parseAction({ kind: 'decide', session: 's1', request: 4, decision: 'approve' }), { kind: 'decide', session: 's1', request: 4, decision: 'approve' })
  assert.deepEqual(parseAction({ kind: 'answer', session: 's1', request: 2, answers: [null, { typed: 'x' }, { chosen: [0, 2] }] }), { kind: 'answer', session: 's1', request: 2, answers: [null, { typed: 'x' }, { chosen: [0, 2] }] })
  assert.deepEqual(parseAction({ kind: 'trust', session: 's1', trusted: false }), { kind: 'trust', session: 's1', trusted: false })
  assert.deepEqual(parseAction({ kind: 'cancel', session: 's1' }), { kind: 'cancel', session: 's1' })
  for (const refused of [
    null, 'new', [], {}, { kind: 'close' }, { kind: 'new', session: 's1' }, { kind: 'attach' }, { kind: 'attach', session: '' },
    { kind: 'send', session: 's1', text: '   ' }, { kind: 'send', session: 's1', text: 3 }, { kind: 'send', session: 's1', text: 'x'.repeat(LONGEST_MESSAGE + 1) }, { kind: 'send', session: 's1', text: 'hi', messageId: 'a' },
    { kind: 'send', text: 'hi' }, { kind: 'cancel' }, { kind: 'trust', trusted: true }, { kind: 'decide', request: 1, decision: 'approve' }, { kind: 'send', session: '', text: 'hi' },
    { kind: 'decide', session: 's1', request: -1, decision: 'approve' }, { kind: 'decide', session: 's1', request: 1.5, decision: 'approve' }, { kind: 'decide', session: 's1', request: 1, decision: 'yes' },
    { kind: 'answer', session: 's1', request: 1, answers: [{ typed: 1 }] }, { kind: 'answer', session: 's1', request: 1, answers: [{ typed: 'a', chosen: [] }] },
    { kind: 'trust', session: 's1', trusted: 'yes' },
  ]) {
    assert.equal(parseAction(refused), null, JSON.stringify(refused))
  }
})

/** A host that holds `held`, answers as the real one does, and lets a test make the next send busy. */
function aHost({ held = [], answers = ['confirm', 'run', 'fetch', 'ask', 'trust'] } = {}) {
  const host = { sent: [], busy: false, client: null, changes: 0, ended: false, holding: false, late: [] }
  host.releaseInfo = () => host.late.splice(0).forEach((deliver) => deliver())
  const channel = {
    onData: (listener) => (host.deliver = listener),
    onEnd: (listener) => (host.end = listener),
    write(line) {
      const request = JSON.parse(line)
      host.sent.push({ method: request.method, params: request.params })
      const reply = (body) => host.deliver(JSON.stringify({ id: request.id, ...body }) + '\n')
      const event = (session, name, data) => host.deliver(JSON.stringify({ event: name, session, data }) + '\n')
      switch (request.method) {
        case 'agent.info':
          return reply({ ok: { capabilities: { sessionView: contract.capability } } })
        case 'instance.info': {
          const body = { ok: { instance: 'run-1', inspection: 'refused', sessions: structuredClone(held), answers } }
          if (host.holding) host.late.push(() => reply(body))
          else reply(body)
          return
        }
        case 'session.new':
          held.push({ session: 's1', state: 'open', working: false, attached: true })
          return reply({ ok: { session: 's1' } })
        case 'session.view.start':
          event('s1', 'session.view.initial', { sequence: 0, turn: 0, target: 0, status: 'idle', pending: null, rows: [], actor: null })
          return reply({ ok: { version: 1 } })
        case 'session.attach': {
          const { session } = request.params
          const answer = () => {
            event(session, 'session.view.snapshot', { sequence: 4, turn: 1, target: 3, status: 'completed', pending: null, omitted: 0, actor: 'phone',
              rows: [{ id: 1, turn: 1, kind: 'prompt', event: null, data: { kind: 'user', text: 'from the phone' }, resolved: false }] })
            reply({ ok: { version: 1, sequence: 4, omitted: 0 } })
          }
          // The test may keep one session's attach out, as a slow host would.
          if (host.slowAttach === session) host.late.push(answer)
          else answer()
          return
        }
        case 'turn.send':
          return host.busy
            ? reply({ error: { code: 'turn_in_flight', message: 'a turn is running' } })
            : reply({ ok: { turn: 1, target: 3 } })
        case 'trust.reply':
          return reply({ ok: { trusted: request.params.trusted, kept: null } })
        case 'turn.cancel':
          return reply({ ok: { cancelled: true } })
        default:
          return reply({ error: { code: 'not_allowed', message: `${request.method} is not offered` } })
      }
    },
  }
  host.client = new HostClient({ name: 'desk', workspace: '/work' }, channel, () => host.changes++)
  host.update = (session, data) => host.deliver(JSON.stringify({ event: 'session.view.update', session, data }) + '\n')
  return host
}

test('opening a session shows the view the host sent and who acted last, and a joined one shows the snapshot', async () => {
  const host = aHost({ held: [{ session: 's7', state: 'open', working: false, attached: false }] })
  await host.client.refresh()
  assert.deepEqual(host.client.state().answers, ['confirm', 'run', 'fetch', 'ask', 'trust'])
  assert.equal(host.client.state().session, null)

  assert.deepEqual(await host.client.act({ kind: 'new' }), { ok: true })
  let state = host.client.state()
  assert.equal(state.session.id, 's1')
  assert.equal(state.session.view.actor, null)
  host.update('s1', { sequence: 1, turn: 1, target: 3, status: 'running', pending: null, rows: [], actor: 'desk' })
  assert.equal(host.client.state().session.view.actor, 'desk')

  assert.deepEqual(await host.client.act({ kind: 'attach', session: 's7' }), { ok: true })
  state = host.client.state()
  assert.equal(state.session.id, 's7')
  assert.equal(state.session.view.actor, 'phone')
  assert.deepEqual(state.session.view.rows.map((row) => row.data.text), ['from the phone'])
  assert.ok(host.changes > 0, 'the window was told')
})

test('the window is told of a change in what the host holds, and not of a host that has not changed', async () => {
  const host = aHost({ held: [{ session: 's7', state: 'open', working: false, attached: false }] })
  await host.client.refresh()
  const told = host.changes
  await host.client.refresh()
  await host.client.refresh()
  assert.equal(host.changes, told, 'a quiet host cost the window a redraw')
  await Promise.all([host.client.refresh(), host.client.refresh()])
  assert.equal(host.sent.filter((request) => request.method === 'instance.info').length, 4, 'one question out at a time')
})

test('a session opened while a question about the host is out is listed, not missed', async () => {
  const host = aHost()
  // The host answers the next question about what it holds only when the test lets it, with what it
  // held when it was asked.
  host.holding = true
  const stale = host.client.refresh()
  await new Promise((resolve) => setImmediate(resolve))
  assert.equal(host.late.length, 1, 'the question about the host is out')
  const opened = host.client.act({ kind: 'new' })
  await new Promise((resolve) => setImmediate(resolve))
  host.holding = false
  host.releaseInfo()
  await Promise.all([stale, opened])
  assert.deepEqual(host.client.state().held.map((held) => held.session), ['s1'])
})

test('an action that succeeded tells the window, so an answer the host took is shown at once', async () => {
  const host = aHost()
  await host.client.act({ kind: 'new' })
  const told = host.changes
  await host.client.act({ kind: 'trust', session: 's1', trusted: false })
  assert.ok(host.changes > told)
})

test('a send the host refuses as busy says so in the host\'s own code and changes nothing here', async () => {
  const host = aHost()
  await host.client.act({ kind: 'new' })
  host.busy = true
  const before = JSON.stringify(host.client.state())
  assert.deepEqual(await host.client.act({ kind: 'send', session: 's1', text: 'again' }), { ok: false, code: 'turn_in_flight', message: 'a turn is running' })
  assert.equal(JSON.stringify(host.client.state()), before)
  host.busy = false
  assert.deepEqual(await host.client.act({ kind: 'send', session: 's1', text: 'now' }), { ok: true })
  assert.deepEqual(host.sent.filter((request) => request.method === 'turn.send').map((request) => request.params.prompt), ['again', 'now'])
})

test('an action for a session that is not on screen is refused and reaches no session, whatever its question number', async () => {
  const host = aHost({ held: [{ session: 's7', state: 'open', working: false, attached: false }] })
  await host.client.act({ kind: 'new' })
  await host.client.act({ kind: 'attach', session: 's7' })
  const sent = host.sent.length
  // The window still drew s1 when it asked, but s7 is on screen now.
  for (const action of [
    { kind: 'send', session: 's1', text: 'for s1' },
    { kind: 'cancel', session: 's1' },
    { kind: 'decide', session: 's1', request: 1, decision: 'approve' },
    { kind: 'answer', session: 's1', request: 1, answers: [null] },
    { kind: 'trust', session: 's1', trusted: true },
  ]) {
    assert.equal((await host.client.act(action)).code, 'stale_session', action.kind)
  }
  assert.equal(host.sent.length, sent, 'nothing reached the host')
})

test('the window is told at once which session is on screen, and the latest choice is the one that stays', async () => {
  const host = aHost({ held: [{ session: 's7', state: 'open', working: false, attached: false }, { session: 's8', state: 'open', working: false, attached: false }] })
  await host.client.refresh()
  const told = host.changes
  // The host is slow to answer the attach of s7, and the person has chosen s8 since.
  host.slowAttach = 's7'
  const first = host.client.act({ kind: 'attach', session: 's7' })
  await new Promise((resolve) => setImmediate(resolve))
  assert.equal(await host.client.act({ kind: 'attach', session: 's8' }).then((outcome) => outcome.ok), true)
  assert.equal(host.client.state().session.id, 's8')
  host.slowAttach = null
  host.releaseInfo()
  await first
  assert.equal(host.client.state().session.id, 's8', 'a slow choice took the window back')
  assert.ok(host.changes > told)
  // A session already held is shown with no question to the host.
  const sent = host.sent.length
  await host.client.act({ kind: 'attach', session: 's7' })
  assert.equal(host.client.state().session.id, 's7')
  assert.ok(!host.sent.slice(sent).some((request) => request.method === 'session.attach'))
})

test('a request the host does not offer is refused with its code, and no action reaches the host without a session', async () => {
  const host = aHost()
  assert.equal((await host.client.act({ kind: 'send', session: 's1', text: 'hi' })).code, 'no_session')
  assert.equal((await host.client.act({ kind: 'decide', session: 's1', request: 1, decision: 'approve' })).code, 'no_session')
  assert.ok(!host.sent.some((request) => request.method === 'turn.send'))
})

test('closing the window\'s view of a session closes nothing: no request is sent and the session stays attached', async () => {
  const host = aHost()
  await host.client.act({ kind: 'new' })
  const sent = host.sent.length
  assert.deepEqual(await host.client.act({ kind: 'leave' }), { ok: true })
  assert.equal(host.client.state().session, null)
  assert.equal(host.sent.length, sent, 'nothing was sent')
  assert.ok(!host.sent.some((request) => request.method === 'session.close'))
  // Opening it again selects what this connection already holds and sends no second attach.
  assert.deepEqual(await host.client.act({ kind: 'attach', session: 's1' }), { ok: true })
  assert.equal(host.client.state().session.id, 's1')
  assert.ok(!host.sent.some((request) => request.method === 'session.attach'))
})

test('a host that closes the channel ends the client and the window is told why', async () => {
  const host = aHost()
  await host.client.act({ kind: 'new' })
  host.end()
  assert.equal(host.client.state().channel, 'ended')
  assert.match(host.client.state().endedReason, /closed/)
  assert.equal((await host.client.act({ kind: 'send', session: 's1', text: 'late' })).ok, false)
})

test('a failure while either start-up path loads is reported and does not leave a process with no window', () => {
  // The entry runs as the app starts, so it is read and not run: each import ends in a catch that
  // reports, the host client's by ending the process and the ordinary app's by throwing again.
  const entry = readFileSync('src/main/entry.ts', 'utf8')
  assert.match(entry, /import\('\.\/prototype\/main'\)[\s\S]*?\.catch\([\s\S]*?process\.exit\(1\)/)
  assert.match(entry, /import\('\.\/index'\)\.catch\([\s\S]*?setImmediate\([\s\S]*?throw error/)
})
