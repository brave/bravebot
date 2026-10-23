import test from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { buildSync } from 'esbuild'
import { existsSync, mkdtempSync, readFileSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { tmpdir } from 'node:os'
import { botRow, fakeBots, loadMain } from './fake-bots.mjs'

const require = createRequire(import.meta.url)
const source = buildSync({ entryPoints: ['src/shared/bots.ts'], bundle: true, write: false, platform: 'node', format: 'cjs' }).outputFiles[0].text
const module = { exports: {} }
new Function('require', 'module', 'exports', source)(require, module, module.exports)
const { withBot, isBotModel, isSlug, slugFor } = module.exports
const definition = botRow()

/** The main-process store over the bots `file` holds, with the profile Electron would name. */
function load(profile, file = fakeBots()) {
  const storage = loadMain("export * from './src/main/bots'\nexport { useBotsBridge } from './src/main/bot-store'\n", { app: { getPath: () => profile } })
  storage.useBotsBridge(file.request)
  return storage
}

test('model selection survives updates and unrelated bot edits', () => {
  const original = botRow({ model: 'provider/model-a' })
  const [changed] = withBot([original], { ...original, model: 'provider/model-b' })
  assert.equal(changed.model, 'provider/model-b')
  assert.equal(changed.avatar, definition.avatar)
  const [renamed] = withBot([changed], { ...changed, name: 'Frontend developer' })
  assert.equal(renamed.model, 'provider/model-b')
})

test('IPC model validation accepts opaque IDs and rejects malformed inputs', () => {
  for (const value of [null, 'openrouter/provider/model:free', 'custom-model']) assert.equal(isBotModel(value), true)
  for (const value of [undefined, '', ' ', 1, {}, 'a\0b', 'x'.repeat(513)]) assert.equal(isBotModel(value), false)
})

test('main-process storage retains model changes across a fresh module load, in the bots file and not in bravebot-ui.json', async () => {
  const profile = mkdtempSync(join(tmpdir(), 'bravebot-model-storage-'))
  const file = fakeBots()
  try {
    const storage = load(profile, file)
    await storage.saveBot(botRow({ model: 'provider/model-a' }))
    await storage.saveBot({ ...(await storage.bot(definition.slug)), model: 'provider/model-b' })
    assert.equal(file.rows[0].model, 'provider/model-b')
    assert.equal((await load(profile, file).bot(definition.slug)).model, 'provider/model-b')
    assert.equal(existsSync(join(profile, 'bravebot-ui.json')), false, 'the bots are not written to the desktop’s own preferences file')
  } finally { rmSync(profile, { recursive: true, force: true }) }
})

test('making a bot saves nothing unless the agent names the definition it wrote', async () => {
  const profile = mkdtempSync(join(tmpdir(), 'bravebot-define-'))
  const storage = load(profile)
  const fresh = botRow({ model: null, definition: null })
  try {
    await assert.rejects(storage.saveFormBot(fresh, null), /not running/)
    await assert.rejects(storage.saveFormBot(fresh, async () => { throw new Error('refused') }), /refused/)
    for (const answer of [null, {}, { name: '../x' }, { name: 42 }]) {
      await assert.rejects(storage.saveFormBot(fresh, async () => answer), /did not name/)
    }
    assert.equal(await storage.bot(fresh.slug), null)

    const calls = []
    const saved = await storage.saveFormBot({ ...fresh, model: 'provider/model-a' }, async (method, params) => { calls.push([method, params]); return { name: 'web-dev-2' } })
    assert.equal(saved.definition, 'web-dev-2')
    assert.equal((await storage.bot(fresh.slug)).definition, 'web-dev-2')
    assert.deepEqual(calls, [['bot.define', { slug: 'web-dev', purpose: 'Build websites', model: 'provider/model-a' }]])

    // MEMORY-9: an edit rewrites the definition it has, and defines nothing new.
    calls.length = 0
    const edited = await storage.saveFormBot({ ...saved, purpose: 'Build sites', model: null }, async (method, params) => { calls.push([method, params]); return { name: 'web-dev-2' } })
    assert.deepEqual(calls, [['bot.redefine', { name: 'web-dev-2', purpose: 'Build sites' }]])
    assert.equal(edited.definition, 'web-dev-2')
    assert.equal((await storage.bot(fresh.slug)).purpose, 'Build sites')

    // A rewrite the agent refuses, or answers for another file, saves nothing: the row and the
    // file say the same thing or the edit did not happen.
    await assert.rejects(storage.saveFormBot({ ...edited, purpose: 'Changed' }, async () => { throw new Error('refused') }), /refused/)
    await assert.rejects(storage.saveFormBot({ ...edited, purpose: 'Changed' }, async () => ({ name: 'other' })), /did not rewrite/)
    await assert.rejects(storage.saveFormBot({ ...edited, purpose: 'Changed' }, null), /not running/)
    assert.equal((await storage.bot(fresh.slug)).purpose, 'Build sites')

    // The model picker is an edit too, and names the model the file is to carry.
    calls.length = 0
    const switched = await storage.saveBotModel(await storage.bot(fresh.slug), 'provider/model-b', async (method, params) => { calls.push([method, params]); return { name: 'web-dev-2' } })
    assert.deepEqual(calls, [['bot.redefine', { name: 'web-dev-2', purpose: 'Build sites', model: 'provider/model-b' }]])
    assert.equal(switched.model, 'provider/model-b')
    await assert.rejects(storage.saveBotModel(switched, null, async () => { throw new Error('refused') }), /refused/)
    assert.equal((await storage.bot(fresh.slug)).model, 'provider/model-b')

    // A bot made before definitions existed has no file to rewrite and the agent is not asked.
    const legacy = botRow({ slug: 'old-bot', model: null })
    assert.equal(legacy.definition, null)
    await storage.saveBot(legacy)
    await storage.saveFormBot({ ...legacy, purpose: 'Still old' }, async () => { throw new Error('a bot with no definition is not rewritten') })
    await storage.saveBotModel(await storage.bot('old-bot'), 'provider/model-c', null)
    assert.equal((await storage.bot('old-bot')).definition, null)
    assert.equal((await storage.bot('old-bot')).model, 'provider/model-c')
  } finally { rmSync(profile, { recursive: true, force: true }) }
})

test('saves from a window run one at a time and an edit changes only the fields it names', async () => {
  const profile = mkdtempSync(join(tmpdir(), 'bravebot-saves-'))
  const storage = load(profile)
  try {
    const made = await storage.saveForm({ name: 'Form bot', purpose: 'Fill in forms', avatar: 'seed-a' }, async () => ({ name: 'form-bot' }))
    const { slug } = made
    assert.equal(await storage.saveForm({ name: '   ', purpose: 'x' }, null), null, 'a new bot still needs a name')
    assert.equal(await storage.saveForm({ purpose: 'no name' }, null), null, 'and a purpose, and a name')

    // A rename is held in the agent while a new face is chosen. The face waits for it, then is
    // applied to the row the rename left, and the rename does not bring the old face back.
    const events = []
    let release
    const held = new Promise((resolve) => { release = resolve })
    let started
    const renaming = new Promise((resolve) => { started = resolve })
    const rename = storage.saveForm({ slug, name: 'Renamed' }, async (method, params) => {
      events.push(['rename', params.purpose]); started(); await held
      // The row changes during the wait, as it does when a conversation is recorded.
      await storage.saveBot({ ...(await storage.bot(slug)), session: 'a1b2c3d4-0000-4000-8000-000000000001' })
      return { name: 'form-bot' }
    })
    const face = storage.saveForm({ slug, avatar: 'seed-b' }, async (method, params) => { events.push(['face', params.purpose]); return { name: 'form-bot' } })
    await renaming
    await new Promise((resolve) => setImmediate(resolve))
    assert.deepEqual(events, [['rename', 'Fill in forms']], 'the second save has not reached the agent')
    release()
    await Promise.all([rename, face])

    assert.deepEqual(events, [['rename', 'Fill in forms'], ['face', 'Fill in forms']])
    const row = await storage.bot(slug)
    assert.equal(row.name, 'Renamed')
    assert.equal(row.avatar, 'seed-b')
    assert.equal(row.purpose, 'Fill in forms')
    assert.equal(row.session, 'a1b2c3d4-0000-4000-8000-000000000001', 'what changed during the wait is kept')

    // A save the agent refuses does not stop the ones after it.
    await assert.rejects(storage.saveForm({ slug, purpose: 'Refused' }, async () => { throw new Error('refused') }), /refused/)
    const after = await storage.saveForm({ slug, purpose: 'Allowed' }, async () => ({ name: 'form-bot' }))
    assert.equal(after.purpose, 'Allowed')
    assert.equal(after.name, 'Renamed')
  } finally { rmSync(profile, { recursive: true, force: true }) }
})

// `sanitised` lives in the Electron entry point, which no test can load, so this pins its source:
// a window's `turn.send` loses the `definition` it claims, and the only place that parameter is set
// is the send this process composes from a bot's row. Rejects the fault of forgetting the strip, which
// would let a window address a turn to any definition on the machine (MEMORY-10).
test('a window cannot name the definition a turn is addressed to', () => {
  const main = readFileSync('src/main/index.ts', 'utf8')
  const strip = readFileSync('src/main/sanitise.ts', 'utf8')
  assert.match(strip, /definition: _definition/, 'the strip no longer removes `definition` from a window’s turn.send')
  const sets = [...main.matchAll(/params\.definition\s*=/g)]
  assert.equal(sets.length, 1, 'more than one place sets the definition a turn is addressed to')
  const sender = main.slice(main.indexOf('async function sendBotTurn('), main.indexOf('async function consolidate('))
  assert.match(sender, /params\.definition = held\.definition/, 'the definition is not the bot row’s')
  assert.match(sender, /if \(held\.definition !== null\) \{[\s\S]*?\} else if \(grounded\)/, 'a bot with a definition is also sent a briefing')
})

test('a bot with a definition keeps its memory where the agent keeps that definition’s', async () => {
  const profile = mkdtempSync(join(tmpdir(), 'bravebot-memory-path-'))
  const file = fakeBots()
  const { memoryPath, nudgeDue, noteBotMemory, saveBot } = load(profile, file)
  try {
    const defined = botRow({ definition: 'web-dev-2', quiet: 99 })
    const legacy = botRow({ slug: 'old-bot', quiet: 99 })
    // The memory is the definition's, named after it and not after the slug the row is known by.
    assert.equal(memoryPath(defined), '.bravebot/memory/web-dev-2.md')
    assert.equal(memoryPath(legacy), '.bravebot-ui/bots/old-bot.md')
    // The quiet-turn counter decided when the briefing was sent again; a bot that is addressed has
    // no briefing, so it never asks for one and the turn end never counts.
    assert.equal(nudgeDue(defined), false)
    assert.equal(nudgeDue(legacy), true)
    await saveBot(defined)
    await saveBot(legacy)
    const writes = () => file.calls.filter(([method]) => method === 'bot.replace').length
    const before = writes()
    await noteBotMemory('web-dev')
    assert.equal(writes(), before, 'a turn end counted a quiet turn against an addressed bot')
    await noteBotMemory('old-bot')
    assert.equal(writes(), before + 1, 'a turn end counts a quiet turn against a bot with no definition')
  } finally { rmSync(profile, { recursive: true, force: true }) }
})

test('a bot made before definitions is migrated once, and the briefing never names its old memory after', async () => {
  const profile = mkdtempSync(join(tmpdir(), 'bravebot-migrate-'))
  const storage = load(profile)
  const old = botRow({ model: 'provider/model-a', definition: null })
  try {
    await storage.saveBot(old)
    await assert.rejects(storage.migrateBot(old, null), /not running/)
    for (const answer of [null, {}, { name: '../x' }, { name: 42 }]) {
      await assert.rejects(storage.migrateBot(old, async () => answer), /did not name/)
    }
    await assert.rejects(storage.migrateBot(old, async () => { throw new Error('refused') }), /refused/)
    assert.equal((await storage.bot(old.slug)).definition, null, 'a failed migration leaves the bot as it was')

    const calls = []
    const migrated = await storage.migrateBot(old, async (method, params) => { calls.push([method, params]); return { name: 'web-dev-2' } })
    assert.equal(migrated.definition, 'web-dev-2')
    assert.equal((await storage.bot(old.slug)).definition, 'web-dev-2')
    assert.deepEqual(calls, [['bot.migrate', { slug: 'web-dev', purpose: 'Build websites', directory: '/tmp/bot-homes/web-dev', model: 'provider/model-a' }]])

    const again = await storage.migrateBot(migrated, async () => { throw new Error('a bot with a definition is not migrated again') })
    assert.equal(again.definition, 'web-dev-2')

    const prompt = storage.consolidationPrompt(migrated, 'why')
    assert.ok(prompt.includes('.bravebot/memory/web-dev-2.md'), prompt)
    assert.ok(!prompt.includes('.bravebot-ui'), prompt)
  } finally { rmSync(profile, { recursive: true, force: true }) }
})

test('a migrated bot is asked to carry its old notes over only in the folder they were recorded in', () => {
  const profile = mkdtempSync(join(tmpdir(), 'bravebot-carry-over-'))
  const { owesCarryOver, carryOverPrompt } = load(profile)
  const first = 'a1b2c3d4-0000-4000-8000-000000000001'
  const second = 'a1b2c3d4-0000-4000-8000-000000000002'
  try {
    const fresh = botRow({ definition: null, conversations: [] })
    const worked = { ...fresh, conversations: [{ id: first, directory: '/work/site' }, { id: second, directory: '/work/other' }] }
    // The record holds the home folder for a bot that has never worked anywhere else, and the folder
    // of its first conversation for one that has: a turn in any other folder names a file the
    // record does not cover.
    assert.equal(owesCarryOver(fresh, '/tmp/bot-homes/web-dev'), true)
    assert.equal(owesCarryOver(fresh, '/work/site'), false)
    assert.equal(owesCarryOver(worked, '/work/site'), true)
    assert.equal(owesCarryOver(worked, '/work/other'), false)
    assert.equal(owesCarryOver(worked, '/tmp/bot-homes/web-dev'), false)
    // A bot that has a definition has already been migrated, or never needed it.
    assert.equal(owesCarryOver({ ...worked, definition: 'web-dev-2' }, '/work/site'), false)

    // The turn names the old file to read and the definition's memory to write, and they are two
    // different files: a prompt built from one path would ask the bot to copy a file onto itself.
    const prompt = carryOverPrompt({ slug: 'web-dev', definition: 'web-dev-2' })
    assert.ok(prompt.includes('`.bravebot-ui/bots/web-dev.md`'), prompt)
    assert.ok(prompt.includes('`.bravebot/memory/web-dev-2.md`'), prompt)
    assert.ok(!prompt.includes('—'), 'no em-dash')
  } finally { rmSync(profile, { recursive: true, force: true }) }
})

test('a slug the desktop composes is always one the agent accepts, even when the cut falls on a word break', () => {
  const names = ['a'.repeat(47) + ' b', 'a'.repeat(47) + ' --- b', 'a'.repeat(60), '???', '', 'Web dev']
  for (const name of names) {
    const slug = slugFor(name)
    assert.equal(isSlug(slug), true, `${JSON.stringify(name)} -> ${slug}`)
    assert.ok(slug.length <= 48)
    for (const taken of [new Set([slug]), new Set([slug, `${slug}-2`])]) assert.equal(isSlug(slugFor(name, taken)), true)
  }
  assert.equal(slugFor('a'.repeat(47) + ' b'), 'a'.repeat(47))
})
