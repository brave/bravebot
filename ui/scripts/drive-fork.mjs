// That a conversation can be cut in two, and that the half left behind is untouched.
//
// The interesting assertions are about identity. A fork must be a *different* session — a new
// id, its own record, its own future — and the session it came from must be exactly as it was.
// Getting that wrong does not look like a bug on screen: it looks like a session that quietly
// lost its history. So this checks the parent as carefully as the child.
//
// The rest is the loop a person actually walks: right-click a prompt, fork, find that prompt
// waiting in the composer, read the banner, click it, and land back on the row it was cut at.
//
// Costs nothing — no prompt is sent, so no model is called. It needs a stored session with two
// prompts in it and says so rather than failing when there is none. A native context menu is
// modal and would hang the run, so `Menu.prototype.popup` is replaced with something that
// records the items and clicks the one this asked for — the same trick `drive-menu.mjs` plays,
// with the click added, because here the item is the thing being tested.
import { _electron as electron } from 'playwright-core'
import { mkdirSync, existsSync, readFileSync, writeFileSync, rmSync } from 'node:fs'
import { join } from 'node:path'

const OUT = '/tmp/bravebot-ui'
mkdirSync(OUT, { recursive: true })

const problems = []
const check = (ok, what) => {
  console.log(`${ok ? '  ok  ' : ' FAIL '} ${what}`)
  if (!ok) problems.push(what)
}
const skip = (what) => console.log(`  --   ${what}`)
// Set once the app is up and there is a file to put back; `done` runs before that on the
// skip path, when nothing has been touched.
let restoreForks = null
const done = async (word) => {
  restoreForks?.()
  await app.close()
  if (problems.length > 0) {
    console.log(`\nRESULT: ${problems.length} failed`)
    process.exit(1)
  }
  console.log(`\nRESULT: ${word}`)
  process.exit(0)
}

// The left column has two tabs and remembers which was last open, so a run after somebody left it
// on the bots — or after a run of `drive-bots.mjs` that was interrupted before its teardown — would
// find the session list present in the DOM and invisible, and every click below would time out
// against an element that is right there. Everything here is about that list, so it is put back on
// screen first: a driver leaves the window as the next one expects to find it, and does not assume
// it was left that way.
async function showSessions(page) {
  await page
    .locator('.sidebar-tab')
    .first()
    .waitFor({ state: 'visible', timeout: 15000 })
    .catch(() => undefined)
  await page
    .locator('.sidebar-tab')
    .first()
    .click({ timeout: 3000 })
    .catch(() => undefined)
  await page.waitForTimeout(250)
}

const app = await electron.launch({ args: ['.'], cwd: process.cwd(), timeout: 40000 })
const page = await app.firstWindow()
await page.waitForLoadState('domcontentloaded')
await showSessions(page)
page.on('pageerror', (e) => console.log('PAGE ERROR:', e.message))
page.on('console', (m) => m.type() === 'error' && console.log('CONSOLE ERROR:', m.text()))
await page.waitForTimeout(2200)

/** Ask the agent something directly, the way the window does. */
const request = (method, params) =>
  page.evaluate(([m, p]) => window.bravebot.request(m, p), [method, params])

// A stored session with at least two prompts, so there is something in front of the second.
//
// Two kinds are passed over, and both for the same underlying reason: this driver counts prompts
// in two currencies and every assertion below assumes they are worth the same.
//
//  - A session belonging to a bot. Those are drawn in the bots tab and deliberately kept out of
//    the sessions list, so one picked here could never be found on screen.
//  - A session containing `Contents of …` messages, which is how a file somebody named enters a
//    conversation. The agent counts every user-role message as a prompt, and a fork's recorded
//    ordinal is over *those*; the window draws an attachment as a line rather than a bubble,
//    because nobody typed it, and this driver clicks the second *bubble*. In a session with an
//    attachment in it those two numberings come apart, and the fix is not to translate between
//    them in each of the four places below — it is to fork a session where there is nothing to
//    translate.
//
// Filtering the attachments out of the count was tried first and is worse than useless: it makes
// the *selection* agree with the window while leaving every later comparison counting the agent's
// way, so the driver reports three failures about forking and none of them are about forking.
const bots = await page.evaluate(() => window.bravebot.readBots())
const theirs = new Set(bots.filter((bot) => bot.session).map((bot) => `${bot.directory}/${bot.session}`))

