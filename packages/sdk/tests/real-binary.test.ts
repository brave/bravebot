import assert from 'node:assert/strict'
import { existsSync, readdirSync } from 'node:fs'
import { join } from 'node:path'
import { after, describe, test } from 'node:test'
import { run, startSession, type QuestionEvent } from '../src/index.js'
import { startRig, type Rig } from './support/rig.js'
import { within } from './support/wait.js'

// These tests run the real `bravebot` binary against a stand-in model service. Nothing is trusted:
// each run starts in an empty directory with a home of its own and no --trust-workspace.

const rigs: Rig[] = []
async function rig(plans: Parameters<typeof startRig>[0], stderr: string[] = []): Promise<Rig> {
  const made = await startRig(plans, stderr)
  rigs.push(made)
  return made
}
after(async () => {
  for (const made of rigs) await made.stop()
})

const asking = [
  { tool: { name: 'ask_user', arguments: { questions: [{ header: 'Cache', question: 'Which layer?', options: [{ label: 'HTTP' }, { label: 'Query' }] }] } } },
  { say: 'asked' },
]

describe('the real bravebot command line through the library', () => {
  test('a one-shot run ends on a result object that carries the CLI-12 fields and the events of the run', async () => {
    const made = await rig({ 'plan:plain': [{ say: 'all done' }] })
    const turn = await within(run('plan:plain please', made.options), 'the run')
    assert.equal(turn.result.ok, true)
    assert.equal(turn.result.status, 0)
    assert.equal(turn.result.reason, 'done')
    assert.equal(turn.result.reply, 'all done')
    assert.equal(turn.result.schema, 1)
    assert.deepEqual(Object.keys(turn.result.tokens).sort(), ['cache_read', 'cache_written', 'context', 'output', 'total'])
    assert.ok(turn.events.some((event) => event.event === 'usage'), 'a usage event was written')
    assert.equal(made.stub.unmatched.length, 0)
  })

  test('a write that nobody can approve ends on the refusal status, names its principle as data, and writes nothing', async () => {
    const made = await rig({ 'plan:write': [{ tool: { name: 'write_file', arguments: { path: 'out.txt', contents: 'must not land' } } }, { say: 'finished' }] })
    const turn = await within(run('plan:write', made.options), 'the run')
    assert.equal(turn.result.ok, false)
    assert.equal(turn.result.status, 4)
    assert.ok(turn.result.refusals.length > 0, 'a refusal was listed')
    for (const refusal of turn.result.refusals) {
      assert.equal(typeof refusal.principle, 'string')
      assert.equal(typeof refusal.gate, 'string')
    }
    const streamed = turn.events.filter((event) => event.event === 'refusal') as { principle: string }[]
    assert.deepEqual(streamed.map((event) => event.principle), turn.result.refusals.map((refusal) => refusal.principle))
    assert.equal(existsSync(join(made.project, 'out.txt')), false, 'the library approved nothing')
  })

  test('a session carries two turns over one conversation, and the words of an unvouched line never reach the planner', async () => {
    const made = await rig({ 'plan:one': [{ say: 'first answer' }], 'plan:two': [{ say: 'second answer' }] })
    const session = startSession(made.options)
    const first = await within(session.prompt('plan:one'), 'the first turn')
    const second = await within(session.prompt('plan:two'), 'the second turn')
    await session.close()
    // Unvouched lines are quarantined, so the planner is given a reference and never the words (CLI-36).
    // The stand-in model finds no plan key, so it answers `done`, which shows the words were not sent.
    assert.equal(first.result.reply, 'done')
    assert.equal(second.result.reply, 'done')
    assert.equal(first.result.ok && second.result.ok, true)
    assert.ok(first.result.session !== null && first.result.session === second.result.session, 'both turns are one session record')
    assert.equal(made.stub.requests.some((body) => body.includes('plan:one') || body.includes('plan:two')), false, 'the words of an unvouched line did not reach the planner')
    assert.equal(made.stub.requests.length >= 2, true)
  })

  test('with trustInputLines the words reach the planner', async () => {
    const made = await rig({ 'plan:trusted': [{ say: 'trusted answer' }] })
    const trusted = startSession({ ...made.options, trustInputLines: true })
    const turn = await within(trusted.prompt('plan:trusted'), 'the turn')
    await trusted.close()
    assert.equal(turn.result.reply, 'trusted answer')
    assert.equal(made.stub.requests.some((body) => body.includes('plan:trusted')), true)
    assert.equal(made.stub.unmatched.length, 0)
  })

  test('a planner question is put as a question event and answered by position through the handler', async () => {
    const made = await rig({ 'plan:ask': asking })
    const asked: QuestionEvent[] = []
    const session = startSession({
      ...made.options,
      trustInputLines: true,
      onQuestion: (event) => {
        asked.push(event)
        return [[1]]
      },
    })
    const turn = await within(session.prompt('plan:ask'), 'the turn')
    await session.close()
    assert.equal(asked.length, 1)
    assert.equal(asked[0]?.questions[0]?.header, 'Cache')
    assert.deepEqual(asked[0]?.questions[0]?.choices.map((choice) => choice.label), ['HTTP', 'Query'])
    assert.equal(turn.result.reply, 'asked')
    assert.ok(turn.events.some((event) => event.event === 'call' && (event as { tool: string }).tool === 'ask_user'))
    const answered = made.stub.requests.filter((body) => body.includes('Query'))
    assert.ok(answered.length > 0, 'the planner was told the position picked, which names Query')
  })

  test('a line the command line does not take is surfaced as an input_refused event and the session goes on', async () => {
    const made = await rig({ 'plan:after': [{ say: 'after' }] })
    const session = startSession({ ...made.options, trustInputLines: true })
    session.answer(12345, [[0]])
    const turn = await within(session.prompt('plan:after'), 'the turn')
    await session.close()
    const refused = turn.events.find((event) => event.event === 'input_refused') as { reason: string; line: number } | undefined
    assert.ok(refused, 'an input_refused event was written')
    assert.equal(refused.reason, 'no_such_question')
    assert.equal(refused.line, 1)
    assert.equal(turn.result.reply, 'after')
  })

  test('a steer sent while a turn runs reaches that turn, which ends with the reply to the steer', async () => {
    const made = await rig({
      'plan:steer': [{ tool: { name: 'list_files', arguments: { directory: '.' } }, hold: 'running' }, { say: 'before the steer' }],
      'plan:after-steer': [{ say: 'after the steer' }],
    })
    const session = startSession({ ...made.options, trustInputLines: true })
    session.send('plan:steer')
    await made.stub.reached('running')
    session.steer('plan:after-steer')
    made.stub.release('running')
    const turn = await within(session.nextTurn(), 'the steered turn')
    await session.close()
    assert.ok(turn)
    assert.equal(turn.result.reply, 'after the steer', 'the running turn took the steer')
    assert.equal(turn.result.ok, true)
    assert.equal(made.stub.requests.some((body) => body.includes('plan:after-steer')), true, 'the planner was sent the steer')
  })

  test('an incognito session writes no record under the home and its results name no session', async () => {
    const ordinary = await rig({ 'plan:keep': [{ say: 'kept' }] })
    const kept = startSession({ ...ordinary.options, trustInputLines: true })
    const keptTurn = await within(kept.prompt('plan:keep'), 'the ordinary turn')
    await kept.close()
    assert.equal(keptTurn.result.reply, 'kept')
    assert.ok(keptTurn.result.session !== null, 'an ordinary session names its record')
    const files = (home: string): string[] => readdirSync(home, { recursive: true, withFileTypes: true }).filter((entry) => entry.isFile()).map((entry) => entry.name)
    assert.ok(files(ordinary.home).length > 0, 'an ordinary session writes a record under the home')

    const quiet = await rig({ 'plan:quiet-one': [{ say: 'one' }], 'plan:quiet-two': [{ say: 'two' }] })
    const session = startSession({ ...quiet.options, trustInputLines: true, incognito: true })
    const first = await within(session.prompt('plan:quiet-one'), 'the first incognito turn')
    const second = await within(session.prompt('plan:quiet-two'), 'the second incognito turn')
    await session.close()
    assert.equal(first.result.reply, 'one')
    assert.equal(second.result.reply, 'two')
    assert.equal(first.result.session, null)
    assert.equal(second.result.session, null)
    assert.deepEqual(files(quiet.home), [], 'no file was written under the home')
  })

  test('a stop ends the turn that is running', async () => {
    const made = await rig({ 'plan:slow': [{ say: 'late', hold: 'slow' }] })
    const session = startSession({ ...made.options, trustInputLines: true })
    session.send('plan:slow')
    await made.stub.reached('slow')
    session.stop()
    const stopped = await within(session.nextTurn(), 'the stopped turn')
    made.stub.release('slow')
    assert.ok(stopped)
    assert.equal(stopped.result.reply === 'late', false, 'the stopped turn did not finish')
    await session.close()
  })

  test('a run with nothing to refuse still replies', async () => {
    const stderr: string[] = []
    const made = await rig({ 'plan:quiet': [{ say: 'quiet' }] }, stderr)
    const turn = await within(run('plan:quiet', made.options), 'the run')
    assert.equal(turn.result.reply, 'quiet')
  })

  test('what the binary writes to stderr reaches the sink and the run still ends on its result', async () => {
    const stderr: string[] = []
    const made = await rig({ 'plan:write': [{ tool: { name: 'write_file', arguments: { path: 'out.txt', contents: 'must not land' } } }, { say: 'finished' }] }, stderr)
    const turn = await within(run('plan:write', made.options), 'the run')
    assert.equal(turn.result.status, 4)
    assert.ok(stderr.join('').length > 0, 'the refusal was written to stderr and reached the sink')
    assert.ok(turn.events.length > 0, 'the stream became events')
  })

  test('a read-only tool call is reported as a call event with the tool name', async () => {
    const made = await rig({ 'plan:look': [{ tool: { name: 'list_files', arguments: { directory: '.' } } }, { say: 'looked' }] })
    const turn = await within(run('plan:look', made.options), 'the run')
    const call = turn.events.find((event) => event.event === 'call') as { tool: string } | undefined
    assert.equal(call?.tool, 'list_files')
  })
})
