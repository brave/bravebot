// That the folds fold rather than snap — the context panels, and the runs of tool calls
// in the transcript, which share one implementation. Then the Overview / Files tabs that decide
// which panels the column shows, including that a panel keeps its fold while it is tabbed away.
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
    .locator('.sidebar-tabs [role="option"]')
    .first()
    .waitFor({ state: 'visible', timeout: 15000 })
    .catch(() => undefined)
  await page
    .locator('.sidebar-tabs [role="option"]')
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

// Leo's Collapse is a <details> in a shadow root, so its state is read off the element it draws
// and its movement is the height of the whole disclosure: heading plus whatever is open under it.
const isOpen = (collapse) => collapse.evaluate((el) => el.shadowRoot.querySelector('details').open)
const fold = page.locator('.panel-collapse').first()
const head = fold.locator('summary')
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
const shut = closing[closing.length - 1]
const middles = closing.filter((h) => h > shut + 1 && h < open - 1)
check(middles.length > 0, `collapse passes through part-heights (${middles.length} frames)`)
check(shut < open - 20, `collapse ends at just the heading (${Math.round(shut)}px of ${Math.round(open)}px)`)
await page.screenshot({ path: '/tmp/bravebot-ui/07-panels-closed.png' })

await head.click()
const opening = await sample()
check(
  opening.filter((h) => h > shut + 1 && h < open - 1).length > 0,
  'expand passes through part-heights',
)
check(Math.abs(opening[opening.length - 1] - open) < 1, 'expand ends back at full height')
await page.screenshot({ path: '/tmp/bravebot-ui/08-panels-open.png' })

// --- and the same fold, around a run of tool calls -------------------------------------
if ((await page.locator('.tool-run').count()) === 0) {
  console.log('  --   no session has two calls in a row; the tool run is untested')
} else {
  const run = page.locator('.tool-run').first()
  const runFold = run.locator('.tool-run-collapse')
  const runHead = runFold.locator('summary')
  const runHeight = async () => (await runFold.boundingBox()).height

  const rows = await run.locator('.tool').count()
  check(rows >= 2, `a run gathers more than one call (${rows})`)
  const label = (await runHead.innerText())?.trim() ?? ''
  check(label.endsWith(`${rows} steps`), `the header counts what it is hiding (${label})`)
  check(await isOpen(runFold), 'a run starts open')

  const runOpen = await runHeight()
  await runHead.click()
  const runClosing = []
  const until = Date.now() + 400
  while (Date.now() < until) {
    runClosing.push(await runHeight())
    await page.waitForTimeout(16)
  }
  const runShut = runClosing[runClosing.length - 1]
  check(
    runClosing.filter((h) => h > runShut + 1 && h < runOpen - 1).length > 0,
    'the run collapses through part-heights',
  )
  check(runShut < runOpen - 20, 'the run collapses to just its heading')
  check(!(await isOpen(runFold)), 'and the header says so')
  check(
    !(await run.locator('.tool').first().isVisible()),
    'a closed run leaves the tab order and the accessibility tree',
  )
  check(await runHead.isVisible(), 'the header stays, so the run can be brought back')
  await page.screenshot({ path: '/tmp/bravebot-ui/11-run-closed.png' })

  await runHead.click()
  await page.waitForTimeout(400)
  check(Math.abs((await runHeight()) - runOpen) < 1, 'the run reopens to its full height')
  await page.screenshot({ path: '/tmp/bravebot-ui/12-run-open.png' })
}

// --- the tabs that decide which panels the column shows -------------------------------
// Overview holds the four panels derived from the transcript; Files holds the tree. Switching is
// not a fold: the tab hides the other group with `display: none` rather than unmounting it. The
// assertion that matters is the last one — a panel that comes back has to come back as it was,
// which is the whole reason it is hidden rather than unmounted. A panel that forgot its fold, or
// a tree that forgot which folders were open, would be the tabs quietly undoing somebody's work.
const tabs = page.locator('[data-test="inspector-tabs"] [role="tab"]')
check((await tabs.count()) === 2, `the column offers two tabs (${await tabs.count()})`)

const standing = () => page.locator('.panel:not(.off)').count()
check((await standing()) === 4, `Overview shows its four panels (${await standing()})`)
check(
  (await page.locator('#panel-files').getAttribute('class'))?.includes('off') === true,
  'and keeps the file tree out of the column',
)

// Folded first, so there is a state to lose.
const plan = page.locator('#panel-plan')
await plan.locator('summary').click()
await page.waitForTimeout(400)
check(!(await isOpen(plan.locator('.panel-collapse'))), 'a panel folds')

await page.getByRole('tab', { name: 'Files', exact: true }).click()
await page.waitForTimeout(300)
check((await standing()) === 0, 'the Files tab takes the overview panels out of the column')
check(
  (await page.locator('#panel-files').getAttribute('class'))?.includes('off') === false,
  'and brings the file tree in',
)
check(
  !(await plan.locator('summary').isVisible()),
  'a panel that is tabbed away leaves the tab order and the accessibility tree',
)
await page.screenshot({ path: '/tmp/bravebot-ui/13-panel-tabs.png' })

await page.getByRole('tab', { name: 'Overview', exact: true }).click()
await page.waitForTimeout(300)
check((await standing()) === 4, 'going back to Overview brings the panels back')
check(
  !(await isOpen(plan.locator('.panel-collapse'))),
  'and it comes back folded the way it was left, rather than reset',
)

// Put it back open, because the panels are shared ground with the assertions at the top of this
// file and the next run starts by measuring them.
await plan.locator('summary').click()
await page.waitForTimeout(400)
check(await isOpen(plan.locator('.panel-collapse')), 'and unfolds again')
await app.close()
console.log(problems.length ? `\nRESULT: ${problems.length} problem(s)` : '\nRESULT: ok')
process.exit(problems.length ? 1 : 0)
