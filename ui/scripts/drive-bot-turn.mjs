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
// The bot's conversation runs in the scratch checkout, picked in the composer's project menu. The
// picker is native, so the dialog it opens is answered here.
await app.evaluate(({ dialog }, directory) => {
  dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [directory] })
}, checkout)
const page = await app.firstWindow()
await page.waitForLoadState('domcontentloaded')
page.on('pageerror', (error) => console.log('PAGE ERROR:', error.message))
await page.waitForTimeout(2500)

// The purpose carries a word nothing else would produce, so the reply is evidence the briefing
// arrived rather than evidence the model is agreeable.
await page.evaluate(() =>
  window.bravebot.writeBot({
    name: 'Custodian',
    purpose:
      'You are the custodian of this checkout. Whenever you are greeted, and only then, ' +
      'reply with exactly the word: harbour. Do not explain it.',
  }),
)
await page.reload()
await page.waitForTimeout(2000)
await page.locator('.sidebar-tabs [role="option"]').nth(1).click()
await page.waitForTimeout(400)

const mine = page
  .locator('.bot')
  .filter({ has: page.locator('.bot-name', { hasText: /^Custodian$/ }) })
check((await mine.count()) === 1, 'the bot is in the list')
await mine.locator('.bot-open-button').click()
await page.locator('[data-test="bot-page"]').waitFor()

// The bot's page starts a conversation from its composer, in the project picked in its footer.
await page.locator('[data-test="project-trigger"]').click()
await page.locator('[data-test="project-pick"]').click()
await page.waitForTimeout(400)
await page.locator('.composer textarea').fill('Hello.')
await page.locator('.send').click()
await page.waitForTimeout(1500)

// A new conversation in a folder nobody has trusted asks the trust question, exactly as any new
// chat does; the message waits for the answer.
const trust = page.locator('.trust button').first()
if (await trust.isVisible().catch(() => false)) {
  await trust.click()
  await page.waitForTimeout(600)
  check(true, 'a new bot conversation asks about the checkout, like any new chat')
}
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
check(
  stored?.conversations?.some((each) => each.id === stored.session && each.directory === checkout),
  'and the folder that conversation ran in',
)

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
await page2.locator('[data-test="bot-conversations"] .bot-history-row').first().click()
await page2.waitForTimeout(2000)

check(
  (await page2.locator('.bubble.assistant').count()) >= 1,
  'reopening the bot brings back what it said before — the same session, resumed',
)
check(
  (await page2.locator('.attached').count()) >= 1,
  'the briefing is drawn as a file that was read rather than as something somebody typed',
)
// A bot's conversation is a chat like any other, so it is in the Chats list too, marked with the
// bot's face and named for the project it ran in.
await page2.locator('.sidebar-tabs [role="option"]').nth(0).click()
await page2.waitForTimeout(400)
const listed = page2.locator('.session').filter({ has: page2.locator('.bot-face') })
  .filter({ has: page2.locator('.session-project', { hasText: checkout.split('/').at(-1) }) })
check((await listed.count()) >= 1, 'the bot’s conversation is in the Chats list with its face and project')
await page2.screenshot({ path: '/tmp/bravebot-ui/25-bot-resumed.png' })

await back.close()

putKey('bots', withoutMine())
putKey('view', { ...(hadView ?? { grouped: false, collapsed: [] }), tab: 'sessions' })
rmSync(checkout, { recursive: true, force: true })
rmSync(join(userData, 'bot-homes', MINE), { recursive: true, force: true })
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
