import assert from 'node:assert/strict'
import { test } from 'node:test'
import { ProcessError, ProtocolError, run, stream, type BravebotEvent, type Message } from '../src/index.js'
import { fake } from './support/fake.js'
import { within } from './support/wait.js'

// A literal built from the characters that would break a careless reader: quotes and braces that
// close a JSON object, a newline, U+2028, a backslash, an escape sequence, and multi-byte text.
const HOSTILE = '"},"ok":false,"status":9,"x":"\n{"event":"call"}\u2028 \\ \u001b[2K ✓ 🙂'

async function withFake<T>(scenario: string, body: (stand: ReturnType<typeof fake>) => Promise<T>): Promise<T> {
  const stand = fake(scenario)
  try {
    return await body(stand)
  } finally {
    stand.dispose()
  }
}

test('a one-shot run resolves with the events in order and the result object, and unknown fields and event kinds pass through', async () => {
  await withFake('echo', async (stand) => {
    const seen: BravebotEvent[] = []
    const turn = await run('hello there', { command: stand.command, env: stand.env, onEvent: (event) => seen.push(event) })
    assert.deepEqual(turn.events.map((event) => event.event), ['call', 'future_kind', 'usage'])
    assert.deepEqual(seen, turn.events, 'onEvent saw the same events, in the same order')
    assert.deepEqual(turn.events[1], { schema: 1, event: 'future_kind', anything: [1, 2] })
    assert.equal(turn.result.reply, 'echo: hello there')
    assert.equal(turn.result.ok, true)
    assert.equal(turn.result.status, 0)
    assert.equal((turn.result as unknown as { future_field: string }).future_field, 'kept')
    assert.deepEqual(stand.argv(), ['--json-stream', '-p', 'hello there'])
  })
})

test('a run that ends badly resolves with its status, reason and refusal principle as data', async () => {
  await withFake('refused', async (stand) => {
    const turn = await run('write something', { command: stand.command, env: stand.env })
    assert.equal(turn.result.ok, false)
    assert.equal(turn.result.status, 4)
    assert.equal(turn.result.reason, 'refusal')
    assert.equal(turn.result.identifier, 'BB1004')
    assert.deepEqual(turn.result.refusals.map((one) => one.principle), ['integrity-gate'])
    assert.deepEqual(turn.events, [{ schema: 1, event: 'refusal', gate: 'action', principle: 'integrity-gate', reason: 'injection blocked' }])
  })
})

test('a reply, a message and a call target full of JSON, newlines and escapes come through whole and decide nothing', async () => {
  await withFake('hostile', async (stand) => {
    const turn = await run('hello', { command: stand.command, env: stand.env })
    assert.equal(turn.result.reply, HOSTILE)
    assert.equal(turn.result.message, HOSTILE)
    assert.equal(turn.result.ok, true, 'the text did not close the object and set ok')
    assert.equal(turn.result.status, 0)
    assert.equal(turn.events.length, 1)
    const call = turn.events[0] as { event: string; target: string }
    assert.equal(call.event, 'call')
    assert.equal(call.target, HOSTILE, 'the target is carried exactly as it was written')
  })
})

test('a result object split into three-byte chunks, with multi-byte characters cut in two, is read whole', async () => {
  await withFake('chunks', async (stand) => {
    const turn = await run('hello', { command: stand.command, env: stand.env })
    assert.equal(turn.result.reply, 'héllo ✓ 🙂')
  })
})

test('stream gives each event as it is written and then the result, and stops the process when the loop is left early', async () => {
  await withFake('echo', async (stand) => {
    const kinds: string[] = []
    const lines: Message[] = []
    for await (const message of stream('go', { command: stand.command, env: stand.env })) {
      lines.push(message)
      kinds.push(message.type === 'event' ? message.event.event : 'result')
    }
    assert.deepEqual(kinds, ['call', 'future_kind', 'usage', 'result'])
  })
  await withFake('hang', async (stand) => {
    for await (const message of stream('go', { command: stand.command, env: stand.env })) {
      assert.equal(message.type, 'event')
      break
    }
  })
})

test('a run whose process ends without a result object rejects, and says how it ended', async () => {
  await withFake('no-result', async (stand) => {
    await assert.rejects(run('x', { command: stand.command, env: stand.env }), (error: ProcessError) => {
      assert.ok(error instanceof ProcessError)
      assert.equal(error.code, 0)
      return true
    })
  })
})

test('stdout that closes in the middle of a line rejects instead of giving a half result', async () => {
  await withFake('half-line', async (stand) => {
    await assert.rejects(run('x', { command: stand.command, env: stand.env }), ProtocolError)
  })
})

test('a line that is not JSON rejects without repeating the line, and the process is stopped', async () => {
  await withFake('garbage', async (stand) => {
    await assert.rejects(within(run('x', { command: stand.command, env: stand.env }), 'the rejection'), (error: Error) => {
      assert.ok(error instanceof ProtocolError)
      assert.equal(error.message.includes('SECRET-WORDS'), false, 'the line is not echoed into the error')
      return true
    })
  })
  await withFake('not-an-object', async (stand) => {
    await assert.rejects(within(run('x', { command: stand.command, env: stand.env }), 'the rejection'), ProtocolError)
  })
})

test('a command that does not exist rejects with a ProcessError', async () => {
  await assert.rejects(run('x', { command: '/nonexistent/bravebot-for-test' }), ProcessError)
})

test('aborting the signal stops the process and rejects', async () => {
  await withFake('hang', async (stand) => {
    const controller = new AbortController()
    const pending = run('x', { command: stand.command, env: stand.env, signal: controller.signal, onEvent: () => controller.abort() })
    await assert.rejects(within(pending, 'the rejection'), ProcessError)
  })
})

test('stderr goes to the sink as text and is never read, and without a sink it is not an error', async () => {
  await withFake('stderr', async (stand) => {
    const chunks: string[] = []
    const turn = await run('x', { command: stand.command, env: stand.env, onStderr: (text) => chunks.push(text) })
    assert.equal(chunks.join(''), 'progress \u2713 line\n')
    assert.equal(turn.result.reply, 'quiet')
  })
})
