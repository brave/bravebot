import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { test } from 'node:test'
import { ConnectionLostError, ProtocolError, RpcAgentClient, RpcError, UnsupportedError } from '../src/common/index.js'
import { rawRequest } from '../src/common/client.js'
import { FIXTURES } from './support/scenario.js'

const contract = JSON.parse(readFileSync(join(FIXTURES, 'wire-contract.json'), 'utf8')) as { capability: unknown }

for (const failure of ['write', 'reply'] as const) {
  /** A refused close leaves the runtime session alive, so its view must keep receiving events. */
  test(`a failed close ${failure} preserves session updates and connection-loss reporting`, async () => {
    let client: RpcAgentClient
    client = new RpcAgentClient({
      write(line) {
        const request = JSON.parse(line) as { id: number; method: string }
        let ok: unknown = {}
        switch (request.method) {
          case 'agent.info':
            ok = { capabilities: { sessionView: contract.capability } }
            break
          case 'session.new':
            ok = { session: 's1' }
            break
          case 'session.view.start':
            client.receive(JSON.stringify({ event: 'session.view.initial', session: 's1', data: {
              sequence: 0, turn: 0, target: 0, status: 'idle', pending: null, rows: [],
            } }) + '\n')
            break
          case 'session.close':
            if (failure === 'write') throw new Error('the close could not be written')
            client.receive(JSON.stringify({ id: request.id, error: { code: 'bad_request', message: 'close refused' } }) + '\n')
            return
          case 'turn.send':
            ok = { turn: 1, target: 1 }
            break
          default:
            assert.fail(`unexpected method ${request.method}`)
        }
        client.receive(JSON.stringify({ id: request.id, ok }) + '\n')
      },
    }, { workspaces: [{ id: 'work', name: 'Work', directory: '/work' }] })
    const session = await client.createSession({ workspace: 'work' })
    await assert.rejects(session.close(), (error: RpcError) =>
      error.code === (failure === 'write' ? 'write_failed' : 'bad_request'))
    assert.deepEqual(await session.send('still open'), { turn: 1, target: 1 })
    client.receive(JSON.stringify({ event: 'session.view.update', session: 's1', data: {
      sequence: 1, turn: 1, target: 1, status: 'running', pending: null, rows: [],
    } }) + '\n')
    assert.equal(session.view.sequence, 1)
    assert.equal(session.view.status, 'running')
    client.transportClosed('eof')
    assert.equal(session.view.ended?.reason, 'connection_lost')
    await assert.rejects(session.send('after eof'), UnsupportedError)
  })
}

for (const failure of ['listener', 'formatting', 'diagnostic'] as const) {
  /** Subscriber code must not block later subscribers, other sessions, or transport cleanup. */
  test(`a ${failure} failure cannot interrupt view updates or connection shutdown`, async () => {
    let client: RpcAgentClient
    const reports: string[] = []
    const closed: string[] = []
    let handle = 0
    client = new RpcAgentClient({
      write(line) {
        const request = JSON.parse(line) as { id: number; method: string; params: { session: string } }
        let ok: unknown = {}
        if (request.method === 'agent.info') ok = { capabilities: { sessionView: contract.capability } }
        else if (request.method === 'session.new') ok = { session: `s${++handle}` }
        else if (request.method === 'session.view.start') {
          client.receive(JSON.stringify({ event: 'session.view.initial', session: request.params.session, data: {
            sequence: 0, turn: 0, target: 0, status: 'idle', pending: null, rows: [],
          } }) + '\n')
        } else return
        client.receive(JSON.stringify({ id: request.id, ok }) + '\n')
      },
    }, {
      workspaces: [{ id: 'work', name: 'Work', directory: '/work' }],
      onDiagnostic: (message) => {
        reports.push(message)
        if (failure === 'diagnostic') throw new Error('diagnostic failure')
      },
      onClosed: (detail) => closed.push(detail),
    })
    const first = await client.createSession({ workspace: 'work' })
    const second = await client.createSession({ workspace: 'work' })
    first.subscribe(() => { throw failure === 'formatting' ? Object.create(null) : new Error('listener failure') })
    const seen: (string | null)[] = []
    first.subscribe((view) => seen.push(view.ended?.reason ?? null))
    const updates = [first, second].map((session) => JSON.stringify({
      event: 'session.view.update', session: session.id,
      data: { sequence: 1, turn: 1, target: 1, status: 'running', pending: null, rows: [] },
    })).join('\n') + '\n'
    assert.doesNotThrow(() => client.receive(updates))
    assert.equal(first.view.sequence, 1)
    assert.equal(second.view.sequence, 1)
    assert.deepEqual(seen, [null])

    const inFlight = rawRequest(client, 'turn.send')
    inFlight.catch(() => undefined)
    assert.doesNotThrow(() => client.transportClosed('eof'))
    assert.equal(second.view.ended?.reason, 'connection_lost')
    assert.equal(first.view.ended?.reason, 'connection_lost')
    assert.deepEqual(seen, [null, 'connection_lost'])
    assert.deepEqual(closed, ['eof'])
    await assert.rejects(inFlight, ConnectionLostError)
    if (failure === 'formatting') assert.ok(reports.every((message) => message === 'a view listener threw'))
    else assert.ok(reports.every((message) => message.includes('listener failure')))
    assert.equal(reports.length, 2)
    client.transportClosed('again')
    assert.deepEqual(closed, ['eof'])
  })
}

type Startup = { event?: string; data: unknown }

