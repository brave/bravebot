// Setup for the drivers that write `bravebot-ui.json` and the bots file.
//
// The bot drivers run against `scratchProfile`, a user-data directory and a state directory of
// their own, so the app never reads or rewrites a person's real `bravebot-ui.json`. The two that
// send a turn ask for a live profile, which copies the settings, model and subscription files in. The bots file
// holds somebody's actual bots (names and purposes they wrote, sessions with history in them, and
// the only copy of each face), so `appBots` refuses to run unless `BRAVEBOT_HOME` names a scratch
// directory, refuses a user-data directory whose `bravebot-ui.json` still holds bots the agent has
// not moved, and never reads a file that does not parse as no rows. `drive-bots.mjs` records that
// an earlier version cleared the real file to make room. `locateProfile` is the one helper that
// still launches against the real profile, and `drive-theme.mjs` is its only caller.

import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs'
import { constants, homedir, tmpdir } from 'node:os'
import { dirname, isAbsolute, join } from 'node:path'
import { launch, SIDEBAR_TAB } from './drive-support.mjs'

const LIVE_FILES = ['settings.json', 'model', 'leo-premium.json']
const SIGNALS = ['SIGINT', 'SIGTERM', 'SIGHUP']
const environmentAtStart = { BRAVEBOT_HOME: process.env.BRAVEBOT_HOME, HOME: process.env.HOME, USERPROFILE: process.env.USERPROFILE }

/** The state directory the agent uses when nothing has overridden it, or null where nothing names one. */
export function realStateDirectory(environment = environmentAtStart) {
  const named = environment.BRAVEBOT_HOME
  if (named && isAbsolute(named)) return named
  const variables = process.platform === 'win32' ? ['HOME', 'USERPROFILE'] : ['HOME']
  for (const variable of variables) {
    const profile = environment[variable]
    if (profile && isAbsolute(profile)) return join(profile, '.bravebot')
  }
  return null
}

function copyLiveFiles(from, to) {
  if (!from) return []
  const copied = []
  for (const name of LIVE_FILES) {
    let contents
    try {
      contents = readFileSync(join(from, name))
    } catch (error) {
      if (error.code === 'ENOENT' || error.code === 'ENOTDIR') continue
      throw error
    }
    const copy = join(to, name)
    writeFileSync(copy, contents, { mode: 0o600 })
    chmodSync(copy, 0o600)
    copied.push(name)
  }
  return copied
}

function removeWhenTheProgramEnds(root) {
  const stops = SIGNALS.map((signal) => [signal, () => process.exit(128 + constants.signals[signal])])
  const remove = () => {
    process.off('exit', remove)
    for (const [signal, stop] of stops) process.off(signal, stop)
    rmSync(root, { recursive: true, force: true })
  }
  process.on('exit', remove)
  for (const [signal, stop] of stops) process.on(signal, stop)
  return remove
}

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

/**
 * A user-data directory and a state directory of their own under the temporary directory.
 *
 * Sets `BRAVEBOT_HOME` in this process, which the app and the agent it starts inherit at launch.
 * A live profile, for a driver that sends a turn to a model, also copies the settings, the recorded
 * model and the subscription out of the real state directory into the scratch one, readable by
 * their owner alone. The profile is removed with those copies when the program ends by an exit, an
 * uncaught error or SIGINT, SIGTERM or SIGHUP, and a SIGKILL leaves them in the temporary directory.
 */
export function scratchProfile(name, { live = false, realState = realStateDirectory() } = {}) {
  const root = mkdtempSync(join(tmpdir(), `${name}-`))
  const remove = removeWhenTheProgramEnds(root)
  try {
    const userData = join(root, 'user-data')
    mkdirSync(userData)
    const state = join(root, 'state')
    mkdirSync(state)
    const seeded = live ? copyLiveFiles(realState, state) : []
    process.env.BRAVEBOT_HOME = state
    return { userData, stateFile: join(userData, 'bravebot-ui.json'), seeded, remove }
  } catch (error) {
    remove()
    throw error
  }
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

/** The bots file the agent keeps, which `BRAVEBOT_HOME` moves and the desktop reads through the agent. */
export function botsFile() {
  const home = process.env.BRAVEBOT_HOME
  if (!home) throw new Error('Set BRAVEBOT_HOME to a scratch directory: this driver writes the bots file, and without it that is the real ~/.bravebot/bots.json.')
  return join(home, 'bots.json')
}

const holdsBots = (stateFile) => {
  try {
    const { bots } = JSON.parse(readFileSync(stateFile, 'utf8'))
    return Array.isArray(bots) && bots.length > 0
  } catch {
    return false
  }
}

/**
 * Read and write the rows of the bots file, which holds nothing else.
 *
 * A file that exists and does not parse is refused, never read as no rows, because the write that
 * follows would replace it. A write goes through a temporary file renamed into place. A user-data
 * directory whose `bravebot-ui.json` holds bots, with no bots file yet, is refused: the app would
 * move them into this state directory and then drop them from the file they came from.
 */
export function appBots(userData) {
  if (!userData) throw new Error('appBots needs the user-data directory the driver launches the app with.')
  const file = botsFile()
  const old = join(userData, 'bravebot-ui.json')
  if (!existsSync(file) && holdsBots(old)) {
    throw new Error(`${old} still holds bots that the agent has not moved to ${file}. Run a driver only against a scratch user-data directory.`)
  }
  const readBots = () => {
    let text
    try {
      text = readFileSync(file, 'utf8')
    } catch (error) {
      if (error.code === 'ENOENT') return []
      throw error
    }
    try {
      return JSON.parse(text).bots ?? []
    } catch (error) {
      throw new Error(`${file} does not parse: ${error.message}`)
    }
  }
  const putBots = (bots) => {
    mkdirSync(dirname(file), { recursive: true })
    const temporary = join(dirname(file), `.bots.json.${process.pid}.tmp`)
    writeFileSync(temporary, `${JSON.stringify({ bots }, null, 2)}\n`, 'utf8')
    renameSync(temporary, file)
  }
  return { readBots, putBots }
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
