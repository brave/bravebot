// That a bot is asked to keep its memory current without anybody asking it to.
//
// `drive-bot-turn.mjs` next door proves the memory file is real and that the briefing reaches the
// model. It does not touch the question this feature exists for: the instruction to *write* lives
// in the briefing, the briefing reaches a turn only when that turn is grounded, and a session
// spends most of its life ungrounded. So a bot was told once, at the start, and then left to
// remember on its own for as long as the conversation ran.
//
// Two claims here, and they need a model for different reasons:
//
//  1. **A bot that has stopped writing is grounded early.** `quiet` counts turns since its memory
//     file last moved; past `QUIET_MAX` the next turn carries the briefing whether the window
//     thought it was due or not, with a paragraph saying so. Checked against the ground file on
//     disk rather than against anything the model said — the file is written synchronously on the
//     way into the send, so this is a claim about the app and not about whether a model took the
//     hint.
//  2. **A turn this app sent is not drawn as one somebody typed.** A consolidation opens with a
//     mark, and a reopened transcript matches on it to draw a line rather than a prompt bubble.
//     That match is the fragile half of the arrangement and the only half a reopened session can
//     be asked about.
//
// What is **not** covered, and cannot cheaply be: the trigger itself. A consolidation is sent when
// a conversation's archive rises, which happens when a compaction actually happens, which needs a
// conversation long enough to be worth compacting. Faking it is not available either — the archive
// only rises, so seeding a low figure and waiting for a real one to exceed it needs the real one to
// be non-zero, which is the thing being avoided. The comparison it turns on is two lines in
// `src/main/index.ts`; the expensive parts around it are what is driven here.
//
// Run it the way `drive-bot-turn.mjs` is run. It costs two turns. Like the other drivers it
// perturbs `bravebot-ui.json`, and it puts its own keys back.

