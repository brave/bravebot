import assert from 'node:assert/strict'
import { existsSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { after, describe, test } from 'node:test'
import {
  ConnectionLostError,
  RpcError,
  UnsupportedError,
  type AgentSession,
  type Pending,
} from '../src/common/index.js'
import { recording, startRig, until, type Rig } from './support/rig.js'
import { rawOn } from './support/raw.js'
import { within } from './support/wait.js'

const rigs: Rig[] = []
async function rig(plans: Parameters<typeof startRig>[0]): Promise<Rig> {
  const made = await startRig(plans)
  rigs.push(made)
  return made
}
after(async () => {
  for (const made of rigs) await made.stop()
})

/** Open a session and answer the startup trust question as the caller says a person did. */
async function trusted(made: Rig, answer = true): Promise<AgentSession> {
  const session = await made.rpc.client.createSession({ workspace: 'project' })
  assert.equal(session.view.status, 'awaiting_trust')
  await session.answerTrust(answer)
  await until(session, 'trust to be recorded', (view) => view.status === 'idle')
  return session
}

function pendingOf(view: { pending: Pending | null }): Pending {
  assert.ok(view.pending, 'a question was expected')
  return view.pending
}

const write = (path: string, contents: string) => [{ tool: { name: 'write_file', arguments: { path, contents } } }, { say: 'finished' }]

describe('a real bravebot-rpc process through the typed client', () => {
  test('the runtime advertises the view and the description drops its state directory', async () => {
    const made = await rig({})
    const info = await made.rpc.client.describe()
    assert.deepEqual(info.sessionView.approvals, ['confirm', 'run', 'fetch', 'ask'])
    assert.equal(info.sessionView.reconnect, false)
    const raw = (await rawOn(made.rpc, 'agent.info')) as Record<string, unknown>
    assert.equal(typeof raw.home, 'string', 'the bridge reports its state directory')
    assert.equal(JSON.stringify(info).includes(raw.home as string), false)
  })

  test('startup trust is explicit: a send before it is refused, and the view moves to idle only after the answer', async () => {
    const made = await rig({ 'plan:plain': [{ say: 'all done' }] })
    const session = await made.rpc.client.createSession({ workspace: 'project' })
    const seen = recording(session)
    assert.deepEqual({ sequence: session.view.sequence, status: session.view.status, rows: session.view.rows }, {
      sequence: 0,
      status: 'awaiting_trust',
      rows: [],
    })
    assert.ok(session.startupTrust, 'the trust question reached the client before the session.new response')
    await assert.rejects(session.send('plan:plain'), (error: RpcError) => error.code === 'bad_request')
    assert.equal(seen.length, 1, 'a refused send changed nothing in the view')
    await session.answerTrust(true)
    const idle = await until(session, 'idle', (view) => view.status === 'idle')
    assert.equal(idle.sequence, 1)
  })

  test('an accepted turn produces an ordered prompt row and a reply row, and a second send while it runs is refused', async () => {
    const made = await rig({ 'plan:busy': [{ say: 'all done', hold: 'model' }] })
    const session = await trusted(made)
    const seen = recording(session)
    assert.deepEqual(await session.send('plan:busy please'), { turn: 1 })
    await made.stub.reached('model')
    const running = await until(session, 'the prompt row', (view) => view.status === 'running' && view.rows.length === 1)
    assert.equal(running.rows[0]?.kind, 'prompt')
    assert.deepEqual((running.rows[0]?.data as { text: string }).text, 'plan:busy please')
    await assert.rejects(session.send('must not appear'), (error: RpcError) => error.code === 'turn_in_flight')
    assert.equal(session.view.rows.length, 1, 'a rejected send produced no row')
    made.stub.release('model')
    const done = await until(session, 'the turn to complete', (view) => view.status === 'completed')
    assert.deepEqual(done.rows.map((row) => row.kind), ['prompt', 'reply'])
    assert.equal(done.rows[1]?.event, 'turn.done')
    assert.equal(done.pending, null)
    const sequences = seen.map((view) => view.sequence)
    assert.deepEqual(sequences, sequences.map((_, at) => sequences[0]! + at), 'every update was the next in sequence')
    assert.equal(done.ended, null)
    // The next turn is accepted straight after completion: nothing about the finished one blocks it.
    assert.deepEqual(await session.send('plan:busy again'), { turn: 2 })
    await until(session, 'the second turn', (view) => view.turn === 2 && view.status === 'completed')
  })

  test('the trust answer decides whether a write is put to the person: trusted writes land, declined ones ask first', async () => {
    const made = await rig({ 'plan:trusted': write('trusted.txt', 'trusted bytes'), 'plan:declined': write('declined.txt', 'declined bytes') })
    const yes = await trusted(made, true)
    await yes.send('plan:trusted')
    const finished = await until(yes, 'the trusted turn to end', (view) => view.status === 'completed')
    assert.equal(finished.rows.some((row) => row.kind === 'approval'), false, 'a trusted workspace was not asked')
    assert.equal(readFileSync(join(made.project, 'trusted.txt'), 'utf8'), 'trusted bytes')

    const no = await trusted(made, false)
    await no.send('plan:declined')
    const asked = await until(no, 'the declined session to ask', (view) => view.status === 'waiting')
    assert.equal(pendingOf(asked).kind, 'confirm')
    assert.equal(existsSync(join(made.project, 'declined.txt')), false)
  })

  test('two sessions with turns running at once keep their own rows and finish in the order the model releases them', async () => {
    const made = await rig({
      'plan:first': [{ say: 'first reply', hold: 'first' }],
      'plan:second': [{ say: 'second reply', hold: 'second' }],
    })
    const a = await trusted(made)
    const b = await trusted(made)
    await a.send('plan:first')
    await b.send('plan:second')
    await made.stub.reached('first')
    await made.stub.reached('second')
    assert.deepEqual([a.view.status, b.view.status], ['running', 'running'], 'both turns are in flight together')
    made.stub.release('second')
    const bDone = await until(b, 'the second session to finish', (view) => view.status === 'completed')
    assert.equal(a.view.status, 'running', 'the first session is unaffected by the second finishing')
    made.stub.release('first')
    const aDone = await until(a, 'the first session to finish', (view) => view.status === 'completed')
    for (const [done, prompt, reply] of [[aDone, 'plan:first', 'first reply'], [bDone, 'plan:second', 'second reply']] as const) {
      assert.deepEqual(done.rows.map((row) => row.kind), ['prompt', 'reply'])
      assert.equal((done.rows[0]?.data as { text: string }).text, prompt)
      assert.equal((done.rows[1]?.data as { reply: string }).reply, reply)
    }
  })

  test('cancelling a waiting turn grants nothing, resolves the question, and a late approval is refused', async () => {
    const made = await rig({ 'plan:cancel': write('cancelled.txt', 'must not be written') })
    const session = await trusted(made, false)
    await session.send('plan:cancel')
    const question = pendingOf(await until(session, 'the write question', (view) => view.status === 'waiting'))
    await session.cancel()
    const ended = await until(session, 'cancellation', (view) => view.status === 'cancelled')
    assert.equal(ended.pending, null)
    assert.equal(ended.rows.find((row) => row.id === question.row)?.resolved, true)
    assert.equal(ended.rows.at(-1)?.kind, 'error')
    // The bridge refuses a late answer to the question that was cancelled.
    await assert.rejects(
      rawOn(made.rpc, 'confirm.reply', { session: session.id, request: question.request, decision: 'approve' }),
      (error: RpcError) => error.code === 'no_such_request',
    )
    await made.rpc.client.describe()
    assert.equal(existsSync(join(made.project, 'cancelled.txt')), false, 'the cancelled write must not land')
  })

  test('cancelling a running turn ends it as cancelled', async () => {
    const made = await rig({ 'plan:running': [{ say: 'late', hold: 'slow' }] })
    const session = await trusted(made)
    await session.send('plan:running')
    await made.stub.reached('slow')
    await session.cancel()
    await until(session, 'cancellation', (view) => view.status === 'cancelled')
    made.stub.release('slow')
    assert.deepEqual(await session.send('plan:running again').then((r) => r.turn), 2, 'the session accepts a new turn')
  })

  test('closing detaches the view and claims nothing about the worker or the saved record', async () => {
    const made = await rig({ 'plan:close': write('closed.txt', 'must not be written') })
    const session = await trusted(made, false)
    await session.send('plan:close')
    await until(session, 'the write question', (view) => view.status === 'waiting')
    const outcome = await session.close()
    assert.deepEqual(outcome, { viewDetached: true, workerTerminated: 'unknown', saved: 'unknown' })
    assert.equal(session.view.status, 'detached')
    assert.equal(session.view.pending, null)
    assert.equal(session.view.ended?.reason, 'detached')
    await assert.rejects(session.send('after close'), UnsupportedError)
    await made.rpc.client.describe()
    assert.equal(existsSync(join(made.project, 'closed.txt')), false, 'closing refused the pending write')
  })

  test('the bridge refuses unknown methods, repeated or unsupported view starts, and the client reports them with their code', async () => {
    const made = await rig({})
    const session = await trusted(made)
    await assert.rejects(rawOn(made.rpc, 'session.teleport', { session: session.id }), (e: RpcError) => e.code === 'bad_request')
    await assert.rejects(rawOn(made.rpc, 'session.view.start', { session: session.id, version: 1 }), (e: RpcError) => e.code === 'bad_request')
    await assert.rejects(rawOn(made.rpc, 'session.view.start', { session: session.id, version: 2 }), (e: RpcError) => e.code === 'bad_request')
    assert.equal(session.view.sequence, 1, 'refusals changed nothing in the view')
  })

  test('unreadable input does not stop the bridge, and an answerable error with no matching request is reported', async () => {
    const made = await rig({})
    await made.rpc.client.describe()
    made.rpc.child.stdin.write('not json at all\n')
    made.rpc.child.stdin.write('{"id": 987654}\n')
    await within(
      new Promise<void>((resolve) => {
        const check = setInterval(() => {
          if (made.diagnostics.some((message) => message.includes('987654'))) {
            clearInterval(check)
            resolve()
          }
        }, 5)
      }),
      'the bridge to answer the id-only line',
    )
    assert.ok((await rawOn(made.rpc, 'agent.info')) !== undefined, 'a later valid request still works')
  })

  test('ending input makes the bridge exit, refuses a pending write, and ends the view as lost', async () => {
    const made = await rig({ 'plan:eof': write('eof.txt', 'must not be written') })
    const session = await trusted(made, false)
    await session.send('plan:eof')
    await until(session, 'the write question', (view) => view.status === 'waiting')
    made.rpc.endInput()
    const exit = await within(made.rpc.exited, 'the bridge to exit')
    assert.equal(exit.signal, null)
    assert.equal(session.view.ended?.reason, 'connection_lost')
    assert.equal(existsSync(join(made.project, 'eof.txt')), false, 'ending input refused the pending write')
    await assert.rejects(rawOn(made.rpc, 'agent.info'), ConnectionLostError)
  })

  test(
    'a bridge that dies with a request unanswered fails that request',
    { skip: process.platform === 'win32' },
    async () => {
      const made = await rig({})
      const session = await trusted(made)
      made.rpc.child.kill('SIGSTOP')
      const inFlight = rawOn(made.rpc, 'agent.info')
      inFlight.catch(() => undefined)
      made.rpc.child.kill('SIGKILL')
      await assert.rejects(inFlight, ConnectionLostError)
      assert.equal(session.view.ended?.reason, 'connection_lost')
    },
  )
})
