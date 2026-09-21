// That the folds fold rather than snap — the context panels, and the runs of tool calls
// in the transcript, which share one implementation. Then the row of buttons that decides which
// panels are in the column at all, including that its answer survives a relaunch.
//
// The assertion worth making is the one a screenshot cannot make: that the thing passes
// through heights between full and nothing. A collapse that jumps looks identical in
// stills, so the height is sampled while it moves.
import { _electron as electron } from 'playwright-core'
import { mkdirSync } from 'node:fs'

mkdirSync('/tmp/bravebot-ui', { recursive: true })

const problems = []
const check = (ok, what) => {
  console.log(`${ok ? '  ok  ' : ' FAIL '} ${what}`)
  if (!ok) problems.push(what)
}

const launch = () => electron.launch({ args: ['.'], cwd: process.cwd(), timeout: 40000 })

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

const app = await launch()
const page = await app.firstWindow()
await page.waitForLoadState('domcontentloaded')
await showSessions(page)
page.on('pageerror', (e) => console.log('PAGE ERROR:', e.message))
page.on('console', (m) => m.type() === 'error' && console.log('CONSOLE ERROR:', m.text()))
await page.waitForTimeout(2500)

const sessions = await page.locator('.session').count()
if (sessions === 0) {
  console.log('RESULT: skipped — no sessions to open')
  await app.close()
  process.exit(0)
}

// A run only exists where a turn made two calls in a row, so the sessions are tried in
// order until one has one. Every session has the context panels, so the first will do for
// those; this is only about finding the transcript case.
let withRun = 0
for (let i = 0; i < sessions; i++) {
  await page.locator('.session').nth(i).click()
  await page.waitForTimeout(1200)
  if ((await page.locator('.tool-run').count()) > 0) {
    withRun = i
    break
  }
}
await page.locator('.session').nth(withRun).click()
await page.waitForTimeout(1800)

// Which panels are in the column is a choice somebody makes from the bar, and it is remembered
// between launches and shared with every other driver. So the run starts by putting them all back
// — without this, a panel left off by a previous run has no box to measure and the assertions
// below fail on a window that is behaving perfectly. The same courtesy `drive-columns.mjs` pays
// the columns.
for (let index = 0; index < (await page.locator('.panel-pick').count()); index++) {
  const pick = page.locator('.panel-pick').nth(index)
  if ((await pick.getAttribute('aria-pressed')) === 'false') {
    await pick.click()
    await page.waitForTimeout(200)
  }
}

const fold = page.locator('.panel .fold').first()
const head = page.locator('.panel-head').first()
const height = async () => (await fold.boundingBox()).height

const open = await height()
check(open > 0, `panel starts open (${Math.round(open)}px)`)

/** Click and watch, so a jump and a fold can be told apart. */
async function sample() {
  const seen = []
  const until = Date.now() + 400
  while (Date.now() < until) {
    seen.push(await height())
    await page.waitForTimeout(16)
  }
  return seen
}

await head.click()
const closing = await sample()
const middles = closing.filter((h) => h > 1 && h < open - 1)
check(middles.length > 0, `collapse passes through part-heights (${middles.length} frames)`)
check(closing[closing.length - 1] < 1, 'collapse ends at nothing')
await page.screenshot({ path: '/tmp/bravebot-ui/07-panels-closed.png' })

await head.click()
const opening = await sample()
check(
  opening.filter((h) => h > 1 && h < open - 1).length > 0,
  'expand passes through part-heights',
)
check(Math.abs(opening[opening.length - 1] - open) < 1, 'expand ends back at full height')
await page.screenshot({ path: '/tmp/bravebot-ui/08-panels-open.png' })