const listed = await request('session.list', {})
let target = null
for (const session of listed.ok?.sessions ?? []) {
  if (theirs.has(`${session.directory}/${session.id}`)) continue
  const opened = await request('session.open', { directory: session.directory, id: session.id })
  const said = opened.ok?.said ?? []
  const prompts = said.filter((line) => line.kind === 'user')
  const attached = prompts.some((line) => line.text.startsWith('Contents of '))
  await request('session.close', { session: opened.ok?.session })
  if (prompts.length >= 2 && !attached) {
    target = { session, prompts }
    break
  }
}

if (!target) {
  skip('no stored session has two prompts in it — nothing to fork')
  await done('skipped')
}

const { session: parent, prompts } = target
console.log(`forking "${parent.title}" in front of its second prompt`)

// The whole list before anything is cut, to compare against afterwards.
const before = (await request('session.list', {})).ok?.sessions ?? []
const parentBefore = before.find((s) => s.id === parent.id && s.directory === parent.directory)

await app.evaluate(({ Menu }) => {
  globalThis.__pops = []
  Menu.prototype.popup = function popup() {
    globalThis.__pops.push(
      this.items.map((i) => ({ id: i.id, label: i.label, enabled: i.enabled })),
    )
  }
})

const popped = () => app.evaluate(() => globalThis.__pops.at(-1) ?? [])

await page.locator('.session').filter({ hasText: parent.title }).first().click()
await page.waitForTimeout(1200)
check(await page.locator('.bubble.user').first().isVisible(), 'the session opens with its prompts')

// --- what a right-click offers -----------------------------------------------------------
await page.locator('.bubble.user').first().click({ button: 'right' })
await page.waitForTimeout(350)
let ids = (await popped()).map((i) => i.id)
check(
  ids.join() === 'context.entry.copy,context.entry.fork',
  `a prompt offers copying and forking (${ids.join(', ')})`,
)

const reply = page.locator('.bubble.assistant').first()
if (await reply.isVisible().catch(() => false)) {
  await reply.click({ button: 'right' })
  await page.waitForTimeout(350)
  ids = (await popped()).map((i) => i.id)
  check(
    ids.join() === 'context.entry.copy',
    `and a reply offers copying alone — a fork cuts in front of what somebody asked (${ids.join(', ')})`,
  )
} else {
  skip('no reply on screen to right-click')
}

// A session already in the list, said to have come from another one. Seeded because the mark in
// the list is the one indicator a run like this cannot otherwise reach: a fork has no record
// until its first turn, and this sends no prompts. The file is put back at the end, the way
// `drive-menu.mjs` puts the recents list back — one key of the shared preferences file, with the
// other four left as they were found.
const userData = await app.evaluate(({ app }) => app.getPath('userData'))
const stateFile = join(userData, 'bravebot-ui.json')
const hadState = existsSync(stateFile) ? readFileSync(stateFile, 'utf8') : null
restoreForks = () => {
  if (hadState === null) rmSync(stateFile, { force: true })
  else writeFileSync(stateFile, hadState, 'utf8')
}
// Any session but the parent — and not one belonging to a bot, which would be seeded with a mark
// and then never drawn, since the sessions list deliberately leaves a bot's session to the bots.
const marked =
  (listed.ok?.sessions ?? []).find(
    (s) => s.id !== parent.id && !theirs.has(`${s.directory}/${s.id}`),
  ) ?? parent
writeFileSync(
  stateFile,
  JSON.stringify({
    ...(hadState === null ? {} : JSON.parse(hadState)),
    forks: [
      {
        child: { directory: marked.directory, id: marked.id },
        parent: { directory: parent.directory, id: parent.id },
        prompt: 0,
        at: Date.now(),
      },
    ],
  }),
  'utf8',
)

// --- the fork a prompt offers on the way past ----------------------------------------------
// The button is in the tree at all times so a keyboard can reach it; what hover changes is
// whether it can be seen. Playwright's `isVisible` does not read opacity, so this asks the
// computed style directly.
const forkOpacity = (nth) =>
  page.evaluate(
    (index) =>
      getComputedStyle(
        document.querySelectorAll('.bubble.user')[index]?.querySelector('.fork-here'),
      ).opacity,
    nth,
  )

check((await page.locator('.bubble.user .fork-here').count()) > 0, 'every prompt carries a fork')
check(
  (await page.locator('.bubble.assistant .fork-here').count()) === 0,
  'and nothing else does — a reply is not a place a fork can be cut',
)
check(Number(await forkOpacity(0)) === 0, 'it stays out of the way until the row is under the pointer')
await page.locator('.bubble.user').first().hover()
await page.waitForTimeout(250)
check(Number(await forkOpacity(0)) === 1, 'and appears when it is')
check(
  (await page.locator('.bubble.user .fork-here').first().getAttribute('aria-label')) ===
    'Fork from here',
  'and says what it is to a reader who cannot see it',
)
await page.screenshot({ path: `${OUT}/19-fork-hover.png` })

