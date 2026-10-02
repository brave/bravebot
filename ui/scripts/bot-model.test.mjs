import test from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { buildSync } from 'esbuild'
import { mkdtempSync, readFileSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { tmpdir } from 'node:os'

const require = createRequire(import.meta.url)
const source = buildSync({ entryPoints: ['src/shared/bots.ts'], bundle: true, write: false, platform: 'node', format: 'cjs' }).outputFiles[0].text
const module = { exports: {} }
new Function('require', 'module', 'exports', source)(require, module, module.exports)
const { parseBots, withBot, isBotModel } = module.exports
const definition = { slug: 'web-dev', name: 'Web dev', purpose: 'Build websites', home: '/tmp/bot-homes/web-dev', avatar: 'v2:preview', session: null }

test('model selection survives storage, updates, and unrelated bot edits', () => {
  const original = parseBots({ bots: [{ ...definition, model: 'provider/model-a' }] }).bots[0]
  const changed = withBot([original], { ...original, model: 'provider/model-b' })
  const saved = parseBots(JSON.parse(JSON.stringify({ bots: changed }))).bots[0]
  assert.equal(saved.model, 'provider/model-b')
  assert.equal(saved.avatar, definition.avatar)
  const renamed = parseBots({ bots: [{ ...saved, name: 'Frontend developer' }] }).bots[0]
  assert.equal(renamed.model, 'provider/model-b')
})

test('older and malformed model fields preserve the bot using the configured default', () => {
  for (const model of [undefined, null, '', '  ', 42, {}, 'bad\0model', 'x'.repeat(513)]) {
    const result = parseBots({ bots: [{ ...definition, model }] }).bots
    assert.equal(result.length, 1)
    assert.equal(result[0].model, null)
  }
})

test('IPC model validation accepts opaque IDs and rejects malformed inputs', () => {
  for (const value of [null, 'openrouter/provider/model:free', 'custom-model']) assert.equal(isBotModel(value), true)
  for (const value of [undefined, '', ' ', 1, {}, 'a\0b', 'x'.repeat(513)]) assert.equal(isBotModel(value), false)
})

test('main-process storage retains model changes across a fresh module load', () => {
  const profile = mkdtempSync(join(tmpdir(), 'bravebot-model-storage-'))
  const source = buildSync({ entryPoints: ['src/main/bots.ts'], bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'] }).outputFiles[0].text
  const load = () => {
    const module = { exports: {} }
    const mockedRequire = (id) => id === 'electron' ? { app: { getPath: () => profile } } : require(id)
    new Function('require', 'module', 'exports', source)(mockedRequire, module, module.exports)
    return module.exports
  }
  try {
    const storage = load()
    storage.saveBot(parseBots({ bots: [{ ...definition, model: 'provider/model-a' }] }).bots[0])
    storage.saveBot({ ...storage.bot(definition.slug), model: 'provider/model-b' })
    assert.equal(JSON.parse(readFileSync(join(profile, 'bravebot-ui.json'), 'utf8')).bots[0].model, 'provider/model-b')
    assert.equal(load().bot(definition.slug).model, 'provider/model-b')
  } finally { rmSync(profile, { recursive: true, force: true }) }
})

// Rejects a migration that drops the folder an old bot was pinned to: its conversations ran there,
// so pairing them with the new home would send the next turn to a folder with none of its memory.
test('a bot written before home folders keeps its conversations in the folder it was pinned to', () => {
  const first = 'a1b2c3d4-0000-4000-8000-000000000001'
  const latest = 'a1b2c3d4-0000-4000-8000-000000000002'
  const legacy = { slug: 'web-dev', name: 'Web dev', purpose: 'Build websites', directory: '/work/site', avatar: 'v2:x', session: latest, conversations: [first] }
  const [bot] = parseBots({ bots: [legacy] }, (slug) => `/app/bot-homes/${slug}`).bots
  assert.equal(bot.home, '/app/bot-homes/web-dev')
  assert.deepEqual(bot.conversations, [
    { id: first, directory: '/work/site' },
    { id: latest, directory: '/work/site' },
  ])
  // Without a way to place it there is nowhere to run it, which is not a bot that can be shown.
  assert.equal(parseBots({ bots: [legacy] }).bots.length, 0)
  // Written back and read again, the pairs survive as pairs.
  const again = parseBots(JSON.parse(JSON.stringify({ bots: [bot] }))).bots[0]
  assert.deepEqual(again.conversations, bot.conversations)
})
