import assert from 'node:assert/strict'
import { test } from 'node:test'
import {ConnectionLostError, RpcError, type BridgeEvent } from '../src/common/index.js'
import { RpcConnection } from '../src/common/connection.js'

function harness() {
  const written: { id: number; method: string }[] = []
  const events: BridgeEvent[] = []
  const closed: string[] = []
  const diagnostics: string[] = []
  const connection = new RpcConnection(
    { write: (line) => written.push(JSON.parse(line) as { id: number; method: string }) },
    {
      onEvent: (event) => events.push(event),
      onClosed: (detail) => closed.push(detail),
      onDiagnostic: (message) => diagnostics.push(message),
    },
  )
  return { connection, written, events, closed, diagnostics }
}

test('responses are matched by id, not by the order requests were made', async () => {
  const { connection, written } = harness()
  const first = connection.request('first')
  const second = connection.request('second')
  const [a, b] = written
  connection.receive(JSON.stringify({ id: b!.id, ok: 'for second' }) + '\n' + JSON.stringify({ id: a!.id, ok: 'for first' }) + '\n')
  assert.equal(await first, 'for first')
  assert.equal(await second, 'for second')
})

test('an event written before its response is delivered before that response settles', async () => {
  const { connection, written, events } = harness()
  const order: string[] = []
  const pending = connection.request('session.new', {}, () => order.push(`sync with ${events.length} events`)).then(() => order.push('settled'))
  const id = written[0]!.id
  connection.receive(
    JSON.stringify({ event: 'trust.request', session: 's1', data: {} }) + '\n' +
      JSON.stringify({ id, ok: { session: 's1' } }) + '\n' +
      JSON.stringify({ event: 'session.view.initial', session: 's1', data: {} }) + '\n',
  )
  await pending
  assert.deepEqual(order, ['sync with 1 events', 'settled'])
  assert.deepEqual(events.map((event) => event.event), ['trust.request', 'session.view.initial'])
})

test('an error response rejects with the bridge code', async () => {
  const { connection, written } = harness()
  const pending = connection.request('turn.send')
  connection.receive(JSON.stringify({ id: written[0]!.id, error: { code: 'turn_in_flight', message: 'busy' } }) + '\n')
  await assert.rejects(pending, (error: RpcError) => error.code === 'turn_in_flight' && error.message === 'busy')
})

test('unreadable lines and unmatched responses are reported and do not disturb later ones', async () => {
  const { connection, written, diagnostics } = harness()
  const pending = connection.request('agent.info')
  connection.receive('garbage\n{"id": 999, "ok": 1}\n' + JSON.stringify({ id: written[0]!.id, ok: 'fine' }) + '\n')
  assert.equal(await pending, 'fine')
  assert.equal(diagnostics.length, 2)
})

test('ending the connection fails every request in flight and refuses new ones', async () => {
  const { connection, closed } = harness()
  const one = connection.request('a')
  const two = connection.request('b')
  connection.transportClosed('eof')
  await assert.rejects(one, ConnectionLostError)
  await assert.rejects(two, ConnectionLostError)
  await assert.rejects(connection.request('c'), ConnectionLostError)
  assert.deepEqual(closed, ['eof'])
  connection.transportClosed('again')
  assert.deepEqual(closed, ['eof'])
})

test('a failed write rejects that request alone', async () => {
  const connection = new RpcConnection(
    { write: () => { throw new Error('broken pipe') } },
    { onEvent: () => undefined, onClosed: () => undefined },
  )
  await assert.rejects(connection.request('a'), (error: RpcError) => error.code === 'write_failed')
})

test('an unterminated flood ends the connection', async () => {
  const { connection, closed } = harness()
  const pending = connection.request('a')
  connection.receive('x'.repeat(17 * 1024 * 1024))
  await assert.rejects(pending, ConnectionLostError)
  assert.equal(closed.length, 1)
})

test('a request whose write failed leaves no deadline behind to close the connection later', async () => {
  const fires: (() => void)[] = []
  const closed: string[] = []
  let failing = true
  const connection = new RpcConnection(
    { write: () => { if (failing) throw new Error('broken pipe') } },
    { onEvent: () => undefined, onClosed: (detail) => closed.push(detail) },
    { ms: 10, schedule: (_ms, fire) => { fires.push(fire); return () => { fires.splice(fires.indexOf(fire), 1) } } },
  )
  await assert.rejects(connection.request('a'), (error: RpcError) => error.code === 'write_failed')
  failing = false
  void connection.request('b').catch(() => undefined)
  assert.equal(fires.length, 1, 'only the live request has a deadline')
  assert.deepEqual(closed, [])
})

