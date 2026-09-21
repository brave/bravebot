// A live turn as a bot, which is the only way to see whether any of this works.
//
// `drive-bots.mjs` next door proves a bot can be made, found again and told apart. It cannot prove
// the part the feature is actually for: that the purpose somebody wrote reaches the model, that the
// bot's memory is a real file it can edit, and that resuming the bot resumes the *same* session
// rather than beginning another. Each of those needs a model to answer, so this needs credentials
// and the driver above does not.
//
// Run it the way `drive-turn.mjs` is run. Like the other drivers it perturbs `bravebot-ui.json`,
// and it puts its own keys back.

import { _electron as electron } from 'playwright-core'
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
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

// A checkout of its own, so the bot's memory file and this run's approvals touch nothing real.
const checkout = join(tmpdir(), 'bravebot-ui-drive-bot-turn')
rmSync(checkout, { recursive: true, force: true })
mkdirSync(checkout, { recursive: true })
writeFileSync(join(checkout, 'README.md'), '# a scratch checkout\n', 'utf8')

// This driver's own row and no more. `bots` holds somebody's actual bots — names and purposes they
// wrote, sessions with history in them, and the only copy of what each one's face looks like — so
// clearing it to make room, and putting it back at the end, is a destructive act one interrupted
// run away from happening. It adds one bot and removes one bot.
const MINE = 'custodian'
const withoutMine = () => (readState().bots ?? []).filter((bot) => bot.slug !== MINE)

putKey('bots', withoutMine())
putKey('view', undefined)

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

// The purpose carries a word nothing else would produce, so the reply is evidence the briefing
// arrived rather than evidence the model is agreeable.
await page.evaluate(
  ([directory]) =>
    window.bravebot.writeBot({
      name: 'Custodian',
      purpose:
        'You are the custodian of this checkout. Whenever you are greeted, and only then, ' +
        'reply with exactly the word: harbour. Do not explain it.',
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
  .filter({ has: page.locator('.bot-name', { hasText: /^Custodian$/ }) })
check((await mine.count()) === 1, 'the bot is in the list')
await mine.locator('.bot-open-button').click()
await page.waitForTimeout(1500)

// A bot that has never spoken has no session to resume, so opening it begins one — which asks the
// trust question, exactly as any new session does.
const trust = page.locator('.trust button').first()
if (await trust.isVisible().catch(() => false)) {
  await trust.click()
  await page.waitForTimeout(600)
  check(true, 'a bot with no session yet asks about the checkout, like any new session')
}

await page.locator('.composer textarea').fill('Hello.')
await page.locator('.send').click()
console.log('  ..   sent; waiting for the turn…')
const first = await settle(page)
check(first === 'done', `the turn finished (${first})`)

const reply = (await page.locator('.bubble.assistant').last().textContent()) ?? ''
check(
  reply.toLowerCase().includes('harbour'),
  `the reply obeys the purpose nobody typed into the composer (${reply.slice(0, 60)})`,
)

await page.screenshot({ path: '/tmp/bravebot-ui/24-bot-turn.png' })

// Nothing is drawn for the briefing during the turn it arrives in, and that is not an oversight:
// the agent emits no event when it reads a file the user named, so a live transcript has nothing
// to draw from. It shows up on the reopened session below, where the transcript is built from the
// record instead — which is exactly where it would otherwise have been drawn as a prompt bubble
// holding the whole file.
check(
  (await page.locator('.attached').count()) === 0,
  'a live turn draws nothing for the briefing, having been told nothing about it',
)

// The memory file is real, and in the checkout where the bot can edit it.
const memory = join(checkout, '.bravebot-ui', 'bots', 'custodian.md')
check(existsSync(memory), 'the bot has a memory file inside its checkout')
check(
  existsSync(join(checkout, '.bravebot-ui', '.gitignore')),
  'and the folder holding it ignores itself, so it is not somebody’s diff',
)

// The id the agent minted has been written down, which is what a resume needs.
const stored = (readState().bots ?? []).find((bot) => bot.slug === MINE)
check(typeof stored?.session === 'string', `the bot remembers its session (${stored?.session})`)

await app.close()

// --- and the whole point: the same session, resumed --------------------------------------

const back = await launch()
const page2 = await back.firstWindow()
await page2.waitForLoadState('domcontentloaded')
await page2.waitForTimeout(2500)
await page2
  .locator('.bot')
  .filter({ has: page2.locator('.bot-name', { hasText: /^Custodian$/ }) })
  .locator('.bot-open-button')
  .click()
await page2.waitForTimeout(2000)

check(
  (await page2.locator('.bubble.assistant').count()) >= 1,
  'reopening the bot brings back what it said before — the same session, resumed',
)
check(
  (await page2.locator('.attached').count()) >= 1,
  'the briefing is drawn as a file that was read rather than as something somebody typed',
)
// Exactly one session fewer on screen than the agent has records for, and the missing one is this
// bot's. Counted rather than matched on a title: a session is named after its first prompt, so
// looking for the words that were typed also matches a leftover from an earlier run of this driver
// — which is how this assertion first went wrong.
// Every record the agent has, minus the ones that belong to a bot — this driver's, and any the
// person running it already had. Counted rather than matched on a title: a session is named after
// its first prompt, so looking for the words that were typed also matches a leftover from an
// earlier run of this driver, which is how this assertion first went wrong.
const { known, owned } = await page2.evaluate(async () => {
  const answer = await window.bravebot.request('session.list')
  const bots = await window.bravebot.readBots()
  const theirs = new Set(
    bots.filter((bot) => bot.session).map((bot) => `${bot.directory}/${bot.session}`),
  )
  return {
    known: answer.ok.sessions.length,
    owned: answer.ok.sessions.filter((s) => theirs.has(`${s.directory}/${s.id}`)).length,
  }
})
await page2.locator('.sidebar-tab').nth(0).click()
await page2.waitForTimeout(400)
const drawn = await page2.locator('.session').count()
check(
  owned >= 1 && drawn === known - owned,
  `every bot's session is kept out of the sessions list (${drawn} drawn of ${known}, ${owned} owned by bots)`,
)
await page2.screenshot({ path: '/tmp/bravebot-ui/25-bot-resumed.png' })

await back.close()

putKey('bots', withoutMine())
putKey('view', { ...(hadView ?? { grouped: false, collapsed: [] }), tab: 'sessions' })
rmSync(checkout, { recursive: true, force: true })
// And the records the agent wrote. Sessions are kept per checkout under a directory named by
// mangling its path — every character outside `[A-Za-z0-9._]` becomes a dash — so the scratch
// checkout has one of its own and nothing else is in it. Without this, every run of this driver
// leaves a session in the list the other drivers read, pointing at a folder that is now gone.
rmSync(join(homedir(), '.bravebot', 'sessions', checkout.replace(/[^A-Za-z0-9._]/g, '-')), {
  recursive: true,
  force: true,
})

console.log(problems.length ? `\nRESULT: ${problems.length} problem(s)` : '\nRESULT: ok')
process.exit(problems.length ? 1 : 0)
