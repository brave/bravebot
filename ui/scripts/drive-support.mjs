// Setup that the drivers running against the live session list share.
//
// Everything launched from here uses the real profile: no `--user-data-dir` is passed, so the app
// reads and writes the person's own `bravebot-ui.json` and `experience.json`, and the agent it
// starts uses their `HOME`. `ui/docs/testing.md` lists which drivers do that. A driver that
// launches through this module is on that list.
//
// `playwright-core` is loaded when a launch happens, so the helpers that only take a page can be
// tested without a browser driver installed.

/** The composer's text box, and the same while it accepts input (no turn is running). */
export const COMPOSER_INPUT = '.composer textarea'
export const COMPOSER_IDLE = '.composer textarea:not([disabled])'
/** The left column's tabs: Chats first, Bots second. */
export const SIDEBAR_TAB = '.sidebar-tabs [role="option"]'
/** The composer's send button, which reads Stop while a turn runs. */
export const COMPOSER_SEND = '.composer .send'
/** The accessible name of the composer's text box. */
export const COMPOSER_NAME = 'Message the agent'
/** A reply from the agent in the transcript. */
export const ASSISTANT_BUBBLE = '.bubble.assistant'
/** The button on a bot's row that opens its page. */
export const BOT_OPEN_BUTTON = '.bot-open-button'
/** The greeting a bot's page shows before its first turn. */
export const BOT_GREETING = '[data-test="bot-greeting"]'
/** The yes on a trust question. */
export const TRUST_APPROVE = '[data-test="trust-approve"]'
/**
 * An answering button of a question in a `.confirm-actions` row. Each is a `leo-button` host
 * whose label is slotted light-DOM text; `.confirm-actions button` instead pierces the host's
 * open shadow root and matches its inner native `<button>`, whose own text is never set, so
 * reading a label through it reads empty text (#3659).
 */
export const CONFIRM_BUTTON = '.confirm-actions leo-button'

/** The trust question, which is a dialog named "Project trust". */
export const trustQuestion = (page) => page.getByRole('dialog', { name: 'Project trust', exact: true })

/**
 * Scroll every box in the question through, the dialog's own body and the path box inside it, and
 * back to the top. The yes is held shut until every row it rests on has been on screen, so a
 * driver that means to press it reads the question first. A question that fits needs no scrolling
 * and this does nothing to it. It runs in the page, so it names nothing outside itself.
 */
export function readTrust(question) {
  return question.evaluate(async (host) => {
    const settle = () => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(resolve, 120))))
    const scrolls = (element) => /auto|scroll/.test(getComputedStyle(element).overflowY) && element.scrollHeight > element.clientHeight + 1
    const outer = [...(host.shadowRoot?.querySelectorAll('*') ?? [])].filter(scrolls)
    const inner = [...host.querySelectorAll('*')].filter(scrolls)
    const through = async (element, step) => {
      const last = element.scrollHeight - element.clientHeight
      for (let at = 0; ; at = Math.min(last, at + step)) {
        element.scrollTo({ top: at, behavior: 'instant' })
        await settle()
        if (at >= last) break
      }
      element.scrollTo({ top: 0, behavior: 'instant' })
      await settle()
    }
    const insides = async () => {
      for (const element of inner) {
        const box = element.getBoundingClientRect()
        const showing = Math.min(box.bottom, innerHeight) - Math.max(box.top, 0)
        if (showing >= Math.min(element.clientHeight, 80)) await through(element, Math.max(16, showing * 0.3))
      }
    }
    await insides()
    for (const element of outer) {
      const last = element.scrollHeight - element.clientHeight
      const step = Math.max(40, element.clientHeight * 0.3)
      for (let at = 0; ; at = Math.min(last, at + step)) {
        element.scrollTo({ top: at, behavior: 'instant' })
        await settle()
        await insides()
        if (at >= last) break
      }
      element.scrollTo({ top: 0, behavior: 'instant' })
      await settle()
    }
    return { outer: outer.length, inner: inner.length }
  })
}

/** Read the question through, then press its yes. The press waits for the yes to open. */
export async function approveTrust(question) {
  await readTrust(question)
  await question.locator(TRUST_APPROVE).click()
}

/** The `check` helper every driver prints with, and the list of what failed. */
export function checker() {
  const problems = []
  const check = (ok, what) => {
    console.log(`${ok ? '  ok  ' : ' FAIL '} ${what}`)
    if (!ok) problems.push(what)
  }
  return { check, problems }
}

/**
 * Launch the app against the real profile (no `--user-data-dir`), as the development build runs.
 */
export const launch = async () => {
  const { _electron: electron } = await import('playwright-core')
  return electron.launch({ args: ['.'], cwd: process.cwd(), timeout: 40000 })
}

// The left column has two tabs and remembers which was last open, so a run after somebody left it
// on the bots — or after a run of `drive-bots.mjs` that was interrupted before its teardown — would
// find the session list present in the DOM and invisible, and every click below would time out
// against an element that is right there. Everything here is about that list, so it is put back on
// screen first: a driver leaves the window as the next one expects to find it, and does not assume
// it was left that way.
export async function showSessions(page) {
  await page
    .locator(SIDEBAR_TAB)
    .first()
    .waitFor({ state: 'visible', timeout: 15000 })
    .catch(() => undefined)
  await page
    .locator(SIDEBAR_TAB)
    .first()
    .click({ timeout: 3000 })
    .catch(() => undefined)
  await page.waitForTimeout(250)
}

/** Each driver sets its own wait after this returns. */
export async function launchLive(start = launch) {
  const app = await start()
  const page = await app.firstWindow()
  await page.waitForLoadState('domcontentloaded')
  await showSessions(page)
  page.on('pageerror', (e) => console.log('PAGE ERROR:', e.message))
  page.on('console', (m) => m.type() === 'error' && console.log('CONSOLE ERROR:', m.text()))
  return { app, page }
}
