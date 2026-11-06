// Where the desktop keeps its bots: the agent's bots file, asked for through the bridge (BOT-4, BOT-5).
//
// The agent is stood in for by `fakeBots`, so what these pin is what the main process asks, what
// it does with the answer and what it refuses to do, and not what the agent decides. The agent's
// own rules have tests in `bravebot-agent`, and `bot-rows-bridge.test.mjs` runs both together.
import test from 'node:test'
import assert from 'node:assert/strict'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
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

const PREFERENCES = {
  putLayout: [{ left: 240, right: 300, collapsed: { left: false, right: true } }, 'layout'],
  putView: [{ grouped: true, collapsed: [], tab: 'bots' }, 'view'],
  putTheme: ['dark', 'theme'],
  putRecents: [['/work/project'], 'recents'],
  putForks: [[{ child: { directory: '/work/project', id: ids[0] }, parent: { directory: '/work/project', id: ids[1] }, prompt: 1, at: 5 }], 'forks'],
}
const OLD_BOTS_FILE = (profile) => JSON.stringify({ layout: { left: 1, right: 2, collapsed: { left: false, right: false } }, bots: [botRow({ slug: 'only-copy' })], recents: ['/work/earlier'] }, null, 2) + '\n'

const FAILED_FIRST_READS = {
  'a list the agent could not answer': (file, main, profile) => main.useBotsBridge(async () => { throw new Error('the agent exited') }),
  'a list that reports the old file as unreadable': (file, main, profile) => {
    file.unreadable = { path: OLD_FILE(profile), reason: 'the lock was held' }
    file.oldRowsWaiting = true
  },
  'a list that reports a bravebot-ui.json that does not parse': (file, main, profile) => {
    writeFileSync(OLD_FILE(profile), '{"bots": [')
    file.unreadable = { path: OLD_FILE(profile), reason: 'expected value at line 1' }
    file.oldRowsWaiting = true
  },
  'a list for a machine that names no state directory, with rows still in the old key': (file) => { file.oldRowsWaiting = true },
  'a list from an agent that does not say whether rows are waiting': (file) => { file.leaveOut = ['oldRowsWaiting'] },
}

for (const [name, fail] of Object.entries(FAILED_FIRST_READS)) {
  test(`a preference is not written to bravebot-ui.json after ${name}, and is written once a list answers`, async () => {
    const { profile, file, main, clean } = fixture([botRow({ slug: 'kept' })])
    try {
      writeFileSync(OLD_FILE(profile), OLD_BOTS_FILE(profile))
      fail(file, main, profile)
      const before = readFileSync(OLD_FILE(profile))
      await main.bots()
      for (const [put, [value, key]] of Object.entries(PREFERENCES)) {
        main[put](value)
        assert.deepEqual(readFileSync(OLD_FILE(profile)), before, `${put} left the old file as it was found`)
        assert.deepEqual(main.readState()[key], value, `${put} is still what the window is told`)
      }

      main.useBotsBridge(file.request)
      file.unreadable = null
      file.oldRowsWaiting = false
      file.leaveOut = []
      await main.bots()
      const written = JSON.parse(readFileSync(OLD_FILE(profile), 'utf8'))
      for (const [put, [value, key]] of Object.entries(PREFERENCES)) assert.deepEqual(written[key], value, `${put} reached the file once a list answered`)
      assert.equal('bots' in written, false, 'and the file no longer carries the bots key')
    } finally { clean() }
  })
}

