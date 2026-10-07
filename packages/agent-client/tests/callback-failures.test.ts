import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { test } from 'node:test'
import {CapabilityError, ConnectionLostError, ProtocolError, RpcAgentClient, RpcError } from '../src/common/index.js'
import { RpcConnection } from '../src/common/connection.js'
import { FIXTURES } from './support/scenario.js'

const contract = JSON.parse(readFileSync(join(FIXTURES, 'wire-contract.json'), 'utf8')) as { capability: unknown }

interface Sent { id: number; method: string; params: Record<string, unknown> }

function rig(options: ConstructorParameters<typeof RpcAgentClient>[1] = {}) {
  const sent: Sent[] = []
  const client = new RpcAgentClient({ write: (line) => sent.push(JSON.parse(line) as Sent) }, {
    workspaces: [{ id: 'work', name: 'Work', directory: '/work' }],
    ...options,
  })
  const answer = (method: string, result: { ok: unknown } | { error: { code: string; message: string } }): void => {
    const request = sent.filter((item) => item.method === method).at(-1)
    assert.ok(request, `${method} was sent`)
    client.receive(JSON.stringify({ id: request.id, ...result }) + '\n')
  }
  const event = (name: string, session: string, data: unknown): void => {
    client.receive(JSON.stringify({ event: name, session, data }) + '\n')
  }
  return { client, sent, answer, event }
}

const settle = (): Promise<void> => new Promise((resolve) => setImmediate(resolve))

for (const ending of ['transport', 'deadline'] as const) {
  for (const failure of ['throw', 'reject'] as const) {
    /** Direct connection users need the same failure containment as the client wrapper. */
    test(`a direct connection contains a close callback ${failure} on ${ending} shutdown`, async () => {
      const reports: string[] = []
      let closed = 0
      let cancelled = 0
      let fire: (() => void) | undefined
      const connection = new RpcConnection({ write: () => undefined }, {
        onEvent: () => undefined,
        onClosed: () => {
          closed++
          const error = new Error('close callback failure')
          if (failure === 'throw') throw error
          return Promise.reject(error)
        },
        onDiagnostic: async (message) => {
          reports.push(message)
          throw new Error('diagnostic failure')
        },
      }, { ms: 5, schedule: (_ms, run) => { fire = run; return () => { cancelled++ } } })
      const first = assert.rejects(connection.request('first'), ConnectionLostError)
      const second = assert.rejects(connection.request('second'), ConnectionLostError)
      assert.doesNotThrow(() => ending === 'transport' ? connection.transportClosed('eof') : fire!())
      await Promise.all([first, second])
      await settle()
      assert.equal(connection.closed, true)
      assert.equal(cancelled, 2)
      assert.equal(reports.length, 1)
      assert.match(reports[0]!, /close callback failure/)
      connection.transportClosed('again')
      assert.equal(closed, 1)
      await assert.rejects(connection.request('later'), ConnectionLostError)
    })
  }
}

test('a client close callback that throws does not escape a deadline or the pending requests', async () => {
  const reports: string[] = []
  let fire: (() => void) | undefined
  const { client } = rig({
    onClosed: () => { throw new Error('close callback failure') },
    onDiagnostic: (message) => reports.push(message),
    deadlines: { ms: 5, schedule: (_ms, run) => { fire = run; return () => undefined } },
  })
  const pending = client.describe()
  assert.doesNotThrow(() => fire!())
  await assert.rejects(pending, ConnectionLostError)
  assert.ok(reports.some((message) => message.includes('close callback failure')))
})

test('an event handler that throws is reported and the rest of the chunk is still read', async () => {
  const sent: Sent[] = []
  const seen: string[] = []
  const reports: string[] = []
  const connection = new RpcConnection({ write: (line) => sent.push(JSON.parse(line) as Sent) }, {
    onEvent: (event) => {
      seen.push(event.event)
      if (event.event === 'first') throw new Error('handler failure')
    },
    onClosed: () => undefined,
    onDiagnostic: (message) => reports.push(message),
  })
  const pending = connection.request('agent.info')
  assert.doesNotThrow(() => connection.receive(
    JSON.stringify({ event: 'first', session: 's1', data: {} }) + '\n' +
      JSON.stringify({ event: 'second', session: 's1', data: {} }) + '\n' +
      JSON.stringify({ id: sent[0]!.id, ok: 'fine' }) + '\n',
  ))
  assert.deepEqual(seen, ['first', 'second'])
  assert.equal(await pending, 'fine')
  assert.ok(reports.some((message) => message.includes('handler failure')))
})

test('a response handler that throws is reported, its request still settles and the rest of the chunk is read', async () => {
  const sent: Sent[] = []
  const seen: string[] = []
  const reports: string[] = []
  const connection = new RpcConnection({ write: (line) => sent.push(JSON.parse(line) as Sent) }, {
    onEvent: (event) => seen.push(event.event),
    onClosed: () => undefined,
    onDiagnostic: (message) => reports.push(message),
  })
  const accepted = connection.request('session.new', {}, () => { throw new Error('sync failure') })
  const refused = connection.request('session.new', {}, () => { throw new Error('sync failure on error') })
  assert.doesNotThrow(() => connection.receive(
    JSON.stringify({ id: sent[0]!.id, ok: 'made' }) + '\n' +
      JSON.stringify({ id: sent[1]!.id, error: { code: 'busy', message: 'later' } }) + '\n' +
      JSON.stringify({ event: 'after', session: 's1', data: {} }) + '\n',
  ))
  assert.equal(await accepted, 'made')
  await assert.rejects(refused, (error: RpcError) => error.code === 'busy')
  assert.deepEqual(seen, ['after'])
  assert.ok(reports.some((message) => message.includes('sync failure on error')))
})

