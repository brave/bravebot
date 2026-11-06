// Making, editing, setting the model, archiving, restoring and deleting a desktop bot, through the real `bravebot-rpc` (BOT-5, BOT-6, BOT-7, BOT-8).
// Run after `pnpm run bridge`.
import test from 'node:test'
import assert from 'node:assert/strict'
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, realpathSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { tmpdir } from 'node:os'
import { loadMain } from './fake-bots.mjs'

const RPC = join(process.cwd(), '..', 'target', 'debug', process.platform === 'win32' ? 'bravebot-rpc.exe' : 'bravebot-rpc')
const ENTRY = "export { Bridge } from './src/main/bridge'\nexport * from './src/main/bots'\nexport * from './src/main/bot-store'\n"

async function fixture() {
  assert.ok(existsSync(RPC), `build the agent first (pnpm run bridge): ${RPC}`)
  const root = realpathSync(mkdtempSync(join(tmpdir(), 'bravebot-bot-operations-')))
  const state = join(root, 'state')
  const profile = join(root, 'profile')
  mkdirSync(state)
  mkdirSync(profile)
  const before = process.env.BRAVEBOT_HOME
  process.env.BRAVEBOT_HOME = state
  const main = loadMain(ENTRY, { app: { getPath: () => profile, getAppPath: () => process.cwd(), isPackaged: false } })
  const bridge = new main.Bridge(() => {})
  main.useBotsBridge((method, params) => bridge.request(method, params))
  const agents = join(state, 'agents')
  const file = join(state, 'bots.json')
  const old = join(profile, 'bravebot-ui.json')
  return {
    state, profile, main, file, old, agents,
    definition: (name) => readFileSync(join(agents, `${name}.md`), 'utf8'),
    rows: () => JSON.parse(readFileSync(file, 'utf8')).bots,
    home: (slug) => join(profile, 'bot-homes', slug),
    noBotsKey: () => assert.equal(existsSync(old) && 'bots' in JSON.parse(readFileSync(old, 'utf8')), false, 'bravebot-ui.json never holds a bots key'),
    restarted: async () => {
      const again = new main.Bridge(() => {})
      main.useBotsBridge((method, params) => again.request(method, params))
      return { rows: await main.bots(), dispose: () => again.dispose() }
    },
    clean: () => {
      bridge.dispose()
      if (before === undefined) delete process.env.BRAVEBOT_HOME
      else process.env.BRAVEBOT_HOME = before
      rmSync(root, { recursive: true, force: true })
    },
  }
}

const isDirectory = (path) => existsSync(path) && statSync(path).isDirectory()

const bodyOf = (text) => text.slice(text.indexOf('\n---\n\n') + '\n---\n\n'.length)

test('a bot made through the bridge is in bots.json after a restart, with its definition and its home folder', async () => {
  const f = await fixture()
  try {
    const made = await f.main.saveForm({ name: 'Custodian', purpose: 'Keep the harbour lights lit', model: 'provider/model-a' })
    assert.equal(made.slug, 'custodian')
    assert.equal(made.model, 'provider/model-a')
    assert.equal(made.home, f.home('custodian'))
    assert.equal(isDirectory(made.home), true, 'the home folder is made')
    assert.equal(bodyOf(f.definition(made.definition)), 'Keep the harbour lights lit\n', 'the purpose is the body of the definition')

    const restarted = await f.restarted()
    try {
      assert.deepEqual(restarted.rows.map((each) => [each.slug, each.purpose, each.definition]), [['custodian', 'Keep the harbour lights lit', made.definition]])
    } finally { restarted.dispose() }
    assert.deepEqual(f.rows().map((each) => each.slug), ['custodian'])
    f.noBotsKey()
  } finally { f.clean() }
})

test('an edit rewrites the definition, a model change is in the row and the definition, and only ensure makes the home again', async () => {
  const f = await fixture()
  try {
    const made = await f.main.saveForm({ name: 'Custodian', purpose: 'Keep the harbour lights lit' })
    const edited = await f.main.saveForm({ slug: made.slug, purpose: 'Keep the lighthouse lit', avatar: 'seed-b' })
    assert.equal(edited.purpose, 'Keep the lighthouse lit')
    assert.equal(edited.name, 'Custodian', 'a field the edit does not name is kept')
    assert.equal(edited.avatar, 'seed-b')
    assert.equal(bodyOf(f.definition(made.definition)), 'Keep the lighthouse lit\n', 'the definition is rewritten')
    assert.equal(f.rows()[0].purpose, 'Keep the lighthouse lit')

    const switched = await f.main.saveBotModel(made.slug, 'provider/model-b')
    assert.equal(switched.model, 'provider/model-b')
    assert.match(f.definition(made.definition), /^model: 'provider\/model-b'$/m)
    const cleared = await f.main.saveBotModel(made.slug, null)
    assert.equal(cleared.model, null)
    assert.doesNotMatch(f.definition(made.definition), /^model:/m)
    assert.equal((await f.main.saveForm({ slug: made.slug, name: 'Keeper' })).model, null, 'an edit that does not name the model leaves it')

    rmSync(f.home(made.slug), { recursive: true, force: true })
    await f.main.saveBotModel(made.slug, 'provider/model-c')
    assert.equal(existsSync(f.home(made.slug)), false, 'setting the model does not make the home folder')
    assert.equal(await f.main.ensureBotHome(made.slug), true)
    assert.equal(isDirectory(f.home(made.slug)), true)
    rmSync(f.home(made.slug), { recursive: true, force: true })
    await f.main.saveForm({ slug: made.slug, name: 'Custodian' })
    assert.equal(isDirectory(f.home(made.slug)), true, 'an edit makes a home that was removed')
    assert.equal(await f.main.ensureBotHome('no-such-bot'), false)
    f.noBotsKey()
  } finally { f.clean() }
})