test('a bots file that does not parse, with no old rows waiting, does not hold preferences back, and the problem is still shown', async () => {
  const { profile, file, main, clean } = fixture([botRow({ slug: 'kept' })])
  try {
    writeFileSync(OLD_FILE(profile), JSON.stringify({ view: { grouped: true, collapsed: [], tab: 'sessions' } }))
    file.unreadable = { path: join(profile, 'bots.json'), reason: 'expected value at line 1' }
    file.oldRowsWaiting = false
    await main.bots()
    main.putTheme('dark')
    main.putRecents(['/p', '/q'])
    const written = JSON.parse(readFileSync(OLD_FILE(profile), 'utf8'))
    assert.equal(written.theme, 'dark')
    assert.deepEqual(written.recents, ['/p', '/q'])
    assert.match(main.botsProblem(), /bots\.json/, 'the person is still told the bots file cannot be read')
  } finally { clean() }
})

test('a machine that names no state directory, with rows in the old key, says so and keeps the old file as it was', async () => {
  const { profile, file, main, clean } = fixture([])
  try {
    writeFileSync(OLD_FILE(profile), OLD_BOTS_FILE(profile))
    const before = readFileSync(OLD_FILE(profile))
    file.oldRowsWaiting = true
    await main.bots()
    main.putTheme('dark')
    assert.deepEqual(readFileSync(OLD_FILE(profile)), before)
    const problem = main.botsProblem()
    assert.match(problem, /earlier version/)
    assert.ok(problem.includes(OLD_FILE(profile)), 'it names the file that holds them')
    assert.match(problem, /preferences are not saved/)
  } finally { clean() }
})

test('a preference is written at once after a list has answered, even if a later read fails', async () => {
  const { profile, file, main, clean } = fixture([botRow({ slug: 'kept' })])
  try {
    await main.bots()
    file.unreadable = { path: '/home/someone/.bravebot/bots.json', reason: 'expected value at line 1' }
    await main.bots()
    main.putTheme('dark')
    assert.equal(JSON.parse(readFileSync(OLD_FILE(profile), 'utf8')).theme, 'dark')
  } finally { clean() }
})

test('the first read of the bots is waited for only so long, and preferences stay held until a list answers', async () => {
  const { profile, file, main, clean } = fixture([botRow({ slug: 'kept' })])
  try {
    writeFileSync(OLD_FILE(profile), OLD_BOTS_FILE(profile))
    const before = readFileSync(OLD_FILE(profile))
    main.useBotsBridge(() => new Promise(() => {}))
    const started = Date.now()
    await main.firstBotsRead(40)
    assert.ok(Date.now() - started < 2000, 'an agent that never answers does not keep the window from loading')
    main.putTheme('dark')
    assert.deepEqual(readFileSync(OLD_FILE(profile)), before, 'and until it answers the file is not written')

    main.useBotsBridge(file.request)
    await main.bots()
    assert.equal(JSON.parse(readFileSync(OLD_FILE(profile), 'utf8')).theme, 'dark')

    const quick = Date.now()
    await main.firstBotsRead(60_000)
    assert.ok(Date.now() - quick < 2000, 'an answer ends the wait at once')
  } finally { clean() }
})

