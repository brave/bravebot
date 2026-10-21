// The helpers the bot and theme drivers share to write the real profile's `bravebot-ui.json`.
//
// The drivers need a display and the built app. These run the file helpers against a temporary
// directory and the page helpers against recording doubles, so what each driver relied on is
// pinned without launching Electron.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { appState, forgetCheckoutSessions, openBot, scratchCheckout, settle } from './drive-profile.mjs'

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
