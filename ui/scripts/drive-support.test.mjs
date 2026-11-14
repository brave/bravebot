// The setup the live-profile drivers share: the check line, the session list, the live launch.
//
// The drivers themselves need a display and a real profile. These run the helpers against
// recording doubles for the page and the app, so what each driver relied on is pinned without
// launching Electron.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import {
  ASSISTANT_BUBBLE,
  BOT_GREETING,
  BOT_OPEN_BUTTON,
  COMPOSER_IDLE,
  COMPOSER_INPUT,
  COMPOSER_NAME,
  COMPOSER_SEND,
  SIDEBAR_TAB,
  TRUST_APPROVE,
  approveTrust,
  approveTrustIfAsked,
  checker,
  launchArguments,
  launchLive,
  openFirstSession,
  readTrust,
  sessionsOrSkip,
  showSessions,
  trustQuestion,
} from './drive-support.mjs'

// A page that records what it was asked, in order, and answers every wait and click with the
// outcome the test gives it.
function pageDouble({ waitFails = false, clickFails = false } = {}) {
  const calls = []
  const listeners = {}
  const locator = (selector) => ({
    first: () => ({
      waitFor: async (options) => {
        calls.push(['waitFor', selector, options])
        if (waitFails) throw new Error('not visible')
      },
      click: async (options) => {
        calls.push(['click', selector, options])
        if (clickFails) throw new Error('not clickable')
      },
    }),
  })
  return {
    calls,
    listeners,
    locator,
    waitForLoadState: async (state) => calls.push(['waitForLoadState', state]),
    waitForTimeout: async (ms) => calls.push(['waitForTimeout', ms]),
    on: (event, handler) => {
      listeners[event] = handler
    },
  }
}

function captureLog(run) {
  const lines = []
  const original = console.log
  console.log = (...args) => lines.push(args.join(' '))
  const finish = () => {
    console.log = original
    return lines
  }
  return Promise.resolve(run(lines)).then(
    (value) => ({ value, lines: finish() }),
    (error) => {
      finish()
      throw error
    },
  )
}

test('checker prints ok or FAIL for each check and collects only the failures', async () => {
  const { value, lines } = await captureLog(() => {
    const { check, problems } = checker()
    check(true, 'it passes')
    check(false, 'it fails')
    return problems
  })
  assert.deepEqual(lines, ['  ok   it passes', ' FAIL  it fails'])
  assert.deepEqual(value, ['it fails'])
})

test('each checker keeps its own list of problems', () => {
  const first = checker()
  const second = checker()
  const original = console.log
  console.log = () => undefined
  try {
    first.check(false, 'only in the first')
  } finally {
    console.log = original
  }
  assert.deepEqual(first.problems, ['only in the first'])
  assert.deepEqual(second.problems, [])
})

test('showSessions waits for the first tab, clicks it, then lets the list settle', async () => {
  const page = pageDouble()
  await showSessions(page)
  const tab = '.sidebar-tabs [role="option"]'
  assert.deepEqual(page.calls, [
    ['waitFor', tab, { state: 'visible', timeout: 15000 }],
    ['click', tab, { timeout: 3000 }],
    ['waitForTimeout', 250],
  ])
})

test('showSessions carries on when the tab never shows or cannot be clicked', async () => {
  const page = pageDouble({ waitFails: true, clickFails: true })
  await showSessions(page)
  assert.deepEqual(
    page.calls.map(([name]) => name),
    ['waitFor', 'click', 'waitForTimeout'],
  )
})

test('launchLive returns the app and its first window with the list shown and errors logged', async () => {
  const page = pageDouble()
  const app = { firstWindow: async () => page }
  const { value, lines } = await captureLog(async () => {
    const live = await launchLive(async () => app)
    page.listeners.pageerror({ message: 'boom' })
    page.listeners.console({ type: () => 'error', text: () => 'bad' })
    page.listeners.console({ type: () => 'log', text: () => 'fine' })
    return live
  })
  assert.equal(value.app, app)
  assert.equal(value.page, page)
  assert.deepEqual(
    page.calls.map(([name]) => name),
    ['waitForLoadState', 'waitFor', 'click', 'waitForTimeout'],
  )
  assert.deepEqual(page.calls[0], ['waitForLoadState', 'domcontentloaded'])
  assert.deepEqual(lines, ['PAGE ERROR: boom', 'CONSOLE ERROR: bad'])
})

