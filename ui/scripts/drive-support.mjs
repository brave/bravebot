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