test('a save answers with the row the agent wrote, not the one it was sent', async () => {
  const { file, main, clean } = fixture([botRow({ slug: 'plain', name: 'Plain', purpose: 'Stored purpose', avatar: 'v2:stored', model: 'provider/stored', definition: 'plain', created: 5, updated: 5 })])
  try {
    const made = await main.saveForm({ name: 'Form bot', purpose: 'Fill in forms' })
    const [, sent] = file.calls.filter(([method]) => method === 'bot.make').at(-1)
    assert.deepEqual(Object.keys(sent).sort(), ['avatar', 'dataFolder', 'name', 'purpose'])
    assert.equal(made.slug, 'form-bot', 'the slug is the agent\'s')
    assert.equal(made.definition, 'form-bot')
    assert.match(made.home, /bot-homes[\\/]form-bot$/)
    assert.ok(made.created > 0 && made.updated > 0)
    assert.equal(made.avatar, sent.avatar, 'the seed the window sent is the avatar the agent kept')
    assert.deepEqual(made, main.withoutFigures(file.rows.find((each) => each.slug === 'form-bot')))

    const edited = await main.saveForm({ slug: 'plain', purpose: 'New purpose' })
    assert.equal(edited.purpose, 'New purpose')
    assert.equal(edited.name, 'Plain', 'a field the edit did not name is the stored one')
    assert.equal(edited.avatar, 'v2:stored')
    assert.equal(edited.model, 'provider/stored')
    assert.equal(edited.created, 5)
    assert.ok(edited.updated > 5, 'the agent stamped the change')
    assert.deepEqual(edited, main.withoutFigures(file.rows.find((each) => each.slug === 'plain')))

    const picked = await main.saveBotModel('plain', 'provider/other')
    assert.equal(picked.model, 'provider/other')
    assert.equal(picked.purpose, 'New purpose', 'the rest of the stored row comes back with it')
    assert.deepEqual(picked, main.withoutFigures(file.rows.find((each) => each.slug === 'plain')))
    const cleared = await main.saveBotModel('plain', null)
    assert.equal(cleared.model, null)
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

    await assert.rejects(main.saveForm({ name: 'Form bot', purpose: 'Fill in forms' }), /the disk is full/)
    assert.deepEqual((await main.bots()).map((each) => each.slug), ['kept'])

    file.refuse = null
    await main.saveBot(botRow({ slug: 'new' }))
    assert.deepEqual((await main.bots()).map((each) => each.slug), ['kept', 'new'])
    assert.equal(main.botsProblem(), null, 'a write that works clears it')
  } finally { clean() }
})

