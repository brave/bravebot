// The helpers the bot and theme drivers share to write the real profile's `bravebot-ui.json`.
//
// The drivers need a display and the built app. These run the file helpers against a temporary
// directory and the page helpers against recording doubles, so what each driver relied on is
// pinned without launching Electron.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { chmodSync, existsSync, linkSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, symlinkSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { appBots, appState, forgetCheckoutSessions, openBot, scratchCheckout, settle } from './drive-profile.mjs'
import * as profiles from './drive-profile.mjs'

const temp = () => mkdtempSync(join(tmpdir(), 'drive-profile-test-'))

test('readState is an empty object when the file is missing or is not JSON', () => {
  const dir = temp()
  const { readState } = appState(join(dir, 'bravebot-ui.json'))
  assert.deepEqual(readState(), {})
  writeFileSync(join(dir, 'bravebot-ui.json'), 'not json')
  assert.deepEqual(readState(), {})
})

test('putKey sets one key and leaves every other key as it was found', () => {
  const dir = temp()
  const file = join(dir, 'bravebot-ui.json')
  writeFileSync(file, JSON.stringify({ theme: 'dark', bots: [{ slug: 'theirs' }] }))
  const { readState, putKey } = appState(file)
  putKey('view', { tab: 'sessions' })
  assert.deepEqual(readState(), { theme: 'dark', bots: [{ slug: 'theirs' }], view: { tab: 'sessions' } })
  const text = readFileSync(file, 'utf8')
  assert.ok(text.endsWith('}\n'))
  assert.equal(text, `${JSON.stringify(readState(), null, 2)}\n`)
})

test('putKey with undefined deletes the key, and not the others', () => {
  const dir = temp()
  const file = join(dir, 'bravebot-ui.json')
  writeFileSync(file, JSON.stringify({ theme: 'dark', view: { tab: 'bots' } }))
  const { readState, putKey } = appState(file)
  putKey('theme', undefined)
  assert.deepEqual(readState(), { view: { tab: 'bots' } })
})

test('putKey swallows a write the app has not made room for yet', () => {
  const dir = temp()
  const { putKey } = appState(join(dir, 'missing', 'bravebot-ui.json'))
  assert.doesNotThrow(() => putKey('theme', 'dark'))
})

/** A page whose locators answer from `visible`, and whose waits are counted instead of slept. */
function pageDouble(visible) {
  let waits = 0
  let polls = 0
  return {
    get waits() {
      return waits
    },
    get polls() {
      return polls
    },
    locator: (selector) => ({
      isVisible: async () => {
        polls++
        return visible(selector, polls)
      },
    }),
    waitForTimeout: async () => {
      waits++
    },
  }
}

test('settle answers unconfigured as soon as that screen is up', async () => {
  const page = pageDouble((selector) => selector === '.unconfigured')
  assert.equal(await settle(page), 'unconfigured')
  assert.equal(page.waits, 0)
})

test('settle waits out the first four seconds even when nothing is working', async () => {
  const page = pageDouble(() => false)
  assert.equal(await settle(page), 'done')
  assert.equal(page.waits, 4)
})

test('settle waits while a turn is working and reports a timeout when it never stops', async () => {
  const page = pageDouble((selector) => selector === '.working')
  assert.equal(await settle(page, 6), 'timeout')
  assert.equal(page.waits, 6)
})

test('scratchCheckout empties a checkout left by an earlier run and writes the README', () => {
  const name = `drive-profile-test-${process.pid}`
  const first = scratchCheckout(name)
  try {
    writeFileSync(join(first, 'left-over.txt'), 'x')
    const again = scratchCheckout(name)
    assert.equal(again, first)
    assert.equal(existsSync(join(again, 'left-over.txt')), false)
    assert.equal(readFileSync(join(again, 'README.md'), 'utf8'), '# a scratch checkout\n')
  } finally {
    rmSync(first, { recursive: true, force: true })
  }
})

test('forgetCheckoutSessions removes the mangled directory and no sibling', () => {
  const home = temp()
  const was = process.env.HOME
  process.env.HOME = home
  try {
    const checkout = '/tmp/some dir/a_b.c'
    const mine = join(home, '.bravebot', 'sessions', '-tmp-some-dir-a_b.c')
    const theirs = join(home, '.bravebot', 'sessions', '-tmp-some-dir-a_b.c-other')
    mkdirSync(mine, { recursive: true })
    mkdirSync(theirs, { recursive: true })
    forgetCheckoutSessions(checkout)
    assert.equal(existsSync(mine), false)
    assert.equal(existsSync(theirs), true)
  } finally {
    process.env.HOME = was
  }
})