// --- and the same fold, around a run of tool calls -------------------------------------
if ((await page.locator('.tool-run').count()) === 0) {
  console.log('  --   no session has two calls in a row; the tool run is untested')
} else {
  const run = page.locator('.tool-run').first()
  const runFold = run.locator('.fold')
  const runHead = run.locator('.tool-run-head')
  const runHeight = async () => (await runFold.boundingBox()).height

  const rows = await run.locator('.tool').count()
  check(rows >= 2, `a run gathers more than one call (${rows})`)
  const label = (await runHead.textContent())?.trim() ?? ''
  check(label.endsWith(`${rows} steps`), `the header counts what it is hiding (${label})`)
  check((await runHead.getAttribute('aria-expanded')) === 'true', 'a run starts open')

  const runOpen = await runHeight()
  await runHead.click()
  const runClosing = []
  const until = Date.now() + 400
  while (Date.now() < until) {
    runClosing.push(await runHeight())
    await page.waitForTimeout(16)
  }
  check(
    runClosing.filter((h) => h > 1 && h < runOpen - 1).length > 0,
    'the run collapses through part-heights',
  )
  check(runClosing[runClosing.length - 1] < 1, 'the run collapses to nothing')
  check((await runHead.getAttribute('aria-expanded')) === 'false', 'and the header says so')
  check(
    (await run.locator('.tool').first().evaluate((el) => getComputedStyle(el).visibility)) ===
      'hidden',
    'a closed run leaves the tab order and the accessibility tree',
  )
  check(await runHead.isVisible(), 'the header stays, so the run can be brought back')
  await page.screenshot({ path: '/tmp/bravebot-ui/11-run-closed.png' })

  await runHead.click()
  await page.waitForTimeout(400)
  check(Math.abs((await runHeight()) - runOpen) < 1, 'the run reopens to its full height')
  await page.screenshot({ path: '/tmp/bravebot-ui/12-run-open.png' })
}

// --- the row of buttons that turns panels off ------------------------------------------
// Folding and turning off are different things and the second one is newer: the bar at the top
// of the column decides which panels are in it at all. The assertion that matters is the last
// one — a panel that comes back has to come back as it was, which is the whole reason it is
// hidden rather than unmounted. A panel that forgot its fold, or a tree that forgot which
// folders were open, would be the bar quietly undoing somebody's work.
const picks = page.locator('.panel-pick')
check((await picks.count()) === 5, `the bar has one button per panel (${await picks.count()})`)

const standing = () => page.locator('.panel:not(.off)').count()
const before = await standing()
check(before === 5, `every panel starts in the column (${before})`)

// Folded first, so there is a state to lose.
const plan = page.locator('#panel-plan')
await plan.locator('.panel-head').click()
await page.waitForTimeout(400)
check((await plan.locator('.panel-head').getAttribute('aria-expanded')) === 'false', 'a panel folds')

await picks.first().click()
await page.waitForTimeout(300)
check((await standing()) === before - 1, 'its button takes it out of the column')
check((await picks.first().getAttribute('aria-pressed')) === 'false', 'and the button says so')
check(
  !(await plan.locator('.panel-head').isVisible()),
  'a panel that is off leaves the tab order and the accessibility tree',
)

await picks.first().click()
await page.waitForTimeout(300)
check((await standing()) === before, 'pressing it again brings the panel back')
check(
  (await plan.locator('.panel-head').getAttribute('aria-expanded')) === 'false',
  'and it comes back folded the way it was left, rather than reset',
)

// Put it back open, because the panels are shared ground with the assertions at the top of this
// file and the next run starts by measuring them.
await plan.locator('.panel-head').click()
await page.waitForTimeout(400)
check((await plan.locator('.panel-head').getAttribute('aria-expanded')) === 'true', 'and unfolds again')
await page.screenshot({ path: '/tmp/bravebot-ui/13-panel-bar.png' })

// --- and that the choice outlives the window -------------------------------------------
// The panel is turned off, the app is restarted, and the column has to come back without it.
// Worth a relaunch rather than a unit assertion because the failure this catches is the one a
// single window cannot see: a preference written to the file and then read back through a
// validator that does not recognise its own output.
await picks.last().click()
await page.waitForTimeout(300)
const hidden = await page.locator('.panel.off').count()
check(hidden === 1, `one panel is off when the window closes (${hidden})`)
await app.close()

const second = await launch()
const relaunched = await second.firstWindow()
await relaunched.waitForLoadState('domcontentloaded')
await relaunched.waitForTimeout(2000)
if (await relaunched.locator('.session').first().isVisible().catch(() => false)) {
  await relaunched.locator('.session').first().click()
  await relaunched.waitForTimeout(1200)
}
check(
  (await relaunched.locator('.panel-pick').last().getAttribute('aria-pressed')) === 'false',
  'the panel that was turned off is still off after a relaunch',
)
check(
  (await relaunched.locator('#panel-files').getAttribute('class'))?.includes('off') === true,
  'and the column came back without it',
)

// Put it back on: every panel on is what the other drivers expect to find, and `drive-tree.mjs`
// has nothing to test at all if this one leaves the file tree turned off.
await relaunched.locator('.panel-pick').last().click()
await relaunched.waitForTimeout(400)
check(
  (await relaunched.locator('.panel:not(.off)').count()) === 5,
  'and it goes back on for whatever runs next',
)
await second.close()
console.log(problems.length ? `\nRESULT: ${problems.length} problem(s)` : '\nRESULT: ok')
process.exit(problems.length ? 1 : 0)
