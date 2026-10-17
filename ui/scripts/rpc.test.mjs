import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'

function load() {
  const source = buildSync({ entryPoints: ['src/renderer/rpc.ts'], bundle: true, write: false, platform: 'node', format: 'cjs' }).outputFiles[0].text
  const module = { exports: {} }
  new Function('module', 'exports', source)(module, module.exports)
  return module.exports
}

const answering = (answer) => {
  globalThis.window = { bravebot: { request: async () => answer, sendBotTurn: async () => answer } }
  return load()
}

const bot = { session: 's', slug: 'b', prompt: 'p', grounded: false, model: null }

test('an answer from the agent is returned, over either channel', async () => {
  const rpc = answering({ ok: { sessions: [] } })
  assert.deepEqual(await rpc.call('session.list'), { sessions: [] })
  await rpc.callBot(bot)
})

test('a failed answer is thrown as the failure it is, over either channel', async () => {
  const rpc = answering({ error: { code: 'bad', message: 'no' } })
  for (const attempt of [() => rpc.call('x'), () => rpc.callBot(bot)]) {
    await assert.rejects(attempt, (error) => error.constructor.name === 'RequestFailed' && !(error instanceof rpc.Unconfigurable))
  }
})

test('a missing configuration is Unconfigurable, over either channel', async () => {
  const rpc = answering({ error: { code: 'config', message: 'no credentials' } })
  for (const attempt of [() => rpc.call('x'), () => rpc.callBot(bot)]) {
    await assert.rejects(attempt, (error) => error instanceof rpc.Unconfigurable && error.message === 'no credentials')
  }
})
