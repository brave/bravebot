import assert from 'node:assert/strict'
import { once } from 'node:events'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { test } from 'node:test'
import { CapabilityError, ConnectionLostError, UnsupportedError } from '../src/common/index.js'
import { connectStdio } from '../src/node/index.js'
import { FIXTURES } from './support/scenario.js'
import { rawOn } from './support/raw.js'
import { within } from './support/wait.js'

test('a runtime without the session view is refused before any session is created', async () => {
  const legacy = connectStdio({ command: process.execPath, args: [join(FIXTURES, 'legacy-runtime.mjs')], env: {}, workspaces: [{ id: 'work', name: 'Work', directory: '/work' }] })
  try {
    await assert.rejects(legacy.client.createSession({ workspace: 'work' }), CapabilityError)
    await assert.rejects(legacy.client.describe(), CapabilityError)
    await rawOn(legacy, 'agent.info')
    const requests = legacy.stderr().split('\n').filter(Boolean).map((line) => (JSON.parse(line.slice('request '.length)) as { method: string }).method)
    assert.deepEqual(requests, ['agent.info', 'agent.info'], 'no session.new was sent to the old runtime')
  } finally {
    await legacy.dispose(500)
  }
})

test('a command that cannot start fails requests as a lost connection', async () => {
  const missing = connectStdio({ command: join(FIXTURES, 'does-not-exist'), env: {} })
  const exit = await within(missing.exited, 'the failed start to be reported')
  assert.equal(exit.code, null)
  await assert.rejects(missing.client.describe(), ConnectionLostError)
})

test('a child exit reports its code and ends the connection', async () => {
  const quick = connectStdio({ command: process.execPath, args: ['-e', 'process.exit(3)'], env: {} })
  const exit = await within(quick.exited, 'the child to exit')
  assert.equal(exit.code, 3)
  // Stdout can reach EOF before the exit code is known; the exit promise still reports it.
  await assert.rejects(rawOn(quick, 'agent.info'), ConnectionLostError)
})

test('output split mid-character across reads is decoded whole', async () => {
  const script = `
    const line = JSON.stringify({ id: 1, ok: { text: 'héllo ✓ 🙂' } }) + '\\n'
    const bytes = Buffer.from(line)
    process.stdin.once('data', () => {
      const cut = bytes.indexOf(0xe2) + 1
      process.stdout.write(bytes.subarray(0, cut))
      setTimeout(() => process.stdout.write(bytes.subarray(cut)), 20)
    })`
  const split = connectStdio({ command: process.execPath, args: ['-e', script], env: {} })
  try {
    assert.deepEqual(await within(rawOn(split, 'anything'), 'the split reply'), { text: 'héllo ✓ 🙂' })
  } finally {
    await split.dispose(500)
  }
})

test('a request that is never answered ends the connection with its outcome unknown and stops the child', async () => {
  const silent = connectStdio({ command: process.execPath, args: ['-e', 'setInterval(() => {}, 1000)'], env: {}, requestTimeoutMs: 100 })
  await assert.rejects(rawOn(silent, 'turn.send'), (error: ConnectionLostError) => /outcome is unknown/.test(error.message))
  const exit = await within(silent.exited, 'the silent child to be stopped')
  assert.ok(exit.signal !== null || exit.code !== null)
  await assert.rejects(rawOn(silent, 'agent.info'), ConnectionLostError)
})

for (const ending of ['end', 'error'] as const) {
  /** Losing the reply stream must end a view even while the child stays alive with no pending request. */
  test(`stdout ${ending} ends idle session views before the child exits`, async () => {
    const contract = JSON.parse(readFileSync(join(FIXTURES, 'wire-contract.json'), 'utf8')) as { capability: unknown }
    const script = `
      const capability = ${JSON.stringify(contract.capability)}
      const lines = require('node:readline').createInterface({ input: process.stdin })
      const send = message => process.stdout.write(JSON.stringify(message) + '\\n')
      setInterval(() => {}, 1000)
      lines.on('line', line => {
        const { id, method } = JSON.parse(line)
        if (method === 'end-output') { process.stdout.end(); return }
        let ok = {}
        if (method === 'agent.info') ok = { capabilities: { sessionView: capability } }
        if (method === 'session.new') ok = { session: 's1' }
        if (method === 'session.view.start') send({ event: 'session.view.initial', session: 's1',
          data: { sequence: 0, turn: 0, status: 'idle', pending: null, rows: [] } })
        send({ id, ok })
      })`
    const connection = connectStdio({ command: process.execPath, args: ['-e', script], env: {},
      workspaces: [{ id: 'work', name: 'Work', directory: '/work' }] })
    try {
      const session = await within(connection.client.createSession({ workspace: 'work' }), 'the initial view')
      const stopped = once(connection.child.stdout, ending)
      if (ending === 'end') connection.child.stdin.write(JSON.stringify({ method: 'end-output' }) + '\n')
      else connection.child.stdout.destroy(new Error('the reply stream failed'))
      await within(stopped, `stdout ${ending}`)
      assert.equal(connection.child.exitCode, null, 'the child is still alive')
      assert.equal(session.view.ended?.reason, 'connection_lost')
      await assert.rejects(session.send('after output loss'), UnsupportedError)
      await assert.rejects(rawOn(connection, 'agent.info'), ConnectionLostError)
      await within(connection.exited, 'the child to be stopped after output loss')
    } finally {
      await connection.dispose(500)
    }
  })
}

test('a write the child never reads fails the request at once, not at its deadline', async () => {
  const deaf = connectStdio({
    command: process.execPath,
    args: ['-e', 'require("fs").closeSync(0); console.error("input closed"); setInterval(() => {}, 1000)'],
    env: {},
    requestTimeoutMs: 60_000,
  })
  const closed = new Promise<void>((resolve) => {
    const watch = (): void => { if (deaf.stderr().includes('input closed')) resolve(); else deaf.child.stderr.once('data', watch) }
    watch()
  })
  try {
    // Write only after the child reports that its end of the pipe is closed, so the write breaks.
    await within(closed, 'the child to close its input')
    await within(
      assert.rejects(rawOn(deaf, 'turn.send'), (error: ConnectionLostError) => /writing to bravebot-rpc failed/.test(error.message)),
      'the failed write to end the request',
    )
  } finally {
    await deaf.dispose(500)
  }
})

test('a request after the child input has ended also ends the connection while the child lives', async () => {
  const idle = connectStdio({ command: process.execPath, args: ['-e', 'setTimeout(() => {}, 10000)'], env: {} })
  try {
    idle.endInput()
    await within(once(idle.child.stdin, 'close'), 'the input to close')
    await assert.rejects(rawOn(idle, 'agent.info'), (error: Error) => /not accepting input/.test(error.message))
    await assert.rejects(rawOn(idle, 'agent.info'), ConnectionLostError)
  } finally {
    await idle.dispose(200)
  }
})
