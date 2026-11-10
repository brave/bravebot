import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { test } from 'node:test'
import { ConnectionLostError, ProtocolError, RpcAgentClient, RpcError, StaleActionError, UnsupportedError } from '../src/common/index.js'
import { rawRequest } from '../src/common/client.js'
import { FIXTURES } from './support/scenario.js'

const contract = JSON.parse(readFileSync(join(FIXTURES, 'wire-contract.json'), 'utf8')) as { capability: Record<string, unknown> }

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

/** The bridge refuses a send without an id, and a repeated id would read as the same send again. */
test('each send carries a message id of its own in the shape the bridge takes', async () => {
  const ids: unknown[] = []
  let client: RpcAgentClient
  client = new RpcAgentClient({
    write(line) {
      const request = JSON.parse(line) as { id: number; method: string; params: Record<string, unknown> }
      let ok: unknown = {}
      if (request.method === 'agent.info') ok = { capabilities: { sessionView: contract.capability } }
      if (request.method === 'session.new') ok = { session: 's1' }
      if (request.method === 'session.view.start') {
        client.receive(JSON.stringify({ event: 'session.view.initial', session: 's1', data: {
          sequence: 0, turn: 0, target: 0, status: 'idle', pending: null, rows: [],
        } }) + '\n')
      }
      if (request.method === 'turn.send') {
        ids.push(request.params.messageId)
        ok = { turn: ids.length, target: ids.length }
      }
      client.receive(JSON.stringify({ id: request.id, ok }) + '\n')
    },
  }, { workspaces: [{ id: 'work', name: 'Work', directory: '/work' }] })
  const session = await client.createSession({ workspace: 'work' })
  await session.send('same words')
  await session.send('same words')
  assert.equal(ids.length, 2)
  for (const id of ids) {
    assert.equal(typeof id, 'string', 'a send carried no id')
    assert.match(id as string, /^[A-Za-z0-9_-]{1,64}$/, `${id as string} is not an id the bridge takes`)
  }
  assert.notEqual(ids[0], ids[1], 'two sends of the same words carried one id')
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

test('the stdio client reports that it lists no sessions and refuses to list or open one without sending', async () => {
  const sent: string[] = []
  const client: RpcAgentClient = new RpcAgentClient({
    write(line) {
      const request = JSON.parse(line) as { id: number; method: string }
      sent.push(request.method)
      client.receive(JSON.stringify({ id: request.id, ok: { capabilities: { sessionView: contract.capability } } }) + '\n')
    },
  })
  assert.equal((await client.describe()).sessionList, false)
  await assert.rejects(client.listSessions(), UnsupportedError)
  await assert.rejects(client.openSession('s1'), UnsupportedError)
  assert.deepEqual(sent, ['agent.info'])
})

/**
 * A bridge for session s1 that numbers rows and updates as the Rust view does, keeps its most
 * recent `retainedRows`, and answers `session.view.resync` with a snapshot of them unless `refuse`.
 * `drop` leaves out the updates with those sequences, as a missed line would. `hold` keeps the
 * snapshot and the resync answer back until `release` is called, from the resync it numbers on
 * when it is a number.
 */
function aBridge(options: { capability?: Record<string, unknown>; drop?: number | number[]; refuse?: boolean; hold?: boolean | number } = {}) {
  const dropped = [options.drop ?? []].flat()
  const holdFrom = options.hold === true ? 1 : options.hold === false || options.hold === undefined ? Infinity : options.hold
  let resyncs = 0
  const capability = options.capability ?? (contract.capability as Record<string, unknown>)
  const retained = (capability.retainedRows as number | undefined) ?? Infinity
  const rows: { id: number; turn: number; kind: string; event: null; data: string; resolved: boolean }[] = []
  const sent: string[] = []
  let sequence = 0
  let held: (() => void) | null = null
  let client: RpcAgentClient
  const state = (rows: unknown[]) => ({ sequence, turn: 1, target: 1, status: 'running', pending: null, rows })
  client = new RpcAgentClient({
    write(line) {
      const request = JSON.parse(line) as { id: number; method: string }
      sent.push(request.method)
      let reply: unknown = { ok: {} }
      let snapshot: (() => void) | null = null
      if (request.method === 'agent.info') reply = { ok: { capabilities: { sessionView: capability } } }
      else if (request.method === 'session.new') reply = { ok: { session: 's1' } }
      else if (request.method === 'session.view.start') {
        client.receive(JSON.stringify({ event: 'session.view.initial', session: 's1', data: { ...state([]), turn: 0, target: 0, status: 'idle' } }) + '\n')
      } else if (request.method === 'session.view.resync') {
        if (options.refuse) reply = { error: { code: 'no_such_session', message: 'gone' } }
        else {
          // The snapshot is taken when it is written, as the bridge takes it under the update lock.
          snapshot = () => {
            const kept = rows.slice(-retained)
            client.receive(JSON.stringify({ event: 'session.view.snapshot', session: 's1', data: { ...state(kept), omitted: rows.length - kept.length } }) + '\n')
            reply = { ok: { version: 1, sequence, omitted: rows.length - kept.length } }
          }
        }
      }
      const answer = () => {
        snapshot?.()
        client.receive(JSON.stringify({ id: request.id, ...(reply as object) }) + '\n')
      }
      if (request.method === 'session.view.resync' && ++resyncs >= holdFrom) held = answer
      else answer()
    },
  }, { workspaces: [{ id: 'work', name: 'Work', directory: '/work' }] })
  return {
    client,
    sent,
    rows,
    /** Add a row, or change the row `changing`, and send the update unless it is the dropped one. */
    emit(changing?: number) {
      sequence++
      let row
      if (changing === undefined) {
        row = { id: rows.length + 1, turn: 1, kind: 'narration', event: null, data: `row ${rows.length + 1}`, resolved: false }
        rows.push(row)
      } else {
        row = { ...rows[changing - 1]!, data: `changed ${changing}` }
        rows[changing - 1] = row
      }
      if (!dropped.includes(sequence)) client.receive(JSON.stringify({ event: 'session.view.update', session: 's1', data: state([row]) }) + '\n')
    },
    release() {
      held?.()
    },
  }
}

test('a missed update is recovered from a snapshot, and the updates after it leave what a later snapshot holds', async () => {
  const bridge = aBridge({ drop: 150 })
  const session = await bridge.client.createSession({ workspace: 'work' })
  for (let n = 0; n < 400; n++) bridge.emit(n % 3 === 2 ? bridge.rows.length - 1 : undefined)
  await Promise.resolve()
  assert.deepEqual(bridge.sent.filter((method) => method === 'session.view.resync').length, 1)
  assert.equal(session.view.ended, null)
  const retained = contract.capability.retainedRows as number
  assert.deepEqual(session.view.rows, bridge.rows.slice(-retained))
  assert.equal(session.view.omitted, bridge.rows.length - retained)
  assert.equal(session.view.sequence, 400)
})

test('a view waiting for a snapshot ignores updates and refuses sends without sending them', async () => {
  const bridge = aBridge({ drop: 2, hold: true })
  const session = await bridge.client.createSession({ workspace: 'work' })
  bridge.emit()
  bridge.emit()
  bridge.emit()
  assert.equal(session.view.resyncing, true)
  bridge.emit()
  assert.equal(session.view.sequence, 1)
  await assert.rejects(session.send('now'), StaleActionError)
  assert.ok(!bridge.sent.includes('turn.send'))
  bridge.release()
  await new Promise((resolve) => setImmediate(resolve))
  assert.equal(session.view.ended, null)
  assert.equal(session.view.sequence, 4)
  assert.equal(session.view.rows.length, 4)
})

test('the answer to an earlier resync does not end a view waiting for a later one', async () => {
  // The first answer is read before its callback runs, so a second gap in the same read asks again.
  const bridge = aBridge({ drop: [2, 5], hold: 2 })
  const session = await bridge.client.createSession({ workspace: 'work' })
  for (let n = 0; n < 6; n++) bridge.emit()
  await new Promise((resolve) => setImmediate(resolve))
  assert.equal(bridge.sent.filter((method) => method === 'session.view.resync').length, 2)
  assert.equal(session.view.ended, null)
  assert.equal(session.view.resyncing, true)
  bridge.release()
  await new Promise((resolve) => setImmediate(resolve))
  assert.equal(session.view.ended, null)
  assert.equal(session.view.sequence, 6)
})

test('a refused resync ends the view as a sequence gap', async () => {
  const bridge = aBridge({ drop: 2, refuse: true })
  const session = await bridge.client.createSession({ workspace: 'work' })
  bridge.emit()
  bridge.emit()
  bridge.emit()
  await new Promise((resolve) => setImmediate(resolve))
  assert.equal(session.view.ended?.reason, 'sequence_gap')
})

test('a runtime from before snapshots ends the view at a gap without asking for one', async () => {
  const { resync: _resync, retainedRows: _retained, ...older } = contract.capability as Record<string, unknown>
  const bridge = aBridge({ capability: { ...older, reconnect: false }, drop: 2 })
  const session = await bridge.client.createSession({ workspace: 'work' })
  bridge.emit()
  bridge.emit()
  bridge.emit()
  assert.equal(session.view.ended?.reason, 'sequence_gap')
  assert.ok(!bridge.sent.includes('session.view.resync'))
})
