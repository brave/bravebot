// Where the desktop keeps its bots: the agent's bots file, asked for through the bridge (BOT-4, BOT-5).
//
// The agent is stood in for by `fakeBots`, so what these pin is what the main process asks, what
// it does with the answer and what it refuses to do, and not what the agent decides. The agent's
// own rules have tests in `bravebot-agent`, and `bot-rows-bridge.test.mjs` runs both together.
import test from 'node:test'
import assert from 'node:assert/strict'
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { tmpdir } from 'node:os'
import { botRow, fakeBots, loadMain } from './fake-bots.mjs'

const ENTRY = "export * from './src/main/bots'\nexport * from './src/main/bot-store'\nexport * from './src/main/state'\n"
const ids = ['11111111-1111-4111-8111-111111111111', '22222222-2222-4222-8222-222222222222']

function fixture(rows = []) {
  const profile = mkdtempSync(join(tmpdir(), 'bravebot-bot-store-'))
  const file = fakeBots(rows)
  const main = loadMain(ENTRY, { app: { getPath: () => profile, getAppPath: () => process.cwd(), isPackaged: false } })
  main.useBotsBridge(file.request)
  return { profile, file, main, clean: () => rmSync(profile, { recursive: true, force: true }) }
}

const OLD_FILE = (profile) => join(profile, 'bravebot-ui.json')

test('the bots are listed and replaced through the bridge, with the data folder the app was given', async () => {
  const { profile, file, main, clean } = fixture([botRow({ slug: 'kept' })])
  try {
    assert.deepEqual((await main.bots()).map((each) => each.slug), ['kept'])
    await main.saveBot(botRow({ slug: 'made' }))
    assert.deepEqual(file.rows.map((each) => each.slug), ['kept', 'made'])
    const methods = file.calls.map(([method]) => method)
    assert.deepEqual([...new Set(methods)].sort(), ['bot.list', 'bot.replace'])
    for (const [, params] of file.calls) assert.equal(params.dataFolder, profile, 'the folder Electron names, never one composed here')
  } finally { clean() }
})

test('the bots key of bravebot-ui.json is neither read nor written', async () => {
  const { profile, file, main, clean } = fixture([botRow({ slug: 'in-the-bots-file' })])
  try {
    const old = JSON.stringify({ bots: [botRow({ slug: 'in-the-old-key' })], view: { grouped: true } })
    writeFileSync(OLD_FILE(profile), old)
    assert.deepEqual((await main.bots()).map((each) => each.slug), ['in-the-bots-file'], 'the old key is not a source of bots')
    await main.saveBot(botRow({ slug: 'another' }))
    assert.equal(readFileSync(OLD_FILE(profile), 'utf8'), old, 'saving a bot does not touch the file')
    assert.deepEqual(file.rows.map((each) => each.slug), ['another', 'in-the-bots-file'])

    main.putLayout({ columns: [1, 2, 3] })
    assert.equal('bots' in JSON.parse(readFileSync(OLD_FILE(profile), 'utf8')), false, 'the next write of the file drops the key, as the design says')
    assert.equal('bots' in main.readState(), false)
  } finally { clean() }
})

test('the entries the agent dropped and a file it could not read are reported with their path and reason', async () => {
  const { file, main, clean } = fixture([botRow({ slug: 'kept' })])
  try {
    assert.equal(main.botsProblem(), null)
    file.dropped = [{ index: 1, slug: 'Bad Slug', reason: 'its slug is not lowercase' }, { index: 4, slug: null, reason: 'it is not an object' }]
    assert.deepEqual((await main.bots()).map((each) => each.slug), ['kept'])
    const report = main.botsProblem()
    assert.match(report, /Bot Bad Slug of the bots file was not read.*its slug is not lowercase/)
    assert.match(report, /Entry 5 of the bots file was not read.*it is not an object/)

    file.dropped = []
    file.unreadable = { path: '/home/someone/.bravebot/bots.json', reason: 'expected value at line 1' }
    assert.deepEqual(await main.bots(), [])
    assert.match(main.botsProblem(), /\/home\/someone\/\.bravebot\/bots\.json.*expected value at line 1/)

    file.unreadable = null
    await main.bots()
    assert.equal(main.botsProblem(), null, 'a clean read clears it')
  } finally { clean() }
})

