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
const { withBot } =module.exports
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

test('a form is made, edited and given a model by the agent, and a refusal saves no row', async () => {
  const profile = mkdtempSync(join(tmpdir(), 'bravebot-define-'))
  const file = fakeBots()
  const storage = load(profile, file)
  const sent = () => file.calls.filter(([method]) => method.startsWith('bot.') && !['bot.list', 'bot.replace'].includes(method))
  try {
    file.refuse = '/data/agents/web-dev.md: permission denied'
    await assert.rejects(storage.saveForm({ name: 'Web dev', purpose: 'Build websites' }), /permission denied/)
    assert.equal(await storage.bot('web-dev'), null)
    assert.deepEqual(file.rows, [], 'a make the agent refuses leaves no row')
    file.refuse = null
    file.calls.length = 0

    const saved = await storage.saveForm({ name: 'Web dev', purpose: 'Build websites', model: 'provider/model-a', avatar: 'seed-a', slug: 'not-held', home: '/elsewhere', directory: '/elsewhere', definition: 'other', session: 'x' })
    assert.deepEqual(sent(), [['bot.make', { dataFolder: profile, name: 'Web dev', purpose: 'Build websites', model: 'provider/model-a', avatar: 'seed-a' }]])
    assert.equal(saved.slug, 'web-dev', 'a slug the window names for a bot that is not kept is not used')
    assert.equal(saved.definition, 'web-dev')
    assert.equal(saved.home, join(profile, 'bot-homes', 'web-dev'))
    assert.equal((await storage.bot('web-dev')).purpose, 'Build websites')

    file.calls.length = 0
    const seeded = await storage.saveForm({ name: 'Web dev', purpose: 'Another' })
    const [[, made]] = sent()
    assert.match(made.avatar, /^v2:./, 'the desktop chooses the seed of a new face when the window sent none')
    assert.equal(seeded.slug, 'web-dev-2')
    assert.equal(seeded.avatar, made.avatar)
    assert.equal('model' in made, false, 'a model the form did not name is not sent')

    file.calls.length = 0
    const edited = await storage.saveForm({ slug: 'web-dev', purpose: 'Build sites', model: null })
    assert.deepEqual(sent(), [['bot.edit', { dataFolder: profile, slug: 'web-dev', purpose: 'Build sites', model: null }]])
    assert.equal(edited.model, null, 'null clears the model')
    assert.equal(edited.name, 'Web dev')

    file.calls.length = 0
    await storage.saveForm({ slug: 'web-dev', name: 'Web builder' })
    assert.deepEqual(sent(), [['bot.edit', { dataFolder: profile, slug: 'web-dev', name: 'Web builder' }]], 'an edit names only what it changes, so a model is left as it was')

    file.refuse = '/data/agents/web-dev.md: permission denied'
    await assert.rejects(storage.saveForm({ slug: 'web-dev', purpose: 'Changed' }), /permission denied/)
    file.refuse = null
    assert.equal((await storage.bot('web-dev')).purpose, 'Build sites', 'an edit the agent refuses changes nothing')

    file.calls.length = 0
    const switched = await storage.saveBotModel('web-dev', 'provider/model-b')
    assert.deepEqual(sent(), [['bot.set_model', { dataFolder: profile, slug: 'web-dev', model: 'provider/model-b' }]])
    assert.equal(switched.model, 'provider/model-b')
    assert.equal(switched.remembered, 0)
    await storage.saveBotModel('web-dev', null)
    assert.equal((await storage.bot('web-dev')).model, null)
    file.refuse = 'refused'
    await assert.rejects(storage.saveBotModel('web-dev', 'provider/model-c'), /refused/)
    file.refuse = null
    assert.equal((await storage.bot('web-dev')).model, null)

    await storage.saveBot(botRow({ slug: 'old-bot', model: null }))
    await storage.saveForm({ slug: 'old-bot', purpose: 'Still old' })
    await storage.saveBotModel('old-bot', 'provider/model-c')
    const old = await storage.bot('old-bot')
    assert.equal(old.definition, null)
    assert.equal(old.purpose, 'Still old')
    assert.equal(old.model, 'provider/model-c')
  } finally { rmSync(profile, { recursive: true, force: true }) }
})

test('saves from a window run one at a time and an edit changes only the fields it names', async () => {
  const profile = mkdtempSync(join(tmpdir(), 'bravebot-saves-'))
  const file = fakeBots()
  const storage = load(profile, file)
  try {
    assert.equal(await storage.saveForm(null), null)
    assert.equal(await storage.saveForm('a bot'), null)
    await assert.rejects(storage.saveForm({ name: '   ', purpose: 'x' }), /blank/, 'the refusal of a blank name is the agent’s and reaches the caller')
    await assert.rejects(storage.saveForm({ purpose: 'no name' }), /name/)

    const events = []
    let release
    const held = new Promise((resolve) => { release = resolve })
    let started
    const making = new Promise((resolve) => { started = resolve })
    storage.useBotsBridge(async (method, params) => {
      if (method === 'bot.make') { events.push([method, params.name]); started(); await held }
      else if (method === 'bot.edit') events.push([method, params.slug, params.avatar])
      return file.request(method, params)
    })
    const first = storage.saveForm({ name: 'Form bot', purpose: 'Fill in forms', avatar: 'seed-a' })
    const second = storage.saveForm({ slug: 'form-bot', avatar: 'seed-b' })
    await making
    await new Promise((resolve) => setImmediate(resolve))
    assert.deepEqual(events, [['bot.make', 'Form bot']], 'the second save has not reached the agent')
    release()
    await Promise.all([first, second])

    assert.deepEqual(events, [['bot.make', 'Form bot'], ['bot.edit', 'form-bot', 'seed-b']])
    assert.deepEqual(file.rows.map((row) => row.slug), ['form-bot'], 'the second save read the row the first left')
    const row = await storage.bot('form-bot')
    assert.equal(row.avatar, 'seed-b')
    assert.equal(row.purpose, 'Fill in forms')

    // A save the agent refuses does not stop the ones after it.
    file.refuse = 'refused'
    await assert.rejects(storage.saveForm({ slug: 'form-bot', purpose: 'Refused' }), /refused/)
    file.refuse = null
    const after = await storage.saveForm({ slug: 'form-bot', purpose: 'Allowed' })
    assert.equal(after.purpose, 'Allowed')
    assert.equal(after.avatar, 'seed-b')
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
    const rowBefore = structuredClone(file.rows.find((row) => row.slug === 'web-dev'))
    const writesBefore = file.calls.filter(([method]) => method === 'bot.replace').length
    await noteBotMemory('web-dev')
    assert.equal(file.calls.filter(([method]) => method === 'bot.replace').length, writesBefore, 'a turn end wrote the row of an addressed bot')
    assert.deepEqual(file.rows.find((row) => row.slug === 'web-dev'), rowBefore, 'a turn end changed the row of an addressed bot')
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
