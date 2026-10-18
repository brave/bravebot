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
import { COMPOSER_IDLE, COMPOSER_INPUT, SIDEBAR_TAB, checker, launchLive, showSessions } from './drive-support.mjs'

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
  assert.match(read('Composer.tsx'), /<footer className="composer">/)
  assert.match(read('Composer.tsx'), /querySelector\('textarea'\)/)
  assert.match(read('Sidebar.tsx'), /className="sidebar-tabs"/)
  assert.equal(COMPOSER_INPUT, '.composer textarea')
  assert.equal(COMPOSER_IDLE, `${COMPOSER_INPUT}:not([disabled])`)
  assert.equal(SIDEBAR_TAB, '.sidebar-tabs [role="option"]')
})
