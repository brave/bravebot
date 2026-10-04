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
const definition = { slug: 'web-dev', name: 'Web dev', purpose: 'Build websites', directory: '/tmp/web-dev', avatar: 'v2:preview', session: null }

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

test('a stored definition name is kept only where it is a slug', () => {
  for (const value of ['web-dev', 'web-dev-2']) {
    assert.equal(parseBots({ bots: [{ ...definition, definition: value }] }).bots[0].definition, value)
  }
  for (const value of ['../x', 'a/b', '', 42, {}, undefined, null]) {
    assert.equal(parseBots({ bots: [{ ...definition, definition: value }] }).bots[0].definition, null)
  }
})

test('making a bot saves nothing unless the agent names the definition it wrote', async () => {
  const profile = mkdtempSync(join(tmpdir(), 'bravebot-define-'))
  const source = buildSync({ entryPoints: ['src/main/bots.ts'], bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'] }).outputFiles[0].text
  const module = { exports: {} }
  const mockedRequire = (id) => id === 'electron' ? { app: { getPath: () => profile } } : require(id)
  new Function('require', 'module', 'exports', source)(mockedRequire, module, module.exports)
  const storage = module.exports
  const fresh = { ...parseBots({ bots: [{ ...definition, model: null }] }).bots[0], definition: null }
  try {
    await assert.rejects(storage.saveFormBot(fresh, null), /not running/)
    await assert.rejects(storage.saveFormBot(fresh, async () => { throw new Error('refused') }), /refused/)
    for (const answer of [null, {}, { name: '../x' }, { name: 42 }]) {
      await assert.rejects(storage.saveFormBot(fresh, async () => answer), /did not name/)
    }
    assert.equal(storage.bot(fresh.slug), null)

    const calls = []
    const saved = await storage.saveFormBot({ ...fresh, model: 'provider/model-a' }, async (method, params) => { calls.push([method, params]); return { name: 'web-dev-2' } })
    assert.equal(saved.definition, 'web-dev-2')
    assert.equal(storage.bot(fresh.slug).definition, 'web-dev-2')
    assert.deepEqual(calls, [['bot.define', { slug: 'web-dev', purpose: 'Build websites', model: 'provider/model-a' }]])

    await storage.saveFormBot({ ...saved, purpose: 'Build sites' }, async () => { throw new Error('an edit does not define') })
    assert.equal(storage.bot(fresh.slug).definition, 'web-dev-2')
    assert.equal(storage.bot(fresh.slug).purpose, 'Build sites')
  } finally { rmSync(profile, { recursive: true, force: true }) }
})

test('a bot made before definitions is migrated once, and the briefing never names its old memory after', async () => {
  const profile = mkdtempSync(join(tmpdir(), 'bravebot-migrate-'))
  const source = buildSync({ entryPoints: ['src/main/bots.ts'], bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'] }).outputFiles[0].text
  const module = { exports: {} }
  const mockedRequire = (id) => id === 'electron' ? { app: { getPath: () => profile } } : require(id)
  new Function('require', 'module', 'exports', source)(mockedRequire, module, module.exports)
  const storage = module.exports
  const old = { ...parseBots({ bots: [{ ...definition, model: 'provider/model-a' }] }).bots[0], definition: null }
  try {
    storage.saveBot(old)
    await assert.rejects(storage.migrateBot(old, null), /not running/)
    for (const answer of [null, {}, { name: '../x' }, { name: 42 }]) {
      await assert.rejects(storage.migrateBot(old, async () => answer), /did not name/)
    }
    await assert.rejects(storage.migrateBot(old, async () => { throw new Error('refused') }), /refused/)
    assert.equal(storage.bot(old.slug).definition, null, 'a failed migration leaves the bot as it was')

    const calls = []
    const migrated = await storage.migrateBot(old, async (method, params) => { calls.push([method, params]); return { name: 'web-dev-2' } })
    assert.equal(migrated.definition, 'web-dev-2')
    assert.equal(storage.bot(old.slug).definition, 'web-dev-2')
    assert.deepEqual(calls, [['bot.migrate', { slug: 'web-dev', purpose: 'Build websites', directory: '/tmp/web-dev', model: 'provider/model-a' }]])

    const again = await storage.migrateBot(migrated, async () => { throw new Error('a bot with a definition is not migrated again') })
    assert.equal(again.definition, 'web-dev-2')

    const prompt = storage.consolidationPrompt(migrated, 'why')
    assert.ok(prompt.includes('.bravebot/memory/web-dev-2.md'), prompt)
    assert.ok(!prompt.includes('.bravebot-ui'), prompt)
  } finally { rmSync(profile, { recursive: true, force: true }) }
})