test('a write the agent refuses is not shown as saved, and says why', async () => {
  const { file, main, clean } = fixture([botRow({ slug: 'kept' })])
  try {
    file.refuse = '/home/someone/.bravebot/bots.json: the disk is full'
    await assert.rejects(main.saveBot(botRow({ slug: 'new' })), /the disk is full/)
    assert.deepEqual((await main.bots()).map((each) => each.slug), ['kept'], 'the refused bot is not in the list')
    assert.match(main.botsProblem(), /bots\.json: the disk is full/, 'and a read after it does not make the failure vanish')

    await assert.rejects(main.saveForm({ name: 'Form bot', purpose: 'Fill in forms' }, async () => ({ name: 'form-bot' })), /the disk is full/)
    assert.deepEqual((await main.bots()).map((each) => each.slug), ['kept'])

    file.refuse = null
    await main.saveBot(botRow({ slug: 'new' }))
    assert.deepEqual((await main.bots()).map((each) => each.slug), ['kept', 'new'])
    assert.equal(main.botsProblem(), null, 'a write that works clears it')
  } finally { clean() }
})

test('a bots file that cannot be read is never written over, and a bot is not defined for it', async () => {
  const { file, main, clean } = fixture([botRow({ slug: 'kept' })])
  try {
    file.unreadable = { path: '/home/someone/.bravebot/bots.json', reason: 'expected value at line 1' }
    await assert.rejects(main.saveBot(botRow({ slug: 'new' })), /bots\.json.*expected value/)
    assert.equal(file.calls.some(([method]) => method === 'bot.replace'), false, 'a list that is empty because the file did not parse is not the base of a write')

    let defined = false
    await assert.rejects(main.saveForm({ name: 'Form bot', purpose: 'Fill in forms' }, async () => { defined = true; return { name: 'form-bot' } }), /bots\.json/)
    assert.equal(defined, false, 'no definition is written for a bot that cannot be kept')
  } finally { clean() }
})

test('changes made at the same moment each reach the file', async () => {
  const { file, main, clean } = fixture([botRow({ slug: 'review' })])
  try {
    await Promise.all([
      main.noteBotSession('review', ids[0], '/project'),
      main.noteBotSession('review', ids[1], '/other'),
      main.saveBot(botRow({ slug: 'second' })),
      main.noteBotArchived('review', 3),
    ])
    const review = file.rows.find((each) => each.slug === 'review')
    assert.deepEqual(review.conversations.map((each) => each.id).sort(), [...ids].sort())
    assert.equal(review.archived, 3)
    assert.deepEqual(file.rows.map((each) => each.slug), ['review', 'second'])
  } finally { clean() }
})

test('with no agent running the bots are none, and nothing is written', async () => {
  const { main, clean } = fixture([botRow()])
  try {
    main.useBotsBridge(null)
    assert.deepEqual(await main.bots(), [])
    assert.match(main.botsProblem(), /agent is not running/)
    await assert.rejects(main.saveBot(botRow({ slug: 'new' })), /agent is not running/)
  } finally { clean() }
})

test('the methods that list and replace the bots are not among those a window may send', () => {
  const index = readFileSync('src/main/index.ts', 'utf8')
  const allowed = /const ALLOWED = new Set\(\[([^\]]*)\]\)/.exec(index)
  assert.ok(allowed, 'the list of methods the window may send is still one literal')
  const methods = [...allowed[1].matchAll(/'([^']+)'/g)].map((each) => each[1])
  for (const method of ['bot.list', 'bot.replace', 'bot.define', 'bot.redefine', 'bot.migrate']) {
    assert.equal(methods.includes(method), false, `${method} is a method only the main process calls`)
  }
  assert.ok(methods.includes('session.list'), 'the pattern read the list')
  assert.equal(existsSync('src/main/bot-store.ts'), true)
})
