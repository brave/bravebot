// The desktop's bots, through the real `bravebot-rpc` (BOT-3, BOT-4, BOT-5).
//
// Run after `pnpm run bridge`. The agent runs against a state directory and a data folder made for
// each test, so nothing here reads or writes the person's own `~/.bravebot` or desktop profile.
import test from 'node:test'
import assert from 'node:assert/strict'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { tmpdir } from 'node:os'
import { botRow, loadMain } from './fake-bots.mjs'

const RPC = join(process.cwd(), '..', 'target', 'debug', process.platform === 'win32' ? 'bravebot-rpc.exe' : 'bravebot-rpc')
const ENTRY = "export { Bridge } from './src/main/bridge'\nexport * from './src/main/bots'\nexport * from './src/main/bot-store'\n"

async function fixture() {
  assert.ok(existsSync(RPC), `build the agent first (pnpm run bridge): ${RPC}`)
  const root = realpathSync(mkdtempSync(join(tmpdir(), 'bravebot-bot-rows-')))
  const state = join(root, 'state')
  const profile = join(root, 'profile')
  mkdirSync(state)
  mkdirSync(profile)
  const before = process.env.BRAVEBOT_HOME
  process.env.BRAVEBOT_HOME = state
  const main = loadMain(ENTRY, { app: { getPath: () => profile, getAppPath: () => process.cwd(), isPackaged: false } })
  const bridge = new main.Bridge(() => {})
  main.useBotsBridge((method, params) => bridge.request(method, params))
  return {
    state, profile, main, file: join(state, 'bots.json'), old: join(profile, 'bravebot-ui.json'),
    clean: () => {
      bridge.dispose()
      if (before === undefined) delete process.env.BRAVEBOT_HOME
      else process.env.BRAVEBOT_HOME = before
      rmSync(root, { recursive: true, force: true })
    },
  }
}

const slugs = (rows) => rows.map((each) => each.slug)

test('a bot made by the desktop is in bots.json and is not in bravebot-ui.json', async () => {
  const f = await fixture()
  try {
    writeFileSync(f.old, JSON.stringify({ view: { grouped: true } }))
    const before = readFileSync(f.old, 'utf8')
    await f.main.saveBot(botRow({ slug: 'custodian', name: 'Custodian', home: join(f.profile, 'bot-homes', 'custodian') }))
    assert.deepEqual(slugs(JSON.parse(readFileSync(f.file, 'utf8')).bots), ['custodian'])
    assert.equal(readFileSync(f.old, 'utf8'), before, 'the desktop’s preferences file is not written')
  } finally { f.clean() }
})

test('a bot is listed again by a bridge started afterwards', async () => {
  const f = await fixture()
  try {
    await f.main.saveBot(botRow({ slug: 'custodian', home: join(f.profile, 'bot-homes', 'custodian') }))
    const restarted = new f.main.Bridge(() => {})
    try {
      f.main.useBotsBridge((method, params) => restarted.request(method, params))
      assert.deepEqual(slugs(await f.main.bots()), ['custodian'])
    } finally { restarted.dispose() }
  } finally { f.clean() }
})

const OLD_FORMS = (profile) => [
  { slug: 'zeta', name: 'Zeta', purpose: 'Bare conversation identifier and a directory', directory: '/projects/zeta', session: 'a1b2c3d4-0000-4000-8000-000000000001', conversations: ['a1b2c3d4-0000-4000-8000-000000000001'], remembered: 7, quiet: 2 },
  { slug: 'Bad Slug', name: 'Bad', purpose: 'A slug the module drops' },
  { slug: 'alpha', name: 'Alpha', purpose: 'No home, no avatar, no retired', unknownField: { kept: [1, 2] } },
  { slug: 'beta', name: 'Beta', purpose: 'Archived', home: join(profile, 'bot-homes', 'beta'), retired: 5 },
]

test('rows held in the old key of bravebot-ui.json move to bots.json unchanged, and the old file is left as it was', async () => {
  const f = await fixture()
  try {
    const rows = OLD_FORMS(f.profile)
    writeFileSync(f.old, JSON.stringify({ bots: rows, view: { grouped: true } }, null, 2))
    const before = readFileSync(f.old, 'utf8')

    assert.deepEqual(slugs(await f.main.bots()), ['zeta', 'alpha', 'beta'], 'the entry it drops is not listed, and the others are in file order')
    assert.deepEqual(JSON.parse(readFileSync(f.file, 'utf8')), { bots: rows }, 'every entry of the old key, in order, with every field')
    assert.equal(readFileSync(f.old, 'utf8'), before, 'bravebot-ui.json is unchanged')
    assert.match(f.main.botsProblem(), /Bad Slug.*was not read/)

    const listed = await f.main.bots()
    assert.equal(listed.find((each) => each.slug === 'alpha').home, join(f.profile, 'bot-homes', 'alpha'), 'a row with no home is given one in the data folder')
    assert.deepEqual(listed.find((each) => each.slug === 'zeta').conversations, [{ id: 'a1b2c3d4-0000-4000-8000-000000000001', directory: '/projects/zeta' }])

    writeFileSync(f.old, JSON.stringify({ bots: [botRow({ slug: 'new-key-row' })] }))
    assert.deepEqual(slugs(await f.main.bots()), ['zeta', 'alpha', 'beta'], 'a second start does not read the old key again')
  } finally { f.clean() }
})