test('openBot stubs the folder dialog, writes the bot, finds its row and picks the project', async () => {
  const calls = []
  const dialog = {}
  const app = {
    evaluate: async (fn, directory) => {
      await fn({ dialog }, directory)
      calls.push('dialog')
    },
  }
  const locator = (selector) => {
    const here = {
      filter: ({ has }) => {
        calls.push(`filter ${has.selector}`)
        return here
      },
      count: async () => 1,
      locator: (inner) => locator(inner),
      click: async () => calls.push(`click ${selector}`),
      waitFor: async () => calls.push(`waitFor ${selector}`),
      selector,
      nth: (n) => ({ click: async () => calls.push(`click ${selector} #${n}`) }),
    }
    return here
  }
  const page = {
    evaluate: async (fn, args) => {
      globalThis.window = { bravebot: { writeBot: (bot) => calls.push(`writeBot ${bot.name}: ${bot.purpose}`) } }
      await fn(args)
      delete globalThis.window
    },
    reload: async () => calls.push('reload'),
    waitForTimeout: async () => {},
    locator,
  }
  const checks = []
  await openBot(app, page, {
    name: 'Custodian',
    row: /^Custodian$/,
    purpose: 'keep it',
    checkout: '/tmp/checkout',
    check: (ok, what) => checks.push([ok, what]),
  })
  assert.deepEqual(await dialog.showOpenDialog(), { canceled: false, filePaths: ['/tmp/checkout'] })
  assert.deepEqual(checks, [[true, 'the bot is in the list']])
  const order = (name) => calls.findIndex((call) => call.startsWith(name))
  assert.ok(order('writeBot Custodian: keep it') < order('reload'))
  assert.ok(order('reload') < order('click .sidebar-tabs [role="option"] #1'))
  assert.ok(order('click .bot-open-button') < order('waitFor [data-test="bot-page"]'))
  assert.ok(order('waitFor [data-test="bot-page"]') < order('click [data-test="project-trigger"]'))
  assert.ok(order('click [data-test="project-trigger"]') < order('click [data-test="project-pick"]'))
})

test('openBot records a failed check when the bot is not in the list exactly once', async () => {
  const none = { filter: () => none, count: async () => 0, locator: () => none, click: async () => {}, waitFor: async () => {}, nth: () => none }
  const page = {
    evaluate: async () => {},
    reload: async () => {},
    waitForTimeout: async () => {},
    locator: () => none,
  }
  const app = { evaluate: async () => {} }
  const checks = []
  await openBot(app, page, { name: 'X', row: /^X$/, purpose: 'p', checkout: '/c', check: (ok, what) => checks.push([ok, what]) })
  assert.deepEqual(checks, [[false, 'the bot is in the list']])
})

function withBravebotHome(value, body) {
  const before = process.env.BRAVEBOT_HOME
  if (value === undefined) delete process.env.BRAVEBOT_HOME
  else process.env.BRAVEBOT_HOME = value
  try { return body() } finally {
    if (before === undefined) delete process.env.BRAVEBOT_HOME
    else process.env.BRAVEBOT_HOME = before
  }
}

test('the bots helper refuses to act on the real state directory', () => {
  withBravebotHome(undefined, () => assert.throws(() => appBots(temp()), /BRAVEBOT_HOME/))
})

test('readBots is none for a missing file and refuses a file that exists and does not parse', () => {
  const dir = temp()
  withBravebotHome(dir, () => {
    const { readBots } = appBots(temp())
    assert.deepEqual(readBots(), [])
    writeFileSync(join(dir, 'bots.json'), '{"bots": [')
    assert.throws(() => readBots(), /bots\.json/)
  })
})