import { _electron as electron } from 'playwright-core'
import { existsSync, mkdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { homedir, tmpdir } from 'node:os'

mkdirSync('/tmp/bravebot-ui', { recursive: true })

const problems = []
const check = (ok, what) => {
  console.log(`${ok ? '  ok  ' : ' FAIL '} ${what}`)
  if (!ok) problems.push(what)
}

const launch = () => electron.launch({ args: ['.'], cwd: process.cwd(), timeout: 40000 })

const naming = await launch()
const userData = await naming.evaluate(({ app }) => app.getPath('userData'))
await naming.close()

const stateFile = join(userData, 'bravebot-ui.json')
const readState = () => {
  try {
    return JSON.parse(readFileSync(stateFile, 'utf8'))
  } catch {
    return {}
  }
}
const hadView = readState().view
const putKey = (key, value) => {
  const state = readState()
  if (value === undefined) delete state[key]
  else state[key] = value
  try {
    writeFileSync(stateFile, `${JSON.stringify(state, null, 2)}\n`, 'utf8')
  } catch {}
}

// A checkout of its own, so the memory file this writes and the approvals this run earns touch
// nothing real.
const checkout = join(tmpdir(), 'bravebot-ui-drive-bot-memory')
rmSync(checkout, { recursive: true, force: true })
mkdirSync(checkout, { recursive: true })
writeFileSync(join(checkout, 'README.md'), '# a scratch checkout\n', 'utf8')

// This driver's own row and no more. `bots` holds somebody's actual bots, so clearing it to make
// room is a destructive act one interrupted run away from happening. It adds one and removes one.
const MINE = 'archivist'
const withoutMine = () => (readState().bots ?? []).filter((bot) => bot.slug !== MINE)
const stored = () => (readState().bots ?? []).find((bot) => bot.slug === MINE) ?? null

putKey('bots', withoutMine())
putKey('view', undefined)

/**
 * The first line of a turn this app sends on a bot's behalf.
 *
 * Copied rather than imported, because a driver launches the built app and cannot reach into its
 * modules. It must match `CONSOLIDATION_MARK` in `src/shared/bots.ts`, and the check below is what
 * says so out loud when it stops matching.
 */
const MARK = '[bravebot-ui] Keeping your memory current.'

/** Wait for a turn to finish, or for the screen that says why it cannot. */
async function settle(page, seconds = 90) {
  for (let i = 0; i < seconds; i++) {
    if (await page.locator('.unconfigured').isVisible().catch(() => false)) return 'unconfigured'
    const running = await page.locator('.working').isVisible().catch(() => false)
    if (!running && i > 3) return 'done'
    await page.waitForTimeout(1000)
  }
  return 'timeout'
}

const app = await launch()
const page = await app.firstWindow()
await page.waitForLoadState('domcontentloaded')
page.on('pageerror', (error) => console.log('PAGE ERROR:', error.message))
await page.waitForTimeout(2500)

await page.evaluate(
  ([directory]) =>
    window.bravebot.writeBot({
      name: 'Archivist',
      purpose:
        'You are the archivist of this checkout. Keep your answers to one short sentence.',
      directory,
    }),
  [checkout],
)
await page.reload()
await page.waitForTimeout(2000)
await page.locator('.sidebar-tab').nth(1).click()
await page.waitForTimeout(400)

const mine = page
  .locator('.bot')
  .filter({ has: page.locator('.bot-name', { hasText: /^Archivist$/ }) })
check((await mine.count()) === 1, 'the bot is in the list')
await mine.locator('.bot-open-button').click()
await page.waitForTimeout(1500)

const trust = page.locator('.trust button').first()
if (await trust.isVisible().catch(() => false)) {
  await trust.click()
  await page.waitForTimeout(600)
}

// --- one ordinary turn, to ground the session --------------------------------------------

await page.locator('.composer textarea').fill('Name one file in this checkout.')
await page.locator('.send').click()
console.log('  ..   sent the first turn; waiting…')
const first = await settle(page)
// Asked before anything is asserted. A machine with no credentials has not failed this driver, it
// has declined to run it — and recording a FAIL on the way to announcing a skip is the kind of
// output that teaches somebody to stop reading it.
if (first === 'unconfigured') {
  console.log('\nRESULT: skipped — this machine has no credentials for a live turn')
  await app.close()
  putKey('bots', withoutMine())
  putKey('view', { ...(hadView ?? { grouped: false, collapsed: [] }), tab: 'sessions' })
  rmSync(checkout, { recursive: true, force: true })
  process.exit(0)
}
check(first === 'done', `the first turn finished (${first})`)

const groundFile = join(userData, 'bots', MINE, 'ground.md')
check(existsSync(groundFile), 'the briefing was composed as a file outside the checkout')
check(
  !readFileSync(groundFile, 'utf8').includes('not changed it in a while'),
  'and says nothing about a quiet memory, this bot having only just started',
)

// --- then a quiet spell, and the turn that answers it -------------------------------------

// Seeded rather than earned. Earning it means six turns nobody needs and the model deciding, on
// each of them, not to write anything down — six chances for this to fail over something that is
// not what it is testing. Written between turns, when nothing in the app is writing this file:
// `noteBotMemory` runs on `turn.done`, which has already happened.
const before = stored()
check(before !== null, 'the bot has a row to seed')
putKey(
  'bots',
  (readState().bots ?? []).map((bot) =>
    bot.slug === MINE
      ? { ...bot, quiet: 9, remembered: statSync(join(checkout, '.bravebot-ui', 'bots', `${MINE}.md`)).mtimeMs }
      : bot,
  ),
)

// The prompt is the one this app would have sent itself. Ordinary text as far as the composer is
// concerned, which is the point: what makes it a consolidation is the mark it opens with, and the
// reopened transcript below is where that is read back.
const prompt = [
  MARK,
  '',
  `Look back over this conversation and bring \`.bravebot-ui/bots/${MINE}.md\` up to date: write`,
  'down the name of the file you just mentioned, and nothing else.',
].join('\n')

await page.locator('.composer textarea').fill(prompt)
await page.locator('.send').click()
await page.waitForTimeout(1200)

// Read while the turn is still running. `ground()` writes this synchronously on the way into the
// send, so the paragraph is on disk long before the model has said anything — which is the whole
// reason this claim can be made about the app rather than about a model.
check(
  readFileSync(groundFile, 'utf8').includes('not changed it in a while'),
  'a bot whose memory has gone quiet is handed its briefing again, with a line saying so',
)
check(stored()?.quiet === 0, 'and the count resets on the nudge, so it is not nudged every turn')

console.log('  ..   sent the second turn; waiting…')
const second = await settle(page)
check(second === 'done', `the second turn finished (${second})`)

// Drawn as a bubble live and as house-keeping once reopened, because this one *was* typed: the
// live transcript draws what the composer sent, and only the record is matched on the mark. That
// is the known cost of recognising a turn by its first line, and it is charged only to somebody
// who types a bracketed sentence out of this app's source. A consolidation the app actually sent
// never enters the composer, so it is drawn the same way in both places.
check(
  (await page.locator('.bubble.user').last().textContent())?.includes(MARK) === true,
  'typing the mark oneself still draws a bubble live — the record is what the match is for',
)
await page.screenshot({ path: '/tmp/bravebot-ui/26-bot-memory-nudged.png' })

const memory = join(checkout, '.bravebot-ui', 'bots', `${MINE}.md`)
check(existsSync(memory), 'the memory file is still where the bot can edit it')
// Whether the model took the hint is not this driver's to assert — nothing here can make it write,
// and a check that demanded it would fail on a model having an off day rather than on a bug. What
// is worth reporting is the app's own reading of it, which is the count that drives the next nudge:
// zero means `noteBotMemory` saw the file move.
check(
  typeof stored()?.quiet === 'number',
  `the app read the turn back as ${stored()?.quiet === 0 ? 'a memory that moved' : 'another quiet turn'} (quiet=${stored()?.quiet})`,
)

await app.close()

// --- and how it is drawn when the session is read back ------------------------------------

const back = await launch()
const page2 = await back.firstWindow()
await page2.waitForLoadState('domcontentloaded')
await page2.waitForTimeout(2500)
await page2
  .locator('.bot')
  .filter({ has: page2.locator('.bot-name', { hasText: /^Archivist$/ }) })
  .locator('.bot-open-button')
  .click()
await page2.waitForTimeout(2500)

check(
  (await page2.locator('.consolidation').count()) === 1,
  'a reopened transcript draws the consolidation as house-keeping, not as a prompt',
)
const bubbles = await page2.locator('.bubble.user').allTextContents()
check(
  !bubbles.some((text) => text.includes(MARK)),
  'and draws no prompt bubble for it — nobody typed it, and saying they did is the lie this avoids',
)
check(
  bubbles.some((text) => text.includes('Name one file')),
  'while what somebody did type is still drawn as theirs',
)
await page2.screenshot({ path: '/tmp/bravebot-ui/27-bot-memory-replayed.png' })

await back.close()

// --- putting back only what this driver moved ---------------------------------------------

putKey('bots', withoutMine())
putKey('view', { ...(hadView ?? { grouped: false, collapsed: [] }), tab: 'sessions' })
rmSync(checkout, { recursive: true, force: true })
rmSync(join(userData, 'bots', MINE), { recursive: true, force: true })
// And the records the agent wrote, kept per checkout under a directory named by mangling its path.
rmSync(join(homedir(), '.bravebot', 'sessions', checkout.replace(/[^A-Za-z0-9._]/g, '-')), {
  recursive: true,
  force: true,
})

console.log(problems.length ? `\nRESULT: ${problems.length} problem(s)` : '\nRESULT: ok')
process.exit(problems.length ? 1 : 0)