test('the selector constants name the classes the renderer owns', () => {
  const read = (file) => readFileSync(join(dirname(fileURLToPath(import.meta.url)), '..', 'src', 'renderer', 'components', file), 'utf8')
  assert.match(read('Composer.tsx'), /<footer className="composer"[ >]/)
  assert.match(read('Composer.tsx'), /querySelector\('textarea'\)/)
  assert.match(read('Sidebar.tsx'), /className="sidebar-tabs"/)
  assert.equal(COMPOSER_INPUT, '.composer textarea')
  assert.equal(COMPOSER_IDLE, `${COMPOSER_INPUT}:not([disabled])`)
  assert.equal(SIDEBAR_TAB, '.sidebar-tabs [role="option"]')
})

test('the send, name, reply, bot and trust constants name what the renderer draws', () => {
  const read = (file) => readFileSync(join(dirname(fileURLToPath(import.meta.url)), '..', 'src', 'renderer', 'components', file), 'utf8')
  // `.composer .send`: the footer carries `composer` (pinned above) and the button carries `send`.
  assert.match(read('Composer.tsx'), /className=\{running \? 'send stop' : 'send'\}/)
  assert.match(read('Composer.tsx'), /aria-label="Message the agent"/)
  assert.match(read('Transcript.tsx'), /<div className="bubble assistant">/)
  assert.match(read('Bots.tsx'), /<button type="button" className="bot-open-button"/)
  assert.match(read('Transcript.tsx'), /data-test="bot-greeting"/)
  assert.match(read('TrustPrompt.tsx'), /data-test="trust-approve"/)
  assert.equal(COMPOSER_SEND, '.composer .send')
  assert.equal(COMPOSER_NAME, 'Message the agent')
  assert.equal(ASSISTANT_BUBBLE, '.bubble.assistant')
  assert.equal(BOT_OPEN_BUTTON, '.bot-open-button')
  assert.equal(BOT_GREETING, '[data-test="bot-greeting"]')
  assert.equal(TRUST_APPROVE, '[data-test="trust-approve"]')
})

// The trust question's yes is held shut until the question has been read, so a driver that means to
// press it reads first. These pin the order and the names, against a question that records its calls.
function questionDouble() {
  const calls = []
  const question = {
    evaluate: async () => { calls.push(['evaluate']); return { outer: 1, inner: 1 } },
    locator: (selector) => ({ click: async () => calls.push(['click', selector]) }),
  }
  return { question, calls }
}

test('trustQuestion names the dialog the renderer titles Project trust', () => {
  const asked = []
  const page = { getByRole: (role, options) => { asked.push([role, options]); return 'dialog' } }
  assert.equal(trustQuestion(page), 'dialog')
  assert.deepEqual(asked, [['dialog', { name: 'Project trust', exact: true }]])
  assert.match(readFileSync(join(dirname(fileURLToPath(import.meta.url)), '..', 'src', 'renderer', 'components', 'TrustPrompt.tsx'), 'utf8'), /title="Project trust"/)
})

test('readTrust reports how many scrolling boxes it went through', async () => {
  const { question, calls } = questionDouble()
  assert.deepEqual(await readTrust(question), { outer: 1, inner: 1 })
  assert.deepEqual(calls, [['evaluate']])
})

test('approveTrust reads the question through before it presses the yes', async () => {
  const { question, calls } = questionDouble()
  await approveTrust(question)
  assert.deepEqual(calls, [['evaluate'], ['click', '[data-test="trust-approve"]']])
})

