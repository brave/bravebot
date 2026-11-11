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

/**
 * A host holding session `s7`, left by its controller at sequence 4 with rows 1 to 3 dropped. It
 * answers `session.attach` as `answer` says: with the snapshot first, the snapshot and the startup
 * question, no snapshot, or a refusal.
 */
function aHostHoldingALeftSession(answer: 'snapshot' | 'trust' | 'nothing' | 'refused') {
  const sent: { method: string; params: Record<string, unknown> }[] = []
  const prompt = { id: 4, turn: 2, kind: 'prompt', event: null, data: { text: 'go' }, resolved: false }
  const client: RpcAgentClient = new RpcAgentClient({
    write(line) {
      const request = JSON.parse(line) as { id: number; method: string; params: Record<string, unknown> }
      sent.push({ method: request.method, params: request.params })
      const reply = (body: Record<string, unknown>): void => client.receive(JSON.stringify({ id: request.id, ...body }) + '\n')
      const event = (name: string, data: unknown): void => client.receive(JSON.stringify({ event: name, session: 's7', data }) + '\n')
      if (request.method === 'agent.info') return reply({ ok: { capabilities: { sessionView: contract.capability } } })
      if (request.method === 'instance.info') {
        return reply({ ok: { instance: 'run-1', inspection: 'refused', sessions: [{ session: 's7', state: 'open', working: false, controlled: false }] } })
      }
      if (request.method === 'turn.send') return reply({ ok: { turn: 3, target: 9 } })
      if (request.method !== 'session.attach') return assert.fail(`unexpected method ${request.method}`)
      if (answer === 'refused') return reply({ error: { code: 'not_allowed', message: 'another connection controls that session' } })
      const status = answer === 'trust' ? 'awaiting_trust' : 'completed'
      if (answer !== 'nothing') event('session.view.snapshot', { sequence: 4, turn: 2, target: 8, status, pending: null, rows: [prompt], omitted: 3 })
      if (answer === 'trust') event('trust.request', { directory: '/work' })
      reply({ ok: { version: 1, sequence: 4, omitted: 3 } })
    },
  })
  return { client, sent }
}

test('a session taken over starts its view from the snapshot the host sends, and is driven from there', async () => {
  const { client, sent } = aHostHoldingALeftSession('snapshot')
  assert.deepEqual(await client.instance(), { instance: 'run-1', sessions: [{ session: 's7', state: 'open', working: false, controlled: false }] })
  const session = await client.attach('s7')
  assert.equal(session.id, 's7')
  assert.deepEqual(
    { sequence: session.view.sequence, turn: session.view.turn, target: session.view.target, status: session.view.status, omitted: session.view.omitted, rows: session.view.rows.map((row) => row.id) },
    { sequence: 4, turn: 2, target: 8, status: 'completed', omitted: 3, rows: [4] },
  )
  assert.equal(session.startupTrust, null)
  client.receive(JSON.stringify({ event: 'session.view.update', session: 's7', data: { sequence: 5, turn: 3, target: 9, status: 'running', pending: null, rows: [], omitted: 3 } }) + '\n')
  assert.equal(session.view.sequence, 5, 'the updates after the snapshot apply to it')
  assert.deepEqual(await session.send('again'), { turn: 3, target: 9 })
  assert.deepEqual(sent.find((request) => request.method === 'session.attach')?.params, { session: 's7' })
})

test('a session taken over with its startup question unanswered offers that question', async () => {
  const { client } = aHostHoldingALeftSession('trust')
  const session = await client.attach('s7')
  assert.equal(session.view.status, 'awaiting_trust')
  assert.deepEqual(session.startupTrust, { directory: '/work' })
})

test('a session the client holds is not taken over again, and nothing is sent for it', async () => {
  const { client, sent } = aHostHoldingALeftSession('snapshot')
  await client.attach('s7')
  await assert.rejects(client.attach('s7'), (error: unknown) => error instanceof RpcError && error.code === 'bad_request' && error.outcome === 'rejected')
  assert.equal(sent.filter((request) => request.method === 'session.attach').length, 1)
})

test('a take-over answered before its snapshot, or refused, leaves no session behind to route to', async () => {
  for (const answer of ['nothing', 'refused'] as const) {
    const { client, sent } = aHostHoldingALeftSession(answer)
    await assert.rejects(client.attach('s7'), answer === 'nothing' ? ProtocolError : (error: unknown) => error instanceof RpcError && error.code === 'not_allowed')
    // A snapshot that arrives now has no attach waiting for it, so it starts nothing.
    client.receive(JSON.stringify({ event: 'session.view.snapshot', session: 's7', data: { sequence: 4, turn: 2, target: 8, status: 'completed', pending: null, rows: [], omitted: 0 } }) + '\n')
    // The session is not held, so a later attempt is sent rather than refused locally.
    await assert.rejects(client.attach('s7'))
    assert.equal(sent.filter((request) => request.method === 'session.attach').length, 2, answer)
  }
})

test('an instance listing in a shape the host does not send is refused', async () => {
  for (const ok of [{ sessions: [] }, { instance: 'run-1', sessions: [{ session: 's7', state: 'gone', working: false, controlled: false }] }]) {
    const client: RpcAgentClient = new RpcAgentClient({
      write(line) {
        const request = JSON.parse(line) as { id: number }
        client.receive(JSON.stringify({ id: request.id, ok }) + '\n')
      },
    })
    await assert.rejects(client.instance(), ProtocolError)
  }
})
