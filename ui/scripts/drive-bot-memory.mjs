// That a prompt which reads like house-keeping this app sent is drawn as a prompt somebody typed,
// and that a reopened transcript draws no house-keeping row for a turn the app did not send.
//
// Run it the way `drive-bot-turn.mjs` is run. It costs two turns. It runs against a scratch
// user-data directory and state directory, and it puts its own keys back. The scratch state
// directory holds copies of the real `settings.json`, `model` and `leo-premium.json`, so the agent
// can reach the backend the real one is configured for, and the copies are removed when it ends.

import { checker, launch, COMPOSER_INPUT, BOT_OPEN_BUTTON, SHOTS_DIR } from './drive-support.mjs'
import { appBots, appState, forgetCheckoutSessions, openBot, scratchProfile, settle } from './drive-profile.mjs'
import { mkdirSync, rmSync } from 'node:fs'
import { join } from 'node:path'

mkdirSync(SHOTS_DIR, { recursive: true })

const { check, problems } = checker()

const { userData, stateFile, remove } = scratchProfile('bravebot-drive-bot-memory', { live: true })
const { readState, putKey } = appState(stateFile)
const { readBots, putBots } = appBots(userData)
const hadView = readState().view

// This driver's own row and no more. `bots` holds somebody's actual bots, so clearing it to make
// room is a destructive act one interrupted run away from happening. It adds one and removes one.
const MINE = 'archivist'
const withoutMine = () => readBots().filter((bot) => bot.slug !== MINE)

putBots(withoutMine())
putKey('view', undefined)

/**
 * A first line that reads like house-keeping this app sent itself.
 *
 * A sentence anybody can type, and the whole point of typing it here is that it buys nothing:
 * what makes a turn house-keeping is the `composed` tag the send carries, not anything in the
 * prompt, so this is a prompt somebody typed and is drawn as one in both places below. A driver
 * cannot reach the app's modules, so the sentence is written out.
 */
const LOOKALIKE = '[bravebot-ui] Keeping your memory current.'

const app = await launch(userData)
const page = await app.firstWindow()
await page.waitForLoadState('domcontentloaded')
page.on('pageerror', (error) => console.log('PAGE ERROR:', error.message))
await page.waitForTimeout(2500)

const home = await openBot(page, {
  name: 'Archivist',
  row: /^Archivist$/,
  purpose: 'You are the archivist of this folder. Keep your answers to one short sentence.',
  check,
})

// --- one ordinary turn -------------------------------------------------------------------

// Sent in the bot's conversation, which runs in its home folder.
await page.locator(COMPOSER_INPUT).fill('Name one file in this folder.')
await page.locator('.send').click()
await page.waitForTimeout(1500)
console.log('  ..   sent the first turn; waiting…')
const first = await settle(page)
// Asked before anything is asserted. A machine with no credentials has not failed this driver, it
// has declined to run it — and recording a FAIL on the way to announcing a skip is the kind of
// output that teaches somebody to stop reading it.
if (first === 'unconfigured') {
  console.log('\nRESULT: skipped — this machine has no credentials for a live turn')
  await app.close()
  putBots(withoutMine())
  putKey('view', { ...(hadView ?? { grouped: false, collapsed: [] }), tab: 'sessions' })
  rmSync(join(userData, 'bot-homes', MINE), { recursive: true, force: true })
  remove()
  process.exit(0)
}
check(first === 'done', `the first turn finished (${first})`)

// The prompt this app would compose, with a sentence in front of it that reads like the app
// talking to itself. Ordinary text as far as everything downstream is concerned, which is the
// point being driven: a person typed it, so a person typed it, and the reopened transcript below
// is where that has to still be true.
const prompt = [
  LOOKALIKE,
  '',
  `Look back over this conversation and bring \`.bravebot/memory/${MINE}.md\` up to date: write`,
  'down the name of the file you just mentioned, and nothing else.',
].join('\n')

await page.locator(COMPOSER_INPUT).fill(prompt)
await page.locator('.send').click()
await page.waitForTimeout(1200)

console.log('  ..   sent the second turn; waiting…')
const second = await settle(page)
check(second === 'done', `the second turn finished (${second})`)

// Drawn as a bubble, because somebody typed it. The reopened transcript below is the half that
// used to differ.
check(
  (await page.locator('.bubble.user').last().textContent())?.includes(LOOKALIKE) === true,
  'a prompt that reads like house-keeping is drawn as the prompt it is',
)
await page.screenshot({ path: `${SHOTS_DIR}/26-bot-memory-nudged.png` })

await app.close()

// --- and how it is drawn when the session is read back ------------------------------------

const back = await launch(userData)
const page2 = await back.firstWindow()
await page2.waitForLoadState('domcontentloaded')
await page2.waitForTimeout(2500)
await page2
  .locator('.bot')
  .filter({ has: page2.locator('.bot-name', { hasText: /^Archivist$/ }) })
  .locator(BOT_OPEN_BUTTON)
  .click()
await page2.waitForTimeout(2500)

// LAYER-6. Nothing in this session was composed by the app, so nothing in it is drawn as the row
// the app writes about itself, however the prompts read. A build deciding this from a message's
// first line draws that row here, with the words somebody typed left out of the transcript and
// the ordinal a fork of them cuts on gone with them.
check(
  (await page2.locator('.consolidation').count()) === 0,
  'a reopened transcript draws no house-keeping row for a turn the app did not send',
)
const bubbles = await page2.locator('.bubble.user').allTextContents()
check(
  bubbles.some((text) => text.includes(LOOKALIKE)),
  'and draws the prompt that reads like one with the words somebody typed',
)
check(
  bubbles.some((text) => text.includes('Name one file')),
  'while what somebody else typed is still drawn as theirs',
)
check(
  (await page2.locator('.bubble.user .fork-here').count()) === bubbles.length,
  'every one of them offers the cut that is taken on its ordinal',
)
await page2.screenshot({ path: `${SHOTS_DIR}/27-bot-memory-replayed.png` })

await back.close()

// --- putting back only what this driver moved ---------------------------------------------

putBots(withoutMine())
putKey('view', { ...(hadView ?? { grouped: false, collapsed: [] }), tab: 'sessions' })
rmSync(join(userData, 'bots', MINE), { recursive: true, force: true })
rmSync(join(userData, 'bot-homes', MINE), { recursive: true, force: true })
forgetCheckoutSessions(home)

console.log(problems.length ? `\nRESULT: ${problems.length} problem(s)` : '\nRESULT: ok')
remove()
process.exit(problems.length ? 1 : 0)
