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

    // MEMORY-9: an edit rewrites the definition it has, and defines nothing new.
    calls.length = 0
    const edited = await storage.saveFormBot({ ...saved, purpose: 'Build sites', model: null }, async (method, params) => { calls.push([method, params]); return { name: 'web-dev-2' } })
    assert.deepEqual(calls, [['bot.redefine', { name: 'web-dev-2', purpose: 'Build sites' }]])
    assert.equal(edited.definition, 'web-dev-2')
    assert.equal(storage.bot(fresh.slug).purpose, 'Build sites')

    // A rewrite the agent refuses, or answers for another file, saves nothing: the row and the
    // file say the same thing or the edit did not happen.
    await assert.rejects(storage.saveFormBot({ ...edited, purpose: 'Changed' }, async () => { throw new Error('refused') }), /refused/)
    await assert.rejects(storage.saveFormBot({ ...edited, purpose: 'Changed' }, async () => ({ name: 'other' })), /did not rewrite/)
    await assert.rejects(storage.saveFormBot({ ...edited, purpose: 'Changed' }, null), /not running/)
    assert.equal(storage.bot(fresh.slug).purpose, 'Build sites')

    // The model picker is an edit too, and names the model the file is to carry.
    calls.length = 0
    const switched = await storage.saveBotModel(storage.bot(fresh.slug), 'provider/model-b', async (method, params) => { calls.push([method, params]); return { name: 'web-dev-2' } })
    assert.deepEqual(calls, [['bot.redefine', { name: 'web-dev-2', purpose: 'Build sites', model: 'provider/model-b' }]])
    assert.equal(switched.model, 'provider/model-b')
    await assert.rejects(storage.saveBotModel(switched, null, async () => { throw new Error('refused') }), /refused/)
    assert.equal(storage.bot(fresh.slug).model, 'provider/model-b')

    // A bot made before definitions existed has no file to rewrite and the agent is not asked.
    const legacy = { ...parseBots({ bots: [{ ...definition, slug: 'old-bot', model: null }] }).bots[0] }
    assert.equal(legacy.definition, null)
    storage.saveBot(legacy)
    await storage.saveFormBot({ ...legacy, purpose: 'Still old' }, async () => { throw new Error('a bot with no definition is not rewritten') })
    await storage.saveBotModel(storage.bot('old-bot'), 'provider/model-c', null)
    assert.equal(storage.bot('old-bot').definition, null)
    assert.equal(storage.bot('old-bot').model, 'provider/model-c')
  } finally { rmSync(profile, { recursive: true, force: true }) }
})

// `sanitised` lives in the Electron entry point, which no test can load, so this pins its source:
// a window's `turn.send` loses the `definition` it claims, and the only place that parameter is set
// is the send this process composes from a bot's row. Rejects the fault of forgetting the strip, which
// would let a window address a turn to any definition on the machine (MEMORY-10).
test('a window cannot name the definition a turn is addressed to', () => {
  const main = readFileSync('src/main/index.ts', 'utf8')
  const strip = main.slice(main.indexOf('function sanitised('), main.indexOf('app.whenReady()'))
  assert.match(strip, /definition: _definition/, 'the strip no longer removes `definition` from a window’s turn.send')
  const sets = [...main.matchAll(/params\.definition\s*=/g)]
  assert.equal(sets.length, 1, 'more than one place sets the definition a turn is addressed to')
  const sender = main.slice(main.indexOf('async function sendBotTurn('), main.indexOf('async function consolidate('))
  assert.match(sender, /params\.definition = held\.definition/, 'the definition is not the bot row’s')
  assert.match(sender, /if \(held\.definition !== null\) \{[\s\S]*?\} else if \(grounded\)/, 'a bot with a definition is also sent a briefing')
})

test('a bot with a definition keeps its memory where the agent keeps that definition’s', () => {
  const profile = mkdtempSync(join(tmpdir(), 'bravebot-memory-path-'))
  const source = buildSync({ entryPoints: ['src/main/bots.ts'], bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'] }).outputFiles[0].text
  const module = { exports: {} }
  const mockedRequire = (id) => id === 'electron' ? { app: { getPath: () => profile } } : require(id)
  new Function('require', 'module', 'exports', source)(mockedRequire, module, module.exports)
  const { memoryPath, nudgeDue, noteBotMemory, saveBot, bot } = module.exports
  try {
    const defined = { ...parseBots({ bots: [{ ...definition, definition: 'web-dev-2', quiet: 99 }] }).bots[0] }
    const legacy = { ...parseBots({ bots: [{ ...definition, slug: 'old-bot', quiet: 99 }] }).bots[0] }
    // The memory is the definition's, named after it and not after the slug the row is known by.
    assert.equal(memoryPath(defined), '.bravebot/memory/web-dev-2.md')
    assert.equal(memoryPath(legacy), '.bravebot-ui/bots/old-bot.md')
    // The quiet-turn counter decided when the briefing was sent again; a bot that is addressed has
    // no briefing, so it never asks for one and the turn end never counts.
    assert.equal(nudgeDue(defined), false)
    assert.equal(nudgeDue(legacy), true)
    saveBot(defined)
    saveBot(legacy)
    noteBotMemory('web-dev')
    noteBotMemory('old-bot')
    assert.equal(bot('web-dev').quiet, 99, 'a turn end counted a quiet turn against an addressed bot')
    assert.notEqual(bot('old-bot').quiet, 99)
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
    assert.deepEqual(calls, [['bot.migrate', { slug: 'web-dev', purpose: 'Build websites', directory: '/tmp/bot-homes/web-dev', model: 'provider/model-a' }]])

    const again = await storage.migrateBot(migrated, async () => { throw new Error('a bot with a definition is not migrated again') })
    assert.equal(again.definition, 'web-dev-2')

    const prompt = storage.consolidationPrompt(migrated, 'why')
    assert.ok(prompt.includes('.bravebot/memory/web-dev-2.md'), prompt)
    assert.ok(!prompt.includes('.bravebot-ui'), prompt)
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