test('putBots writes through a file renamed into place, so a reader never sees part of a write', () => {
  const dir = temp()
  withBravebotHome(dir, () => {
    const file = join(dir, 'bots.json')
    writeFileSync(file, '{"bots": [{"slug": "theirs"}]}\n')
    linkSync(file, join(dir, 'held-open'))
    const { readBots, putBots } = appBots(temp())
    putBots([{ slug: 'mine' }])
    assert.deepEqual(readBots(), [{ slug: 'mine' }])
    assert.equal(readFileSync(join(dir, 'held-open'), 'utf8'), '{"bots": [{"slug": "theirs"}]}\n', 'the old file was replaced and not rewritten in place')
    assert.deepEqual(readdirSync(dir).sort(), ['bots.json', 'held-open'], 'no temporary file is left')
  })
})

test('the bots helper needs the user-data directory the driver launches with', () => {
  withBravebotHome(temp(), () => assert.throws(() => appBots(), /user-data/))
})

test('the bots helper refuses a user-data directory whose bravebot-ui.json holds bots the agent has not moved', () => {
  const userData = temp()
  writeFileSync(join(userData, 'bravebot-ui.json'), JSON.stringify({ bots: [{ slug: 'theirs' }] }))
  withBravebotHome(temp(), () => assert.throws(() => appBots(userData), /bravebot-ui\.json/))
})

test('the bots helper accepts a user-data directory with nothing waiting to be moved', () => {
  const state = temp()
  const moved = temp()
  const empty = temp()
  const keyless = temp()
  const unparseable = temp()
  writeFileSync(join(state, 'bots.json'), '{"bots": []}\n')
  writeFileSync(join(moved, 'bravebot-ui.json'), JSON.stringify({ bots: [{ slug: 'moved' }] }))
  writeFileSync(join(empty, 'bravebot-ui.json'), JSON.stringify({ bots: [] }))
  writeFileSync(join(keyless, 'bravebot-ui.json'), JSON.stringify({ theme: 'dark' }))
  writeFileSync(join(unparseable, 'bravebot-ui.json'), '{"bots": [')
  withBravebotHome(state, () => assert.doesNotThrow(() => appBots(moved), 'bots.json exists, so the old key has been moved'))
  for (const userData of [empty, keyless, unparseable, temp()]) {
    withBravebotHome(temp(), () => assert.doesNotThrow(() => appBots(userData)))
  }
})

test('a scratch profile is a user-data directory and a state directory of its own, and removing it removes both', () => {
  withBravebotHome(undefined, () => {
    const profile = profiles.scratchProfile('drive-profile-test')
    try {
      assert.ok(profile.userData.startsWith(tmpdir()), 'under the temporary directory, never the real profile')
      assert.equal(profile.stateFile, join(profile.userData, 'bravebot-ui.json'))
      assert.ok(existsSync(profile.userData))
      const state = process.env.BRAVEBOT_HOME
      assert.ok(state && state.startsWith(tmpdir()) && existsSync(state), 'the app and its agent are told to keep their state there')
      assert.notEqual(join(state, '..'), tmpdir(), 'inside a directory of its own')
      assert.doesNotThrow(() => appBots(profile.userData), 'the bots helper accepts it')
      profile.remove()
      assert.equal(existsSync(profile.userData), false)
      assert.equal(existsSync(state), false)
    } finally {
      profile.remove()
    }
  })
})

test('two scratch profiles never share a directory', () => {
  withBravebotHome(undefined, () => {
    const first = profiles.scratchProfile('drive-profile-test')
    const second = profiles.scratchProfile('drive-profile-test')
    try {
      assert.notEqual(first.userData, second.userData)
    } finally {
      first.remove()
      second.remove()
    }
  })
})

function realState(files) {
  const dir = temp()
  for (const [name, text] of Object.entries(files)) {
    mkdirSync(dirname(join(dir, name)), { recursive: true })
    writeFileSync(join(dir, name), text)
  }
  return dir
}

const modeOf = (path) => statSync(path).mode & 0o777
const unix = process.platform === 'win32' ? { skip: 'file modes are a Unix notion' } : {}