test('an old file that cannot be converted gives no bots, names its path, and a make is refused', async () => {
  for (const content of ['{ not json', JSON.stringify({ bots: { not: 'an array' } })]) {
    const f = await fixture()
    try {
      writeFileSync(f.old, content)
      assert.deepEqual(await f.main.bots(), [])
      assert.ok(f.main.botsProblem().includes(f.old), f.main.botsProblem())
      await assert.rejects(f.main.saveBot(botRow({ slug: 'new' })), (error) => error.message.includes(f.old))
      assert.equal(existsSync(f.file), false, 'no bots file is written first, which would leave the rows in the old key unread')
    } finally { f.clean() }
  }
})

test('an old file with no bots in it leaves no bots file', async () => {
  for (const content of [JSON.stringify({ view: {} }), JSON.stringify({ bots: [] })]) {
    const f = await fixture()
    try {
      writeFileSync(f.old, content)
      assert.deepEqual(await f.main.bots(), [])
      assert.equal(f.main.botsProblem(), null)
      assert.equal(existsSync(f.file), false)
    } finally { f.clean() }
  }
})

test('a bots file that does not parse gives no bots, names its path and is not written over', async () => {
  const f = await fixture()
  try {
    writeFileSync(f.file, '{ not json')
    assert.deepEqual(await f.main.bots(), [])
    assert.ok(f.main.botsProblem().includes(f.file), f.main.botsProblem())
    await assert.rejects(f.main.saveBot(botRow({ slug: 'new' })), (error) => error.message.includes(f.file))
    assert.equal(readFileSync(f.file, 'utf8'), '{ not json')
  } finally { f.clean() }
})

test('a hand-edited file is listed in file order, a repaired row repaired, and a Windows path accepted on any system', async () => {
  const f = await fixture()
  try {
    const home = (slug) => join(f.profile, 'bot-homes', slug)
    writeFileSync(f.file, JSON.stringify({ bots: [
      { slug: 'second', name: 'Second', purpose: 'p', home: 'C:\\Users\\someone\\bots\\second', model: 'inherit', avatar: '  ' },
      { slug: 'first', name: 'First', purpose: 'p', home: home('first') },
    ] }))
    const listed = await f.main.bots()
    assert.deepEqual(slugs(listed), ['second', 'first'], 'not sorted on a read')
    assert.equal(listed[0].home, 'C:\\Users\\someone\\bots\\second')
    assert.equal(listed[0].model, null, 'a model the definition would refuse reads as none')
    assert.equal(listed[0].avatar, 'second', 'a blank avatar is the slug')
  } finally { f.clean() }
})

test('rows are written in the byte order of the slug, which is localeCompare’s for slugs of digits and hyphens', async () => {
  const f = await fixture()
  try {
    const names = ['b', 'a10', 'a2', 'a-1', 'a1', 'a', '10', '2', 'a-b', 'ab', 'b-2', 'b2', 'a-10', 'a-2']
    for (const slug of names) await f.main.saveBot(botRow({ slug, home: join(f.profile, 'bot-homes', slug) }))
    const written = slugs(JSON.parse(readFileSync(f.file, 'utf8')).bots)
    assert.deepEqual(written, [...names].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0)))
    assert.deepEqual(written, [...names].sort((a, b) => a.localeCompare(b, 'en')))
  } finally { f.clean() }
})

test('a file with a row that is dropped and a row past the hundredth reports each', async () => {
  const f = await fixture()
  try {
    const rows = Array.from({ length: 101 }, (_, n) => ({ slug: `bot-${String(n).padStart(3, '0')}`, name: 'N', purpose: 'p', home: join(f.profile, 'bot-homes', `bot-${n}`) }))
    rows.splice(3, 0, { slug: 'Not A Slug', name: 'N', purpose: 'p' })
    writeFileSync(f.file, JSON.stringify({ bots: rows }))
    assert.equal((await f.main.bots()).length, 100)
    const report = f.main.botsProblem()
    assert.match(report, /Bot Not A Slug of the bots file was not read/)
    assert.match(report, /Bot bot-100 of the bots file was not read.*past the 100th/)
  } finally { f.clean() }
})
