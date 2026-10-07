import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { test } from 'node:test'
import { ConnectionLostError, RpcAgentClient, RpcError, UnsupportedError } from '../src/common/index.js'
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
              sequence: 0, turn: 0, status: 'idle', pending: null, rows: [],
            } }) + '\n')
            break
          case 'session.close':
            if (failure === 'write') throw new Error('the close could not be written')
            client.receive(JSON.stringify({ id: request.id, error: { code: 'bad_request', message: 'close refused' } }) + '\n')
            return
          case 'turn.send':
            ok = { turn: 1 }
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
    assert.deepEqual(await session.send('still open'), { turn: 1 })
    client.receive(JSON.stringify({ event: 'session.view.update', session: 's1', data: {
      sequence: 1, turn: 1, status: 'running', pending: null, rows: [],
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
            sequence: 0, turn: 0, status: 'idle', pending: null, rows: [],
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
      data: { sequence: 1, turn: 1, status: 'running', pending: null, rows: [] },
    })).join('\n') + '\n'
    assert.doesNotThrow(() => client.receive(updates))
    assert.equal(first.view.sequence, 1)
    assert.equal(second.view.sequence, 1)
    assert.deepEqual(seen, [null])

    const inFlight = client.raw('turn.send')
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
