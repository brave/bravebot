import assert from 'node:assert/strict'
import { test } from 'node:test'
import { ArgumentError, ProcessError, startSession, type QuestionEvent, type Turn } from '../src/index.js'
import { fake } from './support/fake.js'
import { within } from './support/wait.js'

async function withFake<T>(scenario: string, body: (stand: ReturnType<typeof fake>) => Promise<T>): Promise<T> {
  const stand = fake(scenario)
  try {
    return await body(stand)
  } finally {
    stand.dispose()
  }
}

const turnOf = async (next: Promise<Turn | undefined>): Promise<Turn> => {
  const turn = await within(next, 'a turn')
  assert.ok(turn, 'the session ended before the turn did')
  return turn
}

test('two prompts to one session give two results in order, and closing ends the process cleanly', async () => {
  await withFake('lines', async (stand) => {
    const session = startSession({ command: stand.command, env: stand.env })
    const first = await within(session.prompt('hello'), 'the first turn')
    const second = await within(session.prompt('again'), 'the second turn')
    assert.equal(first.result.reply, 'Reply 1 to: hello')
    assert.equal(second.result.reply, 'Reply 2 to: again')
    const exit = await within(session.close(), 'the process to end')
    assert.equal(exit.code, 0)
    assert.equal(await session.nextTurn(), undefined, 'a closed session with every turn ended has no more turns')
    assert.deepEqual(stand.argv(), ['--json-stream', '--input-format', 'lines'])
    assert.deepEqual(stand.stdin().map((line) => JSON.parse(line)), [
      { type: 'prompt', text: 'hello' },
      { type: 'prompt', text: 'again' },
    ])
  })
})

test('a prompt is sent as the line spelt it, with quotes, newlines and U+2028 inside one JSON line', async () => {
  await withFake('lines', async (stand) => {
    const text = 'say "hi"\nthen {"type":"stop"}\u2028 ✓ 🙂'
    const session = startSession({ command: stand.command, env: stand.env })
    const turn = await within(session.prompt(text), 'the turn')
    assert.equal(turn.result.reply, `Reply 1 to: ${text}`)
    await session.close()
    const lines = stand.stdin()
    assert.equal(lines.length, 1, 'the prompt was one line')
    assert.deepEqual(JSON.parse(lines[0] as string), { type: 'prompt', text })
  })
})

test('trustInputLines is passed only when it is asked for', async () => {
  await withFake('lines', async (stand) => {
    const session = startSession({ command: stand.command, env: stand.env, trustInputLines: true })
    await within(session.prompt('hello'), 'the turn')
    await session.close()
    assert.deepEqual(stand.argv(), ['--json-stream', '--input-format', 'lines', '--trust-input-lines'])
  })
})

test('a question is answered by the handler with positions and the turn then ends with the answer it was given', async () => {
  await withFake('lines', async (stand) => {
    const asked: QuestionEvent[] = []
    const session = startSession({
      command: stand.command,
      env: stand.env,
      onQuestion: (event) => {
        asked.push(event)
        return [[1], []]
      },
    })
    const turn = await within(session.prompt('ASK which cache'), 'the turn')
    assert.equal(asked.length, 1)
    assert.equal(asked[0]?.id, 1)
    assert.equal(asked[0]?.questions[0]?.choices[1]?.label, 'Query')
    assert.equal(turn.result.reply, 'answered [[1],[]]')
    assert.deepEqual(turn.events.map((event) => event.event), ['question'])
    await session.close()
    assert.deepEqual(stand.stdin().map((line) => JSON.parse(line)), [
      { type: 'prompt', text: 'ASK which cache' },
      { type: 'answer', id: 1, choices: [[1], []] },
    ])
  })
})

test('a handler that throws, or returns words in place of positions, declines every question and reports the failure', async () => {
  for (const handler of [
    () => {
      throw new Error('handler failed')
    },
    () => ['HTTP'] as never,
    () => [['1']] as never,
    () => [[1.5]] as never,
  ]) {
    await withFake('lines', async (stand) => {
      const failures: Error[] = []
      const session = startSession({ command: stand.command, env: stand.env, onQuestion: handler, onError: (error) => failures.push(error) })
      const turn = await within(session.prompt('ASK which cache'), 'the turn')
      assert.equal(turn.result.reply, 'answered [[],[]]')
      assert.equal(failures.length, 1)
      await session.close()
    })
  }
})