test('a name that is taken gets the next free slug and a definition of its own', async () => {
  const f = await fixture()
  try {
    const first = await f.main.saveForm({ name: 'Custodian', purpose: 'First purpose' })
    const second = await f.main.saveForm({ name: 'Custodian', purpose: 'Second purpose' })
    assert.deepEqual([first.slug, second.slug], ['custodian', 'custodian-2'])
    assert.notEqual(first.definition, second.definition)
    assert.equal(bodyOf(f.definition(first.definition)), 'First purpose\n')
    assert.equal(bodyOf(f.definition(second.definition)), 'Second purpose\n')
    assert.equal(second.home, f.home('custodian-2'))
    assert.deepEqual(f.rows().map((each) => each.slug), ['custodian', 'custodian-2'])
  } finally { f.clean() }
})

test('the 101st make is refused with every row kept, and a name with a null character is refused', async () => {
  const f = await fixture()
  try {
    await assert.rejects(f.main.saveForm({ name: 'bad\0name', purpose: 'p' }))
    await assert.rejects(f.main.saveForm({ name: 'name', purpose: 'bad\0purpose' }))
    assert.equal(existsSync(f.file), false, 'a refused make writes no bots file')
    assert.equal(existsSync(f.agents) ? readdirSync(f.agents).length : 0, 0, 'and no definition')

    for (let n = 0; n < 100; n++) await f.main.saveForm({ name: `Bot ${n}`, purpose: `Purpose ${n}` })
    assert.equal(f.rows().length, 100)
    await assert.rejects(f.main.saveForm({ name: 'One too many', purpose: 'p' }), (error) => error.code === 'bad_request')
    assert.equal(f.rows().length, 100, 'every row is kept')
    assert.equal(f.rows().some((each) => each.name === 'One too many'), false)
    assert.equal(readdirSync(f.agents).length, 100, 'no definition is written for the refused one')
    assert.equal(f.main.botsProblem(), null, 'a refusal of what was asked for is not a problem with the bots file')
    f.noBotsKey()
  } finally { f.clean() }
})

test('a bots file that does not parse is refused for a make, is left as it was, and no definition or preferences key is written', async () => {
  const f = await fixture()
  try {
    const unparsable = '{"bots": [ not json'
    writeFileSync(f.file, unparsable)
    await assert.rejects(f.main.saveForm({ name: 'Custodian', purpose: 'Keep the harbour lights lit' }), /bots\.json/)
    assert.equal(readFileSync(f.file, 'utf8'), unparsable, 'the file is byte-identical')
    assert.equal(existsSync(f.agents) ? readdirSync(f.agents).length : 0, 0, 'no definition file was written')
    assert.equal(isDirectory(f.home('custodian')), false, 'no home folder was made')
    assert.match(f.main.botsProblem() ?? '', /bots\.json/, 'the file that cannot be read is named as the problem')
    f.noBotsKey()
  } finally { f.clean() }
})

const SESSION = '33333333-3333-4333-8333-333333333333'

const CHANGES = {
  'an edit': {
    start: (main, slug) => main.saveForm({ slug, purpose: 'New purpose' }),
    check: (row, definition) => {
      assert.equal(row.purpose, 'New purpose', 'the edit is in the file')
      assert.equal(bodyOf(definition), 'New purpose\n', 'the definition holds the new purpose')
    },
  },
  'a model change': {
    start: (main, slug) => main.saveBotModel(slug, 'provider/model-b'),
    check: (row, definition) => {
      assert.equal(row.model, 'provider/model-b', 'the model is in the file')
      assert.match(definition, /^model: 'provider\/model-b'$/m, 'and in the definition')
    },
  },
}

