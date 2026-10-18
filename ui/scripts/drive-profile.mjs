// Setup for the drivers that write the real profile's `bravebot-ui.json`.
//
// Everything in this module acts on the real profile: `locateProfile` launches through
// `drive-support.mjs`, which passes no `--user-data-dir`, and `appState` then reads and writes
// the person's own file. A driver replaces only its own rows inside a shared key, and puts its
// own keys back. `bots` holds somebody's actual bots (names and purposes they wrote, sessions
// with history in them, and the only copy of each face), so clearing the key to make room is a
// destructive act one interrupted run away from happening. `drive-bots.mjs` records that an
// earlier version did exactly that.

import { mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { homedir, tmpdir } from 'node:os'
import { join } from 'node:path'
import { launch, SIDEBAR_TAB } from './drive-support.mjs'

/**
 * Where the app keeps what it remembers, asked of the app rather than composed here: `userData`
 * is Electron's to decide and differs by platform. A launch of its own, so a driver can change
 * the state before the first launch that is measured.
 */
export async function locateProfile() {
  const naming = await launch()
  const userData = await naming.evaluate(({ app }) => app.getPath('userData'))
  await naming.close()
  return { userData, stateFile: join(userData, 'bravebot-ui.json') }
}

/** Read and write one key of the state file, leaving every other key as it was found. */
export function appState(stateFile) {
  const readState = () => {
    try {
      return JSON.parse(readFileSync(stateFile, 'utf8'))
    } catch {
      return {}
    }
  }
  const putKey = (key, value) => {
    const state = readState()
    if (value === undefined) delete state[key]
    else state[key] = value
    try {
      writeFileSync(stateFile, `${JSON.stringify(state, null, 2)}\n`, 'utf8')
    } catch {
      // The app has not written the file yet, which is a state it handles on every first launch.
    }
  }
  return { readState, putKey }
}

/** Wait for a turn to finish, or for the screen that says why it cannot. */
export async function settle(page, seconds = 90) {
  for (let i = 0; i < seconds; i++) {
    if (await page.locator('.unconfigured').isVisible().catch(() => false)) return 'unconfigured'
    const running = await page.locator('.working').isVisible().catch(() => false)
    if (!running && i > 3) return 'done'
    await page.waitForTimeout(1000)
  }
  return 'timeout'
}

/** A checkout of its own, so a bot's memory file and a run's approvals touch nothing real. */
export function scratchCheckout(name) {
  const checkout = join(tmpdir(), name)
  rmSync(checkout, { recursive: true, force: true })
  mkdirSync(checkout, { recursive: true })
  writeFileSync(join(checkout, 'README.md'), '# a scratch checkout\n', 'utf8')
  return checkout
}

/**
 * Remove the records the agent wrote for a scratch checkout. Sessions are kept per checkout under
 * a directory named by mangling its path (every character outside `[A-Za-z0-9._]` becomes a dash),
 * so the scratch checkout has one of its own and nothing else is in it. Without this, every run of
 * a driver leaves a session in the list the other drivers read, pointing at a folder that is
 * now gone.
 */
export function forgetCheckoutSessions(checkout) {
  rmSync(join(homedir(), '.bravebot', 'sessions', checkout.replace(/[^A-Za-z0-9._]/g, '-')), {
    recursive: true,
    force: true,
  })
}

/**
 * Make a bot and open its page with the scratch checkout picked as its project. The conversation
 * runs in `checkout`, picked in the composer's project menu; the picker is native, so the dialog
 * it opens is answered here. `row` is the `/^Name$/` pattern the caller already writes.
 */
export async function openBot(app, page, { name, row, purpose, checkout, check }) {
  await app.evaluate(({ dialog }, directory) => {
    dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [directory] })
  }, checkout)
  await page.evaluate(([name, purpose]) => window.bravebot.writeBot({ name, purpose }), [name, purpose])
  await page.reload()
  await page.waitForTimeout(2000)
  await page.locator(SIDEBAR_TAB).nth(1).click()
  await page.waitForTimeout(400)

  const mine = page.locator('.bot').filter({ has: page.locator('.bot-name', { hasText: row }) })
  check((await mine.count()) === 1, 'the bot is in the list')
  await mine.locator('.bot-open-button').click()
  await page.locator('[data-test="bot-page"]').waitFor()
  await page.locator('[data-test="project-trigger"]').click()
  await page.locator('[data-test="project-pick"]').click()
  await page.waitForTimeout(400)
}