async function describedRig(options: ConstructorParameters<typeof RpcAgentClient>[1] = {}) {
  const made = rig(options)
  const described = made.client.describe()
  made.answer('agent.info', { ok: { capabilities: { sessionView: contract.capability } } })
  await described
  return made
}

const initial = { sequence: 0, turn: 0, status: 'awaiting_trust', pending: null, rows: [] }

/** Create a session through the scripted bridge. `during` runs after the request and before its response. */
async function open(made: Awaited<ReturnType<typeof describedRig>>, id: string, trust?: unknown, during?: () => void) {
  const created = made.client.createSession({ workspace: 'work' })
  await settle()
  if (trust !== undefined) made.event('trust.request', id, trust)
  during?.()
  made.answer('session.new', { ok: { session: id } })
  await settle()
  made.event('session.view.initial', id, initial)
  made.answer('session.view.start', { ok: {} })
  return created
}

test('startup trust questions that arrive when no creation is waiting, or are left by one that failed, never crowd out the next', async () => {
  const made = await describedRig()
  for (let at = 0; at < 100; at++) made.event('trust.request', `idle${at}`, {})
  const first = await open(made, 'one', { directory: '/work' })
  assert.deepEqual(first.startupTrust, { directory: '/work' })
  const failed = made.client.createSession({ workspace: 'work' })
  await settle()
  for (let at = 0; at < 100; at++) made.event('trust.request', `raw${at}`, {})
  made.answer('session.new', { error: { code: 'busy', message: 'later' } })
  await assert.rejects(failed, (error: RpcError) => error.code === 'busy')
  const session = await open(made, 'two', { directory: '/work2' })
  assert.deepEqual(session.startupTrust, { directory: '/work2' })
})

test('a flood of other trust questions during a creation does not displace its own', async () => {
  const made = await describedRig()
  const session = await open(made, 'real', { directory: '/work' }, () => {
    for (let at = 0; at < 200; at++) made.event('trust.request', `other${at}`, {})
  })
  assert.deepEqual(session.startupTrust, { directory: '/work' })
})

test('an answered trust question is no longer offered, and a refused answer leaves it', async () => {
  const made = await describedRig()
  const session = await open(made, 's1', { directory: '/work' })
  const refused = session.answerTrust(true)
  made.answer('trust.reply', { error: { code: 'busy', message: 'later' } })
  await assert.rejects(refused, (error: RpcError) => error.code === 'busy')
  assert.deepEqual(session.startupTrust, { directory: '/work' })
  const accepted = session.answerTrust(true)
  made.answer('trust.reply', { ok: {} })
  await accepted
  assert.equal(session.startupTrust, null)
})

test('a close that fails because the connection was lost still ends the view and reports the loss', async () => {
  const made = await describedRig()
  const session = await open(made, 's1')
  made.client.transportClosed('gone')
  assert.equal(session.view.ended?.reason, 'connection_lost')
  await assert.rejects(session.close(), ConnectionLostError)
})

test('a failed startup reports its error at once and its cleanup cannot end the connection', async () => {
  const open_: { cancelled: boolean }[] = []
  const closed: string[] = []
  const made = await describedRig({
    onClosed: (detail) => closed.push(detail),
    deadlines: { ms: 5, schedule: () => { const entry = { cancelled: false }; open_.push(entry); return () => { entry.cancelled = true } } },
  })
  const before = open_.filter((entry) => !entry.cancelled).length
  const created = made.client.createSession({ workspace: 'work' })
  await settle()
  made.answer('session.new', { ok: { session: 's1' } })
  await settle()
  made.event('session.view.initial', 's1', { ...initial, sequence: 3 })
  made.answer('session.view.start', { ok: {} })
  await assert.rejects(created, ProtocolError)
  assert.equal(made.sent.filter((item) => item.method === 'session.close').length, 1, 'the cleanup was sent')
  assert.equal(open_.filter((entry) => !entry.cancelled).length, before, 'the cleanup has no deadline')
  assert.deepEqual(closed, [])
})

test('a failed agent.info can be retried, while a refused capability stays final', async () => {
  const { client, sent, answer } = rig()
  const first = client.describe()
  answer('agent.info', { error: { code: 'busy', message: 'try later' } })
  await assert.rejects(first, (error: RpcError) => error.code === 'busy')
  const retry = client.describe()
  assert.equal(sent.filter((item) => item.method === 'agent.info').length, 2)
  answer('agent.info', { ok: { capabilities: { sessionView: contract.capability } } })
  assert.equal((await retry).sessionView.version, 1)

  const refused = rig()
  const old = refused.client.describe()
  refused.answer('agent.info', { ok: { capabilities: {} } })
  await assert.rejects(old, CapabilityError)
  await assert.rejects(refused.client.describe(), CapabilityError)
  assert.equal(refused.sent.filter((item) => item.method === 'agent.info').length, 1)
})