for (const [name, change] of Object.entries(CHANGES)) {
  for (const order of ['change first', 'conversation first']) {
    test(`${name} and a conversation recorded at the same moment both reach bots.json (${order})`, async () => {
      const f = await fixture()
      try {
        const other = await f.main.saveForm({ name: 'Bystander', purpose: 'Stay as written' })
        const made = await f.main.saveForm({ name: 'Custodian', purpose: 'Keep the harbour lights lit' })
        const folder = join(f.state, 'worked-in')
        mkdirSync(folder)
        const write = () => change.start(f.main, made.slug)
        const record = () => f.main.noteBotSession(made.slug, SESSION, folder)
        await Promise.all(order === 'change first' ? [write(), record()] : [record(), write()])

        const rows = f.rows()
        assert.deepEqual(rows.map((each) => each.slug), [made.slug, other.slug].sort(), 'no row is lost')
        const row = rows.find((each) => each.slug === made.slug)
        change.check(row, f.definition(made.definition))
        assert.deepEqual(row.conversations, [{ id: SESSION, directory: folder }], 'and the conversation')
        assert.equal(row.session, SESSION)
        assert.equal(rows.find((each) => each.slug === other.slug).purpose, 'Stay as written')
      } finally { f.clean() }
    })
  }
}

const waitUntil = async (laterThan) => {
  const start = Date.now()
  while (Date.now() <= laterThan) {
    if (Date.now() - start > 3000) throw new Error(`the clock did not move past ${laterThan}`)
    await new Promise((resolve) => setTimeout(resolve, 2))
  }
}

test('archiving an archived bot stamps retired again without rewriting the definition, and restoring sets it to zero', async () => {
  const f = await fixture()
  try {
    const made = await f.main.saveForm({ name: 'Custodian', purpose: 'Keep the harbour lights lit', model: 'provider/model-a' })
    const definition = join(f.agents, `${made.definition}.md`)
    const bytes = Buffer.concat([readFileSync(definition), Buffer.from('Hand edited.\n')])
    writeFileSync(definition, bytes)

    await waitUntil(made.updated)
    const first = await f.main.retireBot(made.slug, true)
    assert.ok(first.retired > 0)
    assert.notEqual(first.updated, made.updated)
    assert.equal(first.retired, first.updated)
    assert.equal(first.purpose, made.purpose)
    assert.equal(first.created, made.created)
    assert.deepEqual(readFileSync(definition), bytes)

    await waitUntil(first.retired)
    const second = await f.main.retireBot(made.slug, true)
    assert.ok(second.retired > first.retired)
    assert.notEqual(second.updated, first.updated)
    assert.equal(second.name, made.name)
    assert.equal(second.model, made.model)
    assert.deepEqual(readFileSync(definition), bytes)

    await waitUntil(second.updated)
    const restored = await f.main.retireBot(made.slug, false)
    assert.equal(restored.retired, 0)
    assert.notEqual(restored.updated, second.updated)
    assert.equal(restored.slug, made.slug)
    assert.deepEqual(readFileSync(definition), bytes)
    assert.equal(f.rows().find((each) => each.slug === made.slug).retired, 0)
  } finally { f.clean() }
})

test('a delete removes the row and the bot folder and leaves the definition, the session, the memory, the home and a sibling', async () => {
  const f = await fixture()
  try {
    const made = await f.main.saveForm({ name: 'Custodian', purpose: 'Keep the harbour lights lit' })
    const other = await f.main.saveForm({ name: 'Bystander', purpose: 'Stay as written' })
    const definition = join(f.agents, `${made.definition}.md`)
    const definitionBytes = readFileSync(definition)
    const session = join(f.state, 'sessions', 'worked', '33333333-3333-4333-8333-333333333333.json')
    mkdirSync(join(f.state, 'sessions', 'worked'), { recursive: true })
    writeFileSync(session, '{"id":"kept"}\n')
    const memory = join(f.profile, 'checkout', '.bravebot', 'memory', `${made.definition}.md`)
    mkdirSync(join(f.profile, 'checkout', '.bravebot', 'memory'), { recursive: true })
    writeFileSync(memory, 'notes from the folder\n')
    const history = join(f.profile, 'bots', made.slug)
    mkdirSync(join(history, 'nested'), { recursive: true })
    writeFileSync(join(history, 'nested', 'history.json'), '{"at":1}')
    const sibling = join(f.profile, 'bots', other.slug)
    mkdirSync(sibling, { recursive: true })
    writeFileSync(join(sibling, 'keep.txt'), 'stay')
    assert.equal(isDirectory(f.home(made.slug)), true)

    assert.equal(await f.main.removeBot(made.slug, false), made.slug)

    assert.equal(f.rows().some((each) => each.slug === made.slug), false)
    assert.equal(f.rows().find((each) => each.slug === other.slug).purpose, 'Stay as written')
    assert.equal(existsSync(history), false)
    assert.equal(readFileSync(join(sibling, 'keep.txt'), 'utf8'), 'stay')
    assert.deepEqual(readFileSync(definition), definitionBytes)
    assert.equal(readFileSync(session, 'utf8'), '{"id":"kept"}\n')
    assert.equal(readFileSync(memory, 'utf8'), 'notes from the folder\n')
    assert.equal(isDirectory(f.home(made.slug)), true)
    assert.equal(isDirectory(f.home(other.slug)), true)
    f.noBotsKey()
  } finally { f.clean() }
})