test('a live profile copies the settings, the recorded model and the subscription, and nothing else the real directory holds', () => {
  const real = realState({
    'settings.json': '{"env": {"A": "1"}}',
    model: 'a-model',
    'leo-premium.json': '{"token": "t"}',
    history: 'typed words',
    theme: 'dark',
    'bots.json': '{"bots": []}',
    'AGENTS.md': 'standing instructions',
    'sessions/one/record.json': '{}',
    'agents/mine.md': 'a definition',
    'untrusted/memory.json': '{}',
    'leo-premium.lock': '',
  })
  withBravebotHome(undefined, () => {
    const profile = profiles.scratchProfile('drive-profile-test', { live: true, realState: real })
    try {
      assert.deepEqual(readdirSync(process.env.BRAVEBOT_HOME).sort(), ['leo-premium.json', 'model', 'settings.json'])
      assert.deepEqual(profile.seeded.sort(), ['leo-premium.json', 'model', 'settings.json'])
    } finally {
      profile.remove()
    }
  })
})

test('the copies are byte for byte the originals and readable by their owner alone', unix, () => {
  const real = realState({ 'settings.json': '{"env": {"A": "1"}}', model: 'a-model', 'leo-premium.json': '{"token": "t"}' })
  for (const name of ['settings.json', 'model']) chmodSync(join(real, name), 0o644)
  withBravebotHome(undefined, () => {
    const profile = profiles.scratchProfile('drive-profile-test', { live: true, realState: real })
    try {
      for (const name of ['settings.json', 'model', 'leo-premium.json']) {
        const copy = join(process.env.BRAVEBOT_HOME, name)
        assert.deepEqual(readFileSync(copy), readFileSync(join(real, name)), name)
        assert.equal(modeOf(copy), 0o600, name)
      }
    } finally {
      profile.remove()
    }
  })
})

test('seeding never changes the real files or their modes', unix, () => {
  const real = realState({ 'settings.json': '{"env": {"A": "1"}}', model: 'a-model', 'leo-premium.json': '{"token": "t"}', history: 'h' })
  chmodSync(join(real, 'settings.json'), 0o644)
  const names = ['settings.json', 'model', 'leo-premium.json', 'history']
  const before = names.map((name) => [readFileSync(join(real, name)), modeOf(join(real, name)), statSync(join(real, name)).mtimeMs])
  withBravebotHome(undefined, () => {
    const profile = profiles.scratchProfile('drive-profile-test', { live: true, realState: real })
    profile.remove()
  })
  const after = names.map((name) => [readFileSync(join(real, name)), modeOf(join(real, name)), statSync(join(real, name)).mtimeMs])
  assert.deepEqual(after, before)
  assert.deepEqual(readdirSync(real).sort(), ['history', 'leo-premium.json', 'model', 'settings.json'])
})

test('a file the real directory does not have is skipped, and so is a real directory that is not there', () => {
  withBravebotHome(undefined, () => {
    const some = profiles.scratchProfile('drive-profile-test', { live: true, realState: realState({ 'settings.json': '{}' }) })
    try {
      assert.deepEqual(readdirSync(process.env.BRAVEBOT_HOME), ['settings.json'])
      assert.deepEqual(some.seeded, ['settings.json'])
    } finally {
      some.remove()
    }
    const none = profiles.scratchProfile('drive-profile-test', { live: true, realState: join(temp(), 'absent') })
    try {
      assert.deepEqual(readdirSync(process.env.BRAVEBOT_HOME), [])
      assert.deepEqual(none.seeded, [])
    } finally {
      none.remove()
    }
  })
})

test('a file that is a link to another file is copied as that file, not as a link', unix, () => {
  const real = realState({ 'elsewhere.json': '{"env": {"A": "1"}}' })
  symlinkSync(join(real, 'elsewhere.json'), join(real, 'settings.json'))
  withBravebotHome(undefined, () => {
    const profile = profiles.scratchProfile('drive-profile-test', { live: true, realState: real })
    try {
      const copy = join(process.env.BRAVEBOT_HOME, 'settings.json')
      assert.equal(readFileSync(copy, 'utf8'), '{"env": {"A": "1"}}')
      assert.equal(statSync(copy).isFile(), true)
      assert.equal(readdirSync(process.env.BRAVEBOT_HOME).length, 1)
    } finally {
      profile.remove()
    }
  })
})

test('a profile that is not live copies nothing, whatever the real directory holds', () => {
  const real = realState({ 'settings.json': '{}', model: 'a-model', 'leo-premium.json': '{}' })
  withBravebotHome(undefined, () => {
    for (const options of [undefined, { realState: real }, { live: false, realState: real }]) {
      const profile = profiles.scratchProfile('drive-profile-test', options)
      try {
        assert.deepEqual(readdirSync(process.env.BRAVEBOT_HOME), [])
        assert.deepEqual(profile.seeded, [])
      } finally {
        profile.remove()
      }
    }
  })
})

