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
  APPROVE_BUTTON,
  ASSISTANT_BUBBLE,
  BOT_GREETING,
  BOT_OPEN_BUTTON,
  COMPOSER_IDLE,
  COMPOSER_INPUT,
  COMPOSER_NAME,
  COMPOSER_SEND,
  CONFIRM_BUTTON,
  REJECT_BUTTON,
  SIDEBAR_TAB,
  TRUST_APPROVE,
  approveTrust,
  approveTrustIfAsked,
  cancelDrag,
  checker,
  copyImage,
  dragFiles,
  heldTurns,
  launchArguments,
  launchLive,
  menuPaste,
  openFirstSession,
  picture,
  readTrust,
  sessionsOrSkip,
  showSessions,
  trustQuestion,
  untilIn,
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

test('the confirm card constants name the row and the buttons the renderer draws', () => {
  const read = (file) => readFileSync(join(dirname(fileURLToPath(import.meta.url)), '..', 'src', 'renderer', 'components', file), 'utf8')
  // `Answers` draws the `confirm-actions` row by default, and the question card puts its buttons in one.
  assert.match(read('Approval.tsx'), /className = 'confirm-actions'/)
  assert.match(read('Approval.tsx'), /className = 'approve'/)
  assert.match(read('Transcript.tsx'), /<div className="confirm-actions">\s[\s\S]*?className="reject"[\s\S]*?className="approve"/)
  assert.equal(CONFIRM_BUTTON, '.confirm-actions leo-button')
  assert.equal(APPROVE_BUTTON, '.confirm-actions .approve')
  assert.equal(REJECT_BUTTON, '.confirm-actions .reject')
  // A variant is the constant with a filter appended, so the filter shares the row's prefix.
  assert.equal(`${APPROVE_BUTTON}:not(.always)`, '.confirm-actions .approve:not(.always)')
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

// Whether a promise has settled, read without waiting on it.
const settled = (promise) => Promise.race([promise.then(() => true), new Promise((resolve) => setImmediate(resolve, false))])

test('a held turn stays pending and listed until release, which settles every one held and empties the list', async () => {
  const { hold, release, waiting } = heldTurns()
  assert.equal(waiting.length, 0)
  const first = hold()
  const second = hold()
  assert.equal(waiting.length, 2)
  assert.equal(await settled(first), false)
  assert.equal(await settled(second), false)
  release()
  assert.equal(waiting.length, 0)
  assert.equal(await settled(first), true)
  assert.equal(await settled(second), true)
  const third = hold()
  assert.equal(waiting.length, 1, 'a turn held after a release is listed again')
  assert.equal(await settled(third), false)
  release()
  assert.equal(await settled(third), true)
})

test('release with nothing held does nothing, and each heldTurns keeps its own list', async () => {
  const one = heldTurns()
  const other = heldTurns()
  one.release()
  const held = one.hold()
  other.release()
  assert.equal(one.waiting.length, 1)
  assert.equal(other.waiting.length, 0)
  assert.equal(await settled(held), false)
  one.release()
  assert.equal(await settled(held), true)
})

// A page that answers each wait by recording its length.
function pollingPage() {
  const waits = []
  return { waits, waitForTimeout: async (ms) => waits.push(ms) }
}

test('until returns as soon as the check is true, waiting 50 ms between checks and not after the last', async () => {
  const page = pollingPage()
  let checks = 0
  await untilIn(page)(async () => ++checks === 4, 'never')
  assert.equal(checks, 4)
  assert.deepEqual(page.waits, [50, 50, 50])
  const ready = pollingPage()
  await untilIn(ready)(async () => true, 'never')
  assert.deepEqual(ready.waits, [])
})

test('until fails with what it waited for after 200 checks, and after the number of tries it is given', async () => {
  const page = pollingPage()
  let checks = 0
  await assert.rejects(untilIn(page)(async () => { checks++; return false }, 'the held turn reached the model'), { message: 'the held turn reached the model' })
  assert.equal(checks, 200)
  assert.equal(page.waits.length, 200)
  checks = 0
  await assert.rejects(untilIn(page)(async () => { checks++; return false }, 'again', 3), { message: 'again' })
  assert.equal(checks, 3)
  checks = 0
  await assert.rejects(untilIn(page, 400)(async () => { checks++; return false }, 'slow'), { message: 'slow' })
  assert.equal(checks, 400)
})

test('until succeeds on the last check it allows', async () => {
  const page = pollingPage()
  let checks = 0
  await untilIn(page, 400)(async () => ++checks === 400, 'never')
  assert.equal(checks, 400)
})

// A page and a CDP session that record every call in one list, in order, so the order of a drag
// is what is compared. `sessions` counts the sessions the page was asked for.
function dragPage(box = { x: 10, y: 20, width: 101, height: 51 }) {
  const calls = []
  const state = { sessions: 0 }
  const cdp = { send: async (method, params) => calls.push(['send', method, params.type, params.x, params.y, params.data]) }
  const page = {
    context: () => ({ newCDPSession: async (given) => { assert.equal(given, page); state.sessions += 1; return cdp } }),
    locator: (selector) => ({ first: () => ({ boundingBox: async () => { calls.push(['box', selector]); return box } }) }),
    waitForTimeout: async (ms) => calls.push(['wait', ms]),
  }
  return { page, calls, state }
}

test('dragFiles enters, hovers three times 100 ms apart over the middle of the composer, then lets go', async () => {
  const { page, calls } = dragPage()
  await dragFiles(page, ['/tmp/a.png'])
  const data = { items: [], files: ['/tmp/a.png'], dragOperationsMask: 1 }
  // The middle of the box is rounded, not floored: 10 + 101 / 2 is 60.5, and 20 + 51 / 2 is 45.5.
  const at = (type) => ['send', 'Input.dispatchDragEvent', type, 61, 46, data]
  assert.deepEqual(calls, [
    ['box', '.composer textarea'],
    at('dragEnter'),
    at('dragOver'), ['wait', 100],
    at('dragOver'), ['wait', 100],
    at('dragOver'), ['wait', 100],
    at('drop'),
  ])
})

test('dragFiles aims at the selector it is given, and a drag told not to drop stays over the target', async () => {
  const { page, calls } = dragPage({ x: 0, y: 0, width: 200, height: 100 })
  await dragFiles(page, ['/tmp/a.png', '/tmp/b.pdf'], { selector: '.entries', drop: false })
  assert.deepEqual(calls[0], ['box', '.entries'])
  const types = calls.filter(([kind]) => kind === 'send').map((call) => call[2])
  assert.deepEqual(types, ['dragEnter', 'dragOver', 'dragOver', 'dragOver'], 'nothing is dropped')
  for (const call of calls.filter(([kind]) => kind === 'send')) {
    assert.deepEqual([call[3], call[4]], [100, 50])
    assert.deepEqual(call[5].files, ['/tmp/a.png', '/tmp/b.pdf'])
  }
})

test('a drag and its cancel go through one CDP session per page, and another page has its own', async () => {
  const first = dragPage()
  await dragFiles(first.page, ['/tmp/a.png'], { drop: false })
  await dragFiles(first.page, ['/tmp/a.png'])
  await cancelDrag(first.page, ['/tmp/a.png'])
  assert.equal(first.state.sessions, 1, 'a second session would not know the drag the first one began')
  const second = dragPage()
  await dragFiles(second.page, ['/tmp/b.png'])
  assert.equal(second.state.sessions, 1)
})

test('cancelDrag sends dragCancel at the origin with the files of the drag, and rejects when the browser does', async () => {
  const { page, calls } = dragPage()
  await cancelDrag(page, ['/tmp/a.png'])
  assert.deepEqual(calls, [['send', 'Input.dispatchDragEvent', 'dragCancel', 0, 0, { items: [], files: ['/tmp/a.png'], dragOperationsMask: 1 }]])
  const failing = {
    context: () => ({ newCDPSession: async () => ({ send: async () => { throw new Error('no drag') } }) }),
  }
  await assert.rejects(cancelDrag(failing, []), /no drag/)
})

// An app that runs the function it is given against the fake parts of Electron the helper names.
function electronApp(parts) {
  return { evaluate: async (fn, argument) => fn(parts, argument) }
}

test('picture draws every pixel in the colour it is given, and noise differs from pixel to pixel', async () => {
  const sizes = []
  const nativeImage = { createFromBitmap: (pixels, size) => { sizes.push(size); return { toPNG: () => pixels } } }
  const solid = Buffer.from(await picture(electronApp({ nativeImage }), 4, [0x20, 0x60, 0xd0, 0xff]), 'base64')
  assert.deepEqual(sizes, [{ width: 4, height: 4 }])
  assert.equal(solid.length, 4 * 4 * 4)
  for (let at = 0; at < solid.length; at += 4) assert.deepEqual([...solid.subarray(at, at + 4)], [0x20, 0x60, 0xd0, 0xff])
  const noisy = Buffer.from(await picture(electronApp({ nativeImage }), 16, null, true), 'base64')
  const pixels = new Set()
  for (let at = 0; at < noisy.length; at += 4) {
    assert.equal(noisy[at + 3], 0xff, 'noise stays opaque')
    pixels.add(noisy.subarray(at, at + 3).toString('hex'))
  }
  assert.ok(pixels.size > 1, 'a noise picture is not one colour')
})

test('copyImage puts the PNG on the clipboard, with the text beside it only when text is given', async () => {
  const written = []
  class ClipboardItem { constructor(entry) { this.entry = entry } }
  const clipboard = { write: async (items) => written.push(items) }
  const app = electronApp({ clipboard, ClipboardItem })
  const bytes = Buffer.from([1, 2, 3, 4]).toString('base64')
  await copyImage(app, bytes)
  await copyImage(app, bytes, 'https://example.com/page')
  assert.equal(written.length, 2)
  const [alone, withText] = written.map(([item]) => item.entry)
  assert.deepEqual(Object.keys(alone), ['image/png'])
  assert.deepEqual(Object.keys(withText), ['image/png', 'text/plain'])
  assert.equal(withText['text/plain'], 'https://example.com/page')
  assert.equal(alone['image/png'].type, 'image/png')
  assert.deepEqual([...Buffer.from(await alone['image/png'].arrayBuffer())], [1, 2, 3, 4])
})

test('menuPaste pastes into the first window through webContents.paste', async () => {
  const pasted = []
  const window = (name) => ({ webContents: { paste: () => { pasted.push(name); return name } } })
  const BrowserWindow = { getAllWindows: () => [window('first'), window('second')] }
  assert.equal(await menuPaste(electronApp({ BrowserWindow })), 'first')
  assert.deepEqual(pasted, ['first'])
})