// A page showing `sessions` rows and a trust question when `trust` is set, recording each call in
// order. `trustProbeFails` makes the `.trust` visibility check throw, as it does when the page
// closes under it. `exits` records `process.exit`, which the test replaces for the run.
function sessionPage({ sessions = 1, trust = false, trustProbeFails = false } = {}) {
  const calls = []
  const { question, calls: asked } = questionDouble()
  const page = {
    locator: (selector) => ({
      count: async () => { calls.push(['count', selector]); return sessions },
      first: () => ({ click: async () => calls.push(['click', selector]) }),
      isVisible: async () => {
        calls.push(['isVisible', selector])
        if (trustProbeFails) throw new Error('closed')
        return trust
      },
    }),
    getByRole: () => question,
    waitForTimeout: async (ms) => calls.push(['waitForTimeout', ms]),
  }
  const app = { close: async () => calls.push(['close']) }
  return { page, app, calls, asked }
}

async function withExitStubbed(run) {
  const original = process.exit
  const exits = []
  process.exit = (code) => {
    exits.push(code)
    throw new Error('exit')
  }
  try {
    const result = await captureLog(() => run().then(() => undefined, (error) => error.message))
    return { exits, ...result }
  } finally {
    process.exit = original
  }
}

test('with no session a driver prints the skip, closes the app and exits 0 without clicking', async () => {
  const { page, app, calls } = sessionPage({ sessions: 0 })
  const { exits, lines, value } = await withExitStubbed(() => openFirstSession(app, page))
  assert.equal(value, 'exit')
  assert.deepEqual(exits, [0])
  assert.deepEqual(lines, ['RESULT: skipped — no sessions to open'])
  assert.deepEqual(calls, [['count', '.session'], ['close']])
})

test('sessionsOrSkip returns the count and leaves the app open when there are sessions', async () => {
  const { page, app, calls } = sessionPage({ sessions: 3 })
  const { exits, value } = await withExitStubbed(async () => assert.equal(await sessionsOrSkip(app, page), 3))
  assert.equal(value, undefined)
  assert.deepEqual(exits, [])
  assert.deepEqual(calls, [['count', '.session']])
})

test('the first session is clicked, waited for 1500 ms, and no trust is approved when none is asked', async () => {
  const { page, app, calls, asked } = sessionPage({ sessions: 2 })
  await openFirstSession(app, page)
  assert.deepEqual(calls, [
    ['count', '.session'],
    ['click', '.session'],
    ['waitForTimeout', 1500],
    ['isVisible', '.trust'],
  ])
  assert.deepEqual(asked, [])
})

test('a visible trust question is read, approved and waited for 600 ms after the session settles', async () => {
  const { page, app, calls, asked } = sessionPage({ trust: true })
  await openFirstSession(app, page, { settle: 1600 })
  assert.deepEqual(calls, [
    ['count', '.session'],
    ['click', '.session'],
    ['waitForTimeout', 1600],
    ['isVisible', '.trust'],
    ['waitForTimeout', 600],
  ])
  assert.deepEqual(asked, [['evaluate'], ['click', '[data-test="trust-approve"]']])
})

test('a trust check that throws counts as no question, and approveTrustIfAsked takes its own wait', async () => {
  const failing = sessionPage({ trustProbeFails: true })
  await approveTrustIfAsked(failing.page, 500)
  assert.deepEqual(failing.asked, [])
  assert.deepEqual(failing.calls, [['isVisible', '.trust']])
  const asking = sessionPage({ trust: true })
  await approveTrustIfAsked(asking.page, 500)
  assert.deepEqual(asking.asked, [['evaluate'], ['click', '[data-test="trust-approve"]']])
  assert.deepEqual(asking.calls.at(-1), ['waitForTimeout', 500])
})

test('a launch names the user-data directory it is given, and none otherwise', () => {
  assert.deepEqual(launchArguments('/tmp/scratch/user-data'), ['.', '--user-data-dir=/tmp/scratch/user-data'])
  assert.deepEqual(launchArguments(), ['.'])
})