test('a diagnostic hook that throws cannot discard the responses and events that follow in the same chunk', async () => {
  const written: { id: number }[] = []
  const events: string[] = []
  const connection = new RpcConnection(
    { write: (line) => written.push(JSON.parse(line) as { id: number }) },
    {
      onEvent: (event) => events.push(event.event),
      onClosed: () => undefined,
      onDiagnostic: () => { throw new Error('hook failure') },
    },
  )
  const pending = connection.request('agent.info')
  connection.receive(
    'not json\n{"id": 999, "ok": 1}\n[1]\n' +
      JSON.stringify({ id: written[0]!.id, ok: 'answered' }) + '\n' +
      JSON.stringify({ event: 'agent.ready', data: {} }) + '\n',
  )
  assert.equal(await pending, 'answered')
  assert.deepEqual(events, ['agent.ready'])
})

test('an unreadable line is reported by length and its text is not echoed', () => {
  const reports: string[] = []
  const connection = new RpcConnection({ write: () => undefined }, {
    onEvent: () => undefined,
    onClosed: () => undefined,
    onDiagnostic: (message) => reports.push(message),
  })
  connection.receive('transcript text SECRET-PROMPT-CONTENT, not json\n')
  assert.equal(reports.length, 1)
  assert.equal(reports[0]!.includes('SECRET-PROMPT-CONTENT'), false)
})

test('a bridge refusal has no effect and a lost connection may have had one', async () => {
  const { connection, written } = harness()
  const refused = connection.request('turn.send')
  connection.receive(JSON.stringify({ id: written[0]!.id, error: { code: 'turn_in_flight', message: 'busy' } }) + '\n')
  await assert.rejects(refused, (error: RpcError) => error.outcome === 'rejected')
  const lost = connection.request('turn.send')
  connection.transportClosed('eof')
  await assert.rejects(lost, (error: RpcError) => error.outcome === 'unknown')
})

test('a write that fails leaves its outcome unknown', async () => {
  const connection = new RpcConnection(
    { write: () => { throw new Error('broken pipe') } },
    { onEvent: () => undefined, onClosed: () => undefined },
  )
  await assert.rejects(connection.request('a'), (error: RpcError) => error.code === 'write_failed' && error.outcome === 'unknown')
})

test('requests waiting for an answer, and one request, are bounded', async () => {
  const { connection, written } = harness()
  const waiting = Array.from({ length: 256 }, () => connection.request('agent.info'))
  await assert.rejects(connection.request('agent.info'), (error: RpcError) => error.code === 'request_limit' && error.outcome === 'rejected')
  assert.equal(written.length, 256, 'the refused request was not written')
  await assert.rejects(
    harness().connection.request('turn.send', { prompt: 'x'.repeat(8 * 1024 * 1024) }),
    (error: RpcError) => error.code === 'frame_limit' && error.outcome === 'rejected',
  )
  connection.transportClosed('done')
  await Promise.allSettled(waiting)
})

test('parameters that cannot be written fail the request instead of throwing at the call', async () => {
  const { connection, written } = harness()
  const circular: Record<string, unknown> = {}
  circular.self = circular
  await assert.rejects(connection.request('turn.send', circular), (error: RpcError) => error.code === 'bad_request' && error.outcome === 'rejected')
  await assert.rejects(connection.request('turn.send', { n: 10n }), (error: RpcError) => error.code === 'bad_request')
  assert.equal(written.length, 0)
})

test('cancel and close are not held back by the limit on waiting requests', async () => {
  const { connection, written } = harness()
  const waiting = Array.from({ length: 256 }, () => connection.request('agent.info'))
  const control = connection.request('turn.cancel', {}, undefined, { control: true })
  assert.equal(written.length, 257, 'the control request was written')
  connection.transportClosed('done')
  await Promise.allSettled([...waiting, control])
})

test('the bridge reporting its own bug may have acted, and any other refusal had no effect', async () => {
  const { connection, written } = harness()
  const bug = connection.request('turn.send')
  const refusal = connection.request('turn.send')
  connection.receive(
    JSON.stringify({ id: written[0]!.id, error: { code: 'internal', message: 'a bug' } }) + '\n' +
      JSON.stringify({ id: written[1]!.id, error: { code: 'turn_in_flight', message: 'busy' } }) + '\n',
  )
  await assert.rejects(bug, (error: RpcError) => error.outcome === 'unknown')
  await assert.rejects(refusal, (error: RpcError) => error.outcome === 'rejected')
})
