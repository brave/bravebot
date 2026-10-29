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

import { checker, launch, COMPOSER_INPUT, SIDEBAR_TAB, ASSISTANT_BUBBLE, BOT_OPEN_BUTTON } from './drive-support.mjs'
import { appState, forgetCheckoutSessions, locateProfile, openBot, settle } from './drive-profile.mjs'
import { existsSync, mkdirSync, rmSync } from 'node:fs'
import { join } from 'node:path'

mkdirSync('/tmp/bravebot-ui', { recursive: true })

const { check, problems } = checker()

const { userData, stateFile } = await locateProfile()
const { readState, putKey } = appState(stateFile)
const hadView = readState().view

// This driver's own row and no more. `bots` holds somebody's actual bots — names and purposes they
// wrote, sessions with history in them, and the only copy of what each one's face looks like — so
// clearing it to make room, and putting it back at the end, is a destructive act one interrupted
// run away from happening. It adds one bot and removes one bot.
const MINE = 'custodian'
const withoutMine = () => (readState().bots ?? []).filter((bot) => bot.slug !== MINE)

putKey('bots', withoutMine())
putKey('view', undefined)

const app = await launch()
const page = await app.firstWindow()
await page.waitForLoadState('domcontentloaded')
page.on('pageerror', (error) => console.log('PAGE ERROR:', error.message))
await page.waitForTimeout(2500)

// The purpose carries a word nothing else would produce, so the reply is evidence the briefing
// arrived rather than evidence the model is agreeable.
// Opening the bot opens its one conversation, in its home folder, and answers the trust question
// that folder asks.
const home = await openBot(page, {
  name: 'Custodian',
  row: /^Custodian$/,
  purpose:
    'You are the custodian of this folder. Whenever you are greeted, and only then, ' +
    'reply with exactly the word: harbour. Do not explain it.',
  check,
})
check(typeof home === 'string', `the bot has a home folder (${home})`)
await page.locator(COMPOSER_INPUT).fill('Hello.')
await page.locator('.send').click()
await page.waitForTimeout(1500)
console.log('  ..   sent; waiting for the turn…')
const first = await settle(page)
check(first === 'done', `the turn finished (${first})`)

const reply = (await page.locator(ASSISTANT_BUBBLE).last().textContent()) ?? ''
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

// The memory file is real, and in the home folder where the bot can edit it.
const memory = join(home, '.bravebot-ui', 'bots', 'custodian.md')
check(existsSync(memory), 'the bot has a memory file inside its home folder')
check(
  existsSync(join(home, '.bravebot-ui', '.gitignore')),
  'and the folder holding it ignores itself, so it is not somebody’s diff',
)

// The id the agent minted has been written down, which is what a resume needs.
const stored = (readState().bots ?? []).find((bot) => bot.slug === MINE)
check(typeof stored?.session === 'string', `the bot remembers its session (${stored?.session})`)
check(
  stored?.conversations?.some((each) => each.id === stored.session && each.directory === home),
  'and that the conversation ran in its home folder',
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
  .locator(BOT_OPEN_BUTTON)
  .click()
await page2.waitForTimeout(2000)

check(
  (await page2.locator(ASSISTANT_BUBBLE).count()) >= 1,
  'reopening the bot brings back what it said before — the same session, resumed',
)
check(
  (await page2.locator('.attached').count()) >= 1,
  'the briefing is drawn as a file that was read rather than as something somebody typed',
)
// A bot's conversation is a chat like any other, so it is in the Chats list too, marked with the
// bot's face.
await page2.locator(SIDEBAR_TAB).nth(0).click()
await page2.waitForTimeout(400)
const listed = page2.locator('.session').filter({ has: page2.locator('.bot-face') })
check((await listed.count()) >= 1, 'the bot’s conversation is in the Chats list with its face')
await page2.screenshot({ path: '/tmp/bravebot-ui/25-bot-resumed.png' })

await back.close()

putKey('bots', withoutMine())
putKey('view', { ...(hadView ?? { grouped: false, collapsed: [] }), tab: 'sessions' })
rmSync(join(userData, 'bot-homes', MINE), { recursive: true, force: true })
forgetCheckoutSessions(home)

console.log(problems.length ? `\nRESULT: ${problems.length} problem(s)` : '\nRESULT: ok')
process.exit(problems.length ? 1 : 0)
