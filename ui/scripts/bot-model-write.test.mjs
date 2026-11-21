// The banner and the list read have to stay in this order: a read may show a bots-file problem and does not clear one.
import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const source = buildSync({ entryPoints: ['src/renderer/bot-model-write.ts'], bundle: true, write: false, platform: 'node', format: 'cjs' }).outputFiles[0].text
const module = { exports: {} }
new Function('require', 'module', 'exports', source)(require, module, module.exports)
const { settleBotModelWrite } = module.exports

const MISSING = 'Error: Could not save the bot’s model.'

function calls() {
  const log = []
  return {
    log,
    setProblem(problem) { log.push(['setProblem', problem]) },
    readBots() { log.push(['readBots']) },
  }
}

test('a saved model clears the banner before the list is read', async () => {
  const { log, setProblem, readBots } = calls()
  await settleBotModelWrite(Promise.resolve({ slug: 'review' }), setProblem, readBots)
  assert.deepEqual(log, [['setProblem', null], ['readBots']])
})

test('a write that resolves with no bot shows the missing-save sentence and then the list is read', async () => {
  const { log, setProblem, readBots } = calls()
  await settleBotModelWrite(Promise.resolve(null), setProblem, readBots)
  assert.deepEqual(log, [['setProblem', MISSING], ['readBots']])
})

test('a model the agent refuses stays on the banner and the list is read', async () => {
  const { log, setProblem, readBots } = calls()
  const error = new Error('no such model')
  await settleBotModelWrite(Promise.reject(error), setProblem, readBots)
  assert.deepEqual(log, [['setProblem', String(error)], ['readBots']])
})