// --- forking ----------------------------------------------------------------------------------
// Through the control on the row rather than the menu item, because it is the one people will
// use. That the menu offers the same command is asserted above and in `drive-menu.mjs`; that
// both reach the same action is a single line in `App.tsx`.
//
// The button sits outside the bubble it belongs to, which is only safe because it is a DOM
// child of it: hover follows the tree, not the geometry, so reaching for it does not dismiss
// it. Clicking it here is what holds that.
await page.locator('.bubble.user').nth(1).hover()
await page.locator('.bubble.user').nth(1).locator('.fork-here').click()
await page.waitForTimeout(1200)

check(
  (await page.locator('.composer textarea').inputValue()) === prompts[1].text,
  'the prompt that was cut is waiting in the composer, to be asked differently',
)
check(await page.locator('.fork-banner').isVisible(), 'the fork says where it came from')
check(
  (await page.locator('.fork-banner').textContent())?.includes(parent.title) === true,
  'and names the session it came out of',
)
check(
  (await page.locator('.bubble.user').count()) === 1,
  'the fork holds what was said before that prompt and nothing after it',
)
await page.screenshot({ path: `${OUT}/17-fork.png` })

// --- what the fork left behind -------------------------------------------------------------
const after = (await request('session.list', {})).ok?.sessions ?? []
const parentAfter = after.find((s) => s.id === parent.id && s.directory === parent.directory)
check(!!parentAfter, 'the session that was forked is still in the list')
check(parentAfter?.updated === parentBefore?.updated, 'and nothing about it was rewritten')
check(
  after.length === before.length,
  'a fork with nothing said in it yet has no record — like any new session',
)

const reopened = await request('session.open', { directory: parent.directory, id: parent.id })
check(
  (reopened.ok?.said ?? []).filter((l) => l.kind === 'user').length === prompts.length,
  'and it still has every prompt it had',
)
await request('session.close', { session: reopened.ok?.session })

// --- the lineage the main process keeps -----------------------------------------------------
const lineage = await page.evaluate(() => window.bravebot.readForks())
const line = lineage.find((f) => f.parent.id === parent.id)
check(!!line, 'which session came out of which is written down')
check(line?.prompt === 1, 'along with the prompt it was cut in front of')
check(line?.child.id !== parent.id, 'and the child is a session of its own')
check(
  (await page.evaluate(() => typeof window.bravebot.writeForks)) === 'undefined',
  'the window can read that list and has no way to write to it',
)

// --- the mark in the list ---------------------------------------------------------------------
// Found by the mark rather than by the title: two sessions in a checkout often share the
// first sixty characters of one, which is all a title is.
const markedRows = page.locator('.session:has(.fork-mark)')
check(
  (await markedRows.count()) === 1,
  'exactly one session in the list is marked as having come out of another',
)
check(
  (await markedRows.first().textContent())?.includes(marked.title) === true,
  'and it is the one that did',
)
check(
  (await markedRows.first().locator('.offscreen').textContent())?.trim() === 'Forked.',
  'which says so in words as well as in a mark',
)
await page.screenshot({ path: `${OUT}/20-fork-list.png` })

// --- the link back ---------------------------------------------------------------------------
await page.locator('.fork-banner .link').click()
// Under the mark's own lifetime, so the assertions below — and the shot — see it up.
await page.waitForTimeout(900)
check(
  (await page.locator('.bubble.user').count()) === prompts.length,
  'the banner opens the session it came from, whole',
)
check(
  (await page.locator('.entry-hit.focused').count()) === 1,
  'and marks exactly one row',
)
await page.screenshot({ path: `${OUT}/18-fork-parent.png` })
// The bubble's own text and not everything inside it: the fork it offers on hover lives in
// there too, and `textContent` would carry that glyph along with the prompt.
const landedOn = await page
  .locator('.entry-hit.focused .bubble.user')
  .evaluate((el) => el.firstChild?.textContent ?? '')
check(
  landedOn.trim() === prompts[1].text.trim(),
  'which is the prompt the cut was made in front of',
)

// The mark goes; where it sent the transcript stays. A view that slid back to the bottom a
// second after the link landed would have taken the reader off the row they asked for.
await page.waitForTimeout(1400)
check(
  (await page.locator('.entry-hit.focused').count()) === 0,
  'the mark lets go once it has been seen',
)
check(
  await page.locator('.fork-banner').count() === 0,
  'and the session it points back to carries no banner of its own',
)

await done(`ok — shots in ${OUT}/17-fork.png, ${OUT}/18-fork-parent.png`)