test('a bots file that cannot be read is never written over, and a bot is not made for it', async () => {
  const { file, main, clean } = fixture([botRow({ slug: 'kept' })])
  try {
    file.unreadable = { path: '/home/someone/.bravebot/bots.json', reason: 'expected value at line 1' }
    await assert.rejects(main.saveBot(botRow({ slug: 'new' })), /bots\.json.*expected value/)
    assert.equal(file.calls.some(([method]) => method === 'bot.replace'), false, 'a list that is empty because the file did not parse is not the base of a write')

    await assert.rejects(main.saveForm({ name: 'Form bot', purpose: 'Fill in forms' }), /bots\.json.*expected value/)
    assert.equal(file.calls.some(([method]) => method === 'bot.replace'), false, 'a make is not made from the empty list either')
    assert.deepEqual(file.rows.map((each) => each.slug), ['kept'], 'and the agent, which refuses it, leaves the rows as they were')
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

test('with no agent running a make, an edit and a model change are refused as not running, and nothing is written', async () => {
  const { file, main, clean } = fixture([botRow({ slug: 'held' })])
  try {
    const before = structuredClone(file.rows)
    const calls = file.calls.length
    main.useBotsBridge(null)
    await assert.rejects(main.saveForm({ name: 'Form bot', purpose: 'Fill in forms' }), /not running/)
    await assert.rejects(main.saveForm({ slug: 'held', purpose: 'New purpose' }), /not running/)
    await assert.rejects(main.saveBotModel('held', 'provider/model'), /not running/)
    assert.equal(file.calls.length, calls, 'no request reached the agent')
    assert.deepEqual(file.rows, before)
  } finally { clean() }
})

test('the remove handler passes the turn table and does not read the list to decide the flag', () => {
  const index = readFileSync('src/main/index.ts', 'utf8')
  const handler = /ipcMain\.handle\('bravebot:bots:remove',[\s\S]*?\n  \}\)/.exec(index)
  assert.ok(handler, 'the remove handler is still one function')
  assert.match(handler[0], /return removeBot\(slug, botRunning\(slug\)\)/)
  assert.equal(handler[0].includes('bot('), false)
})

test('the methods that make, edit and set the model of a bot, and ensure its home, are not among those a window may send', () => {
  const index = readFileSync('src/main/index.ts', 'utf8')
  const allowed = /const ALLOWED = new Set\(\[([^\]]*)\]\)/.exec(index)
  assert.ok(allowed, 'the list of methods the window may send is still one literal')
  const methods = [...allowed[1].matchAll(/'([^']+)'/g)].map((each) => each[1])
  assert.ok(methods.includes('session.list'), 'the pattern read the list')
  for (const method of ['bot.make', 'bot.edit', 'bot.set_model', 'bot.ensure_home', 'bot.archive', 'bot.restore', 'bot.delete']) {
    assert.equal(methods.includes(method), false, `${method} is a method only the main process calls`)
  }
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

test('the data folder Electron names is sent with each method that makes, edits, archives, restores, deletes or sets the model of a bot, and with the home', async () => {
  const { profile, file, main, clean } = fixture([])
  try {
    await main.saveForm({ name: 'Form bot', purpose: 'Fill in forms' })
    await main.saveForm({ slug: 'form-bot', purpose: 'Changed' })
    await main.saveBotModel('form-bot', 'provider/model-a')
    await main.ensureBotHome('form-bot')
    const calls = file.calls.length
    assert.equal(await main.retireBot('form-bot', 'yes'), null)
    assert.equal(file.calls.length, calls, 'a retired value that is not a boolean calls nothing')
    const archived = await main.retireBot('form-bot', true)
    const restored = await main.retireBot('form-bot', false)
    assert.notEqual(archived.retired, 0)
    assert.notEqual(archived.retired, restored.retired)
    assert.equal(restored.retired, 0)
    assert.notEqual(archived.updated, restored.updated)
    assert.equal(await main.removeBot('form-bot', false), 'form-bot')
    for (const method of ['bot.make', 'bot.edit', 'bot.set_model', 'bot.ensure_home', 'bot.archive', 'bot.restore', 'bot.delete']) {
      const sent = file.calls.filter(([each]) => each === method)
      assert.equal(sent.length, 1, `${method} was sent once`)
      assert.equal(sent[0][1].dataFolder, profile, `${method} carries the folder Electron names, never one composed here`)
    }
  } finally { clean() }
})

test('a delete while a turn of that bot is in progress is refused and removes nothing', async () => {
  const { profile, file, main, clean } = fixture([botRow({ slug: 'custodian' })])
  try {
    const folder = join(profile, 'bots', 'custodian')
    mkdirSync(folder, { recursive: true })
    writeFileSync(join(folder, 'history.json'), '{"at":1}')
    const before = structuredClone(file.rows)
    const calls = file.calls.length
    await assert.rejects(
      main.removeBot('custodian', true),
      (error) => error.message === 'Stop this bot’s running conversations before deleting it.',
    )
    assert.equal(file.calls.slice(calls).some(([method]) => method === 'bot.delete'), false)
    assert.deepEqual(file.rows, before)
    assert.equal(readFileSync(join(folder, 'history.json'), 'utf8'), '{"at":1}')
    assert.equal(existsSync(folder), true)
  } finally { clean() }
})

test('a delete while a turn is in progress is refused when the list cannot be read, and removes nothing', async () => {
  const { profile, file, main, clean } = fixture([botRow({ slug: 'custodian' })])
  try {
    const folder = join(profile, 'bots', 'custodian')
    mkdirSync(folder, { recursive: true })
    writeFileSync(join(folder, 'history.json'), '{"at":1}')
    const before = structuredClone(file.rows)
    main.useBotsBridge(async () => { throw new Error('the agent exited') })
    await assert.rejects(
      main.removeBot('custodian', true),
      (error) => error.message === 'Stop this bot’s running conversations before deleting it.',
    )
    assert.deepEqual(file.rows, before)
    assert.equal(readFileSync(join(folder, 'history.json'), 'utf8'), '{"at":1}')
    assert.equal(existsSync(folder), true)
  } finally { clean() }
})

test('a make, an edit and a model change the agent refuses are not shown as saved, and say why with the path', async () => {
  const { file, main, clean } = fixture([botRow({ slug: 'kept' })])
  try {
    file.refuse = '/home/someone/.bravebot/bots.json: the disk is full'
    const attempts = {
      make: () => main.saveForm({ name: 'Form bot', purpose: 'Fill in forms' }),
      edit: () => main.saveForm({ slug: 'kept', purpose: 'Changed' }),
      model: () => main.saveBotModel('kept', 'provider/model-a'),
    }
    for (const [name, attempt] of Object.entries(attempts)) {
      await assert.rejects(attempt(), /bots\.json: the disk is full/, name)
      const [kept, ...others] = await main.bots()
      assert.deepEqual([kept.slug, kept.purpose, kept.model, others.length], ['kept', 'Build websites', null, 0], `${name} changed nothing`)
      assert.match(main.botsProblem(), /\/home\/someone\/\.bravebot\/bots\.json: the disk is full/, `${name} is still reported after a read`)
    }
    file.refuse = null
    await main.saveForm({ slug: 'kept', purpose: 'Changed' })
    assert.equal(main.botsProblem(), null, 'a write that works clears it')
  } finally { clean() }
})

test('a make, an edit and a model change wait their turn behind the writes of the rows and lose none', async () => {
  const { file, main, clean } = fixture([botRow({ slug: 'review' })])
  try {
    main.useBotsBridge(async (method, params) => {
      await new Promise((resolve) => setImmediate(resolve))
      return file.request(method, params)
    })
    await Promise.all([
      main.noteBotSession('review', ids[0], '/project'),
      main.saveForm({ name: 'Second', purpose: 'Another bot' }),
      main.noteBotSession('review', ids[1], '/other'),
      main.saveForm({ slug: 'review', purpose: 'Changed' }),
      main.saveBotModel('review', 'provider/model-a'),
      main.noteBotArchived('review', 3),
      main.saveForm({ name: 'Third', purpose: 'Yet another' }),
    ])
    const review = file.rows.find((each) => each.slug === 'review')
    assert.deepEqual(review.conversations.map((each) => each.id).sort(), [...ids].sort())
    assert.equal(review.archived, 3)
    assert.equal(review.purpose, 'Changed')
    assert.equal(review.model, 'provider/model-a')
    assert.deepEqual(file.rows.map((each) => each.slug), ['review', 'second', 'third'])
  } finally { clean() }
})

test('archiving one bot and deleting another while a conversation is recorded keeps the conversation, the archive and the deletion', async () => {
  const { file, main, clean } = fixture([botRow({ slug: 'review' }), botRow({ slug: 'spare' })])
  try {
    main.useBotsBridge(async (method, params) => {
      await new Promise((resolve) => setImmediate(resolve))
      return file.request(method, params)
    })
    await Promise.all([
      main.retireBot('review', true),
      main.removeBot('spare', false),
      main.noteBotSession('review', ids[0], '/project'),
    ])
    const review = file.rows.find((each) => each.slug === 'review')
    assert.ok(review.conversations.some((each) => each.id === ids[0]))
    assert.notEqual(review.retired, 0)
    assert.equal(file.rows.some((each) => each.slug === 'spare'), false)
  } finally { clean() }
})

test('the main process no longer composes a bot, writes its definition or makes its home itself', () => {
  for (const path of ['src/main/bots.ts', 'src/main/index.ts']) {
    const source = readFileSync(path, 'utf8')
    for (const gone of ['botFromForm', 'saveFormBot', 'ensureHome(', 'bot.define', 'bot.redefine', 'slugFor', 'isBotModel']) {
      assert.equal(source.includes(gone), false, `${path} still has ${gone}`)
    }
  }
})

test('a refusal of what was asked for is thrown and is not kept as a problem with the bots file', async () => {
  const { file, main, clean } = fixture([botRow({ slug: 'kept' })])
  try {
    const refused = {
      'a blank name': () => main.saveForm({ name: ' ', purpose: 'Fill in forms' }),
      'a model that is not a string': () => main.saveForm({ slug: 'kept', model: 7 }),
      'a model change to one that is not a string': () => main.saveBotModel('kept', 7),
      'a model change of a bot the agent does not hold': () => main.saveBotModel('ghost', 'provider/model-a'),
      'a make with no name': () => main.saveForm({ purpose: 'Fill in forms' }),
    }
    for (const [name, attempt] of Object.entries(refused)) {
      await assert.rejects(attempt(), (error) => error.code === 'bad_request', name)
      assert.equal(main.botsProblem(), null, `${name} sets no problem`)
    }

    file.refuse = 'there are already 100 bots'
    file.refuseCode = 'bad_request'
    await assert.rejects(main.saveForm({ name: 'One too many', purpose: 'p' }), /already 100 bots/)
    assert.equal(main.botsProblem(), null, 'a ceiling refusal sets no problem')
  } finally { clean() }
})

test('a write of the bots file that fails keeps its problem with the path, whatever code the bridge gave', async () => {
  const { file, main, clean } = fixture([botRow({ slug: 'kept' })])
  try {
    for (const code of ['state_unusable', 'no_home', 'agent_gone', null]) {
      file.refuse = `/home/someone/.bravebot/bots.json: it failed (${code})`
      file.refuseCode = code
      await assert.rejects(main.saveForm({ name: 'Form bot', purpose: 'Fill in forms' }), /it failed/)
      assert.match(main.botsProblem(), new RegExp(`/home/someone/\\.bravebot/bots\\.json: it failed \\(${code}\\)`), `${code} is kept`)
      file.refuse = null
      await main.saveForm({ slug: 'kept', purpose: 'Changed' })
      assert.equal(main.botsProblem(), null, 'a write that works clears it')
    }
  } finally { clean() }
})

test('a refusal of what was asked for leaves the problem an earlier failed write set', async () => {
  const { file, main, clean } = fixture([botRow({ slug: 'kept' })])
  try {
    file.refuse = '/home/someone/.bravebot/bots.json: the disk is full'
    await assert.rejects(main.saveForm({ slug: 'kept', purpose: 'Changed' }), /disk is full/)
    file.refuse = null
    await assert.rejects(main.saveForm({ name: ' ', purpose: 'p' }), (error) => error.code === 'bad_request')
    assert.match(main.botsProblem(), /the disk is full/)
  } finally { clean() }
})

test('a field sent as null reaches the agent as sent for the name, the purpose and the avatar, and a null model is a request to clear it', async () => {
  const { file, main, clean } = fixture([botRow({ slug: 'kept', model: 'provider/model-a' })])
  try {
    for (const field of ['name', 'purpose', 'avatar']) {
      const before = structuredClone(file.rows)
      await assert.rejects(main.saveForm({ slug: 'kept', [field]: null }), (error) => error.code === 'bad_request', `edit ${field}`)
      const [, params] = file.calls.filter(([method]) => method === 'bot.edit').at(-1)
      assert.deepEqual(params, { slug: 'kept', dataFolder: params.dataFolder, [field]: null }, `${field} null was sent, not dropped`)
      assert.deepEqual(file.rows, before, `edit ${field} changed nothing`)
    }
    await assert.rejects(main.saveForm({ name: 'Form bot', purpose: 'Fill in forms', avatar: null }), (error) => error.code === 'bad_request')
    const [, made] = file.calls.filter(([method]) => method === 'bot.make').at(-1)
    assert.equal(made.avatar, null, 'a null avatar of a make was sent as null')
    assert.deepEqual(file.rows.map((each) => each.slug), ['kept'])

    const cleared = await main.saveForm({ slug: 'kept', model: null })
    assert.equal(cleared.model, null)
    assert.equal(main.botsProblem(), null)
  } finally { clean() }
})