test('removing a live profile removes its copies with it', () => {
  const real = realState({ 'settings.json': '{}', model: 'a-model', 'leo-premium.json': '{}' })
  withBravebotHome(undefined, () => {
    const profile = profiles.scratchProfile('drive-profile-test', { live: true, realState: real })
    const root = dirname(profile.userData)
    assert.equal(existsSync(join(process.env.BRAVEBOT_HOME, 'leo-premium.json')), true)
    profile.remove()
    assert.equal(existsSync(root), false)
    assert.doesNotThrow(() => profile.remove(), 'removing twice is not an error')
  })
})

test('the real state directory is the one the agent would use', () => {
  const absolute = join(tmpdir(), 'named-state')
  assert.equal(profiles.realStateDirectory({ BRAVEBOT_HOME: absolute, HOME: '/home/someone' }), absolute)
  assert.equal(profiles.realStateDirectory({ HOME: '/home/someone' }), join('/home/someone', '.bravebot'))
  for (const passedOver of ['', 'relative/dir']) {
    assert.equal(profiles.realStateDirectory({ BRAVEBOT_HOME: passedOver, HOME: '/home/someone' }), join('/home/someone', '.bravebot'), JSON.stringify(passedOver))
  }
  assert.equal(profiles.realStateDirectory({}), null)
  assert.equal(profiles.realStateDirectory({ HOME: '' }), null)
})

/** A program that makes a live scratch profile, says where, and then ends the way `ending` says. */
function profileInAnotherProcess(real, ending) {
  const scripts = temp()
  const script = join(scripts, 'profile.mjs')
  const module = new URL('./drive-profile.mjs', import.meta.url).href
  writeFileSync(
    script,
    [
      `import { scratchProfile } from ${JSON.stringify(module)}`,
      `const profile = scratchProfile('drive-profile-child-test', { live: true, realState: ${JSON.stringify(real)} })`,
      'console.log(profile.userData)',
      ending,
    ].join('\n'),
  )
  const child = spawn(process.execPath, [script], { stdio: ['ignore', 'pipe', 'inherit'], env: { PATH: process.env.PATH } })
  let said = ''
  const where = new Promise((resolve) => {
    child.stdout.on('data', (chunk) => {
      said += chunk
      if (said.includes('\n')) resolve(dirname(said.split('\n')[0]))
    })
  })
  const ended = new Promise((resolve) => child.on('close', (code, signal) => resolve({ code, signal })))
  return { child, where, ended }
}

const bounded = { timeout: 20_000 }

async function withChild(real, ending, body) {
  const run = profileInAnotherProcess(real, ending)
  try {
    await body(run)
  } finally {
    if (run.child.exitCode === null && run.child.signalCode === null) run.child.kill('SIGKILL')
  }
}

test('a live profile is removed when the program ends without removing it', bounded, async () => {
  const real = realState({ 'leo-premium.json': '{"token": "t"}' })
  await withChild(real, '', async (run) => {
    const root = await run.where
    await run.ended
    assert.equal(existsSync(root), false)
  })
})

test('a live profile is removed when the program ends on an uncaught error', bounded, async () => {
  const real = realState({ 'leo-premium.json': '{"token": "t"}' })
  await withChild(real, "throw new Error('a driver failed')", async (run) => {
    const root = await run.where
    const { code } = await run.ended
    assert.notEqual(code, 0)
    assert.equal(existsSync(root), false)
  })
})

test('a live profile is removed when the program is stopped by a signal', { ...unix, ...bounded }, async () => {
  for (const signal of ['SIGINT', 'SIGTERM', 'SIGHUP']) {
    const real = realState({ 'leo-premium.json': '{"token": "t"}' })
    await withChild(real, 'setInterval(() => {}, 1000)', async (run) => {
      const root = await run.where
      assert.equal(existsSync(join(root, 'state', 'leo-premium.json')), true, `${signal}: copied before the signal`)
      run.child.kill(signal)
      const { code, signal: ended } = await run.ended
      assert.ok(code !== 0 || ended !== null, `${signal}: the program still stops`)
      assert.equal(existsSync(root), false, signal)
    })
  }
})
