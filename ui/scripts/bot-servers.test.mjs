import test from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { buildSync } from 'esbuild'
import { mkdtempSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { tmpdir } from 'node:os'

const require = createRequire(import.meta.url)
const load = (entry, profile) => {
  const source = buildSync({ entryPoints: [entry], bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'] }).outputFiles[0].text
  const module = { exports: {} }
  const mockedRequire = (id) => id === 'electron' ? { app: { getPath: () => profile } } : require(id)
  new Function('require', 'module', 'exports', source)(mockedRequire, module, module.exports)
  return module.exports
}
const { isServerSelection, parseBotServers, parseBots } = load('src/shared/bots.ts')
const definition = { slug: 'web-dev', name: 'Web dev', purpose: 'Build websites', home: '/tmp/bot-homes/web-dev', avatar: 'v2:preview', session: null }

const FLAGS = { editable: true, every: true, everyPossible: true }
const ANSWER = { servers: null, tools: false, inline: false, kind: 'worker', ...FLAGS }

test('a selection is every server or a list of names, and the agent judges the names', () => {
  for (const value of [null, [], ['github'], ['github', 'linear'], ['bad name'], ['github', 'github']]) assert.equal(isServerSelection(value), true)
  for (const value of [undefined, 'github', {}, [42], [null]]) assert.equal(isServerSelection(value), false)
})

test('only a whole answer from the agent is taken as a bot’s servers, with its own reading of the line', () => {
  assert.deepEqual(parseBotServers(ANSWER), ANSWER)
  const narrowed = { ...ANSWER, servers: ['github'], tools: true, every: false, everyPossible: false }
  assert.deepEqual(parseBotServers({ ...narrowed, extra: 1 }), narrowed)
  for (const value of [null, 'x', {}, { ...ANSWER, servers: 'github' }, { ...ANSWER, servers: [1] }, { ...ANSWER, tools: 'no' },
    { ...ANSWER, inline: undefined }, { ...ANSWER, kind: undefined }, { ...ANSWER, editable: undefined },
    { ...ANSWER, every: 1 }, { ...ANSWER, everyPossible: null }]) {
    assert.equal(parseBotServers(value), null)
  }
})

test('servers are read and written under the stored definition name, never another', async () => {
  const profile = mkdtempSync(join(tmpdir(), 'bravebot-servers-'))
  const storage = load('src/main/bots.ts', profile)
  try {
    const stored = { ...parseBots({ bots: [definition] }).bots[0], definition: 'web-dev-2' }
    storage.saveBot(stored)
    const calls = []
    let line = null
    const request = async (method, params) => {
      calls.push([method, params])
      if (method === 'bot.servers.write') { line = params.servers; return { name: params.name } }
      return { ...ANSWER, servers: line, every: line === null }
    }
    const saved = await storage.saveBotServers({ ...stored, definition: 'someone-else' }, ['github'], request)
    assert.deepEqual(saved.servers, ['github'])
    assert.deepEqual(calls, [
      ['bot.servers.write', { name: 'web-dev-2', servers: ['github'] }],
      ['bot.servers.read', { name: 'web-dev-2' }],
    ])
    calls.length = 0
    assert.equal((await storage.saveBotServers(stored, null, request)).servers, null)
    assert.deepEqual(calls[0], ['bot.servers.write', { name: 'web-dev-2', servers: null }])
  } finally { rmSync(profile, { recursive: true, force: true }) }
})

test('a refused or unanswered write is an error, and the next one still runs', async () => {
  const profile = mkdtempSync(join(tmpdir(), 'bravebot-servers-'))
  const storage = load('src/main/bots.ts', profile)
  try {
    const stored = { ...parseBots({ bots: [definition] }).bots[0], definition: 'web-dev' }
    storage.saveBot(stored)
    await assert.rejects(storage.saveBotServers(stored, [], null), /not running/)
    await assert.rejects(storage.saveBotServers(stored, [], async () => { throw new Error('inline mcpServers') }), /inline mcpServers/)
    await assert.rejects(storage.saveBotServers(stored, [], async () => ({ name: 'other' })), /did not rewrite/)
    await assert.rejects(storage.readBotServers(stored, async () => ({ servers: 'all' })), /did not say/)
    await assert.rejects(storage.readBotServers({ ...stored, definition: null }, async () => ({})), /no definition/)
    const ok = await storage.saveBotServers(stored, [], async (method, params) =>
      method === 'bot.servers.write' ? { name: params.name } : { ...ANSWER, servers: [], every: false })
    assert.deepEqual(ok.servers, [])
  } finally { rmSync(profile, { recursive: true, force: true }) }
})