/** A client whose view start emits `events` for session s1 before its successful response. */
function startingWith(events: Startup[], options: { onDiagnostic?: (message: string) => void } = {}): RpcAgentClient {
  let client: RpcAgentClient
  client = new RpcAgentClient({
    write(line) {
      const request = JSON.parse(line) as { id: number; method: string }
      let ok: unknown = {}
      if (request.method === 'agent.info') ok = { capabilities: { sessionView: contract.capability } }
      else if (request.method === 'session.new') ok = { session: 's1' }
      else if (request.method === 'session.view.start') {
        for (const { event, data } of events) {
          client.receive(JSON.stringify({ event: event ?? 'session.view.initial', session: 's1', data }) + '\n')
        }
      }
      client.receive(JSON.stringify({ id: request.id, ok }) + '\n')
    },
  }, { workspaces: [{ id: 'work', name: 'Work', directory: '/work' }], ...options })
  return client
}

const initial = (sequence: number, status = 'idle'): Startup => ({ data: { sequence, turn: 0, target: 0, status, pending: null, rows: [] } })

test('a startup that broke the protocol is refused even when a valid initial view follows', async () => {
  const open = (events: Startup[]) => startingWith(events).createSession({ workspace: 'work' })
  await assert.rejects(open([initial(99), initial(0)]), /sequence 99/)
  await assert.rejects(open([{ data: { nonsense: true } }, initial(0)]), ProtocolError)
  await assert.rejects(open([{ event: 'session.view.update', data: initial(1, 'running').data }, initial(0)]), /before the initial view/)
  const session = await open([initial(0)])
  assert.equal(session.view.ended, null)
})

test('an async view listener or diagnostic hook that rejects is reported and does not escape', async () => {
  const unhandled: unknown[] = []
  const record = (reason: unknown) => unhandled.push(reason)
  process.on('unhandledRejection', record)
  try {
    const reports: string[] = []
    const client = startingWith([initial(0)], {
      onDiagnostic: async (message) => {
        reports.push(message)
        throw new Error('async hook failure')
      },
    })
    const session = await client.createSession({ workspace: 'work' })
    session.subscribe(async () => { throw new Error('async listener failure') })
    client.receive(JSON.stringify({ event: 'session.view.update', session: 's1', data: { sequence: 1, turn: 1, target: 1, status: 'running', pending: null, rows: [] } }) + '\n')
    client.receive('not json\n')
    await new Promise((resolve) => setImmediate(resolve))
    assert.equal(session.view.sequence, 1, 'delivery continued past the failing listener')
    assert.ok(reports.some((message) => message.includes('async listener failure')))
    assert.deepEqual(unhandled, [])
  } finally {
    process.off('unhandledRejection', record)
  }
})

test('a close the bridge answers with no_such_session unregisters the session', async () => {
  let client: RpcAgentClient
  client = new RpcAgentClient({
    write(line) {
      const request = JSON.parse(line) as { id: number; method: string }
      if (request.method === 'session.close') {
        client.receive(JSON.stringify({ id: request.id, error: { code: 'no_such_session', message: 'gone' } }) + '\n')
        return
      }
      if (request.method === 'session.view.start') {
        client.receive(JSON.stringify({ event: 'session.view.initial', session: 's1', data: {
          sequence: 0, turn: 0, target: 0, status: 'idle', pending: null, rows: [],
        } }) + '\n')
      }
      const ok = request.method === 'agent.info' ? { capabilities: { sessionView: contract.capability } }
        : request.method === 'session.new' ? { session: 's1' } : {}
      client.receive(JSON.stringify({ id: request.id, ok }) + '\n')
    },
  }, { workspaces: [{ id: 'work', name: 'Work', directory: '/work' }] })
  const session = await client.createSession({ workspace: 'work' })
  await assert.rejects(session.close(), (error: RpcError) => error.code === 'no_such_session')
  client.receive(JSON.stringify({ event: 'session.view.update', session: 's1', data: {
    sequence: 1, turn: 1, target: 1, status: 'running', pending: null, rows: [],
  } }) + '\n')
  assert.equal(session.view.sequence, 0, 'a session the bridge no longer has receives no updates')
})

test('after a close, the detach ends the view and later events for the session are dropped quietly', async () => {
  const diagnostics: string[] = []
  const send = (client: RpcAgentClient, event: string, data: unknown): void =>
    client.receive(JSON.stringify({ event, session: 's1', data }) + '\n')
  const view = (sequence: number, status: string): unknown => ({ sequence, turn: 0, target: 0, status, pending: null, rows: [] })
  let client: RpcAgentClient
  client = new RpcAgentClient({
    write(line) {
      const request = JSON.parse(line) as { id: number; method: string }
      let ok: unknown = {}
      if (request.method === 'agent.info') ok = { capabilities: { sessionView: contract.capability } }
      if (request.method === 'session.new') ok = { session: 's1' }
      if (request.method === 'session.view.start') send(client, 'session.view.initial', view(0, 'idle'))
      // The bridge detaches the view before it answers the close.
      if (request.method === 'session.close') send(client, 'session.view.update', view(1, 'detached'))
      client.receive(JSON.stringify({ id: request.id, ok }) + '\n')
    },
  }, { workspaces: [{ id: 'work', name: 'Work', directory: '/work' }], onDiagnostic: (message) => diagnostics.push(message) })
  const session = await client.createSession({ workspace: 'work' })
  const outcome = await session.close()
  assert.equal(outcome.viewDetached, true)
  const ended = session.view
  assert.equal(ended.ended?.reason, 'detached')
  send(client, 'session.view.update', view(2, 'running'))
  send(client, 'trust.request', { request: 1 })
  client.transportClosed('eof')
  assert.equal(session.view, ended)
  assert.deepEqual(diagnostics, [])
})