test('without a handler a question is not answered by the library', async () => {
  await withFake('lines', async (stand) => {
    const events: string[] = []
    let refused = 0
    let onRefused: () => void = () => undefined
    const session = startSession({
      command: stand.command,
      env: stand.env,
      onEvent: (event) => {
        events.push(event.event)
        if (event.event === 'input_refused') {
          refused += 1
          onRefused()
        }
      },
    })
    const refusals = (count: number): Promise<void> =>
      within(
        new Promise<void>((resolve) => {
          onRefused = () => refused >= count && resolve()
          onRefused()
        }),
        `${count} input_refused events`,
      )
    session.send('ASK which cache')
    // The stand-in writes the question before it reads the next line. Each probe is refused only
    // after every line sent before it was read, so once the second is refused the stand-in has read
    // anything the library would have sent in answer to the question.
    session.answer(98, [[0]])
    await refusals(1)
    session.answer(97, [[0]])
    await refusals(2)
    assert.ok(events.includes('question'), events.join(','))
    assert.deepEqual(stand.stdin().map((line) => JSON.parse(line)), [
      { type: 'prompt', text: 'ASK which cache' },
      { type: 'answer', id: 98, choices: [[0]] },
      { type: 'answer', id: 97, choices: [[0]] },
    ])
    await session.kill()
  })
})

test('an answer, a stop and a steer are sent as the lines CLI-36 names, and an input_refused event is surfaced as an event', async () => {
  await withFake('lines', async (stand) => {
    const session = startSession({ command: stand.command, env: stand.env })
    session.send('ASK first')
    session.answer(1, [[0], [0, 1]])
    const turn = await turnOf(session.nextTurn())
    assert.equal(turn.result.reply, 'answered [[0],[0,1]]')
    // The stand-in refuses an answer to a question that is not waiting, as the command line does.
    session.answer(99, [[0]])
    session.steer('think harder')
    session.stop()
    const stopped = await turnOf(session.nextTurn())
    assert.equal(stopped.result.reason, 'stopped')
    const kinds = stopped.events.map((event) => event.event)
    assert.ok(kinds.includes('input_refused'), kinds.join(','))
    const refusal = stopped.events.find((event) => event.event === 'input_refused') as { reason: string }
    assert.equal(refusal.reason, 'no_such_question')
    await session.close()
    assert.deepEqual(stand.stdin().map((line) => JSON.parse(line)), [
      { type: 'prompt', text: 'ASK first' },
      { type: 'answer', id: 1, choices: [[0], [0, 1]] },
      { type: 'answer', id: 99, choices: [[0]] },
      { type: 'steer', text: 'think harder' },
      { type: 'stop' },
    ])
  })
})

test('a closed session refuses a send, and an empty prompt is refused before anything is written', async () => {
  await withFake('lines', async (stand) => {
    const session = startSession({ command: stand.command, env: stand.env })
    assert.throws(() => session.send(''), ArgumentError)
    assert.throws(() => session.steer(''), ArgumentError)
    await session.close()
    assert.throws(() => session.send('late'), ProcessError)
    assert.deepEqual(stand.stdin(), [])
  })
})

test('a process that ends while the session is open rejects the next turn', async () => {
  await withFake('no-result', async (stand) => {
    const session = startSession({ command: stand.command, env: stand.env })
    await assert.rejects(within(session.nextTurn(), 'the rejection'), ProcessError)
  })
})

test('stdout that closes in the middle of a turn rejects and does not give a half result', async () => {
  await withFake('half-line', async (stand) => {
    const session = startSession({ command: stand.command, env: stand.env })
    session.send('x')
    await assert.rejects(within(session.nextTurn(), 'the rejection'), (error: Error) => error.name === 'ProtocolError')
  })
})

test('killing a session stops a process that is mid-turn', async () => {
  await withFake('hang', async (stand) => {
    const session = startSession({ command: stand.command, env: stand.env })
    const exit = await within(session.kill(), 'the process to end')
    assert.ok(exit.signal !== null || exit.code !== 0)
  })
})
