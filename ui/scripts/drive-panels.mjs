// That the folds fold rather than snap — the context panels, and the runs of tool calls
// in the transcript, which share one implementation. Then the Overview / Files tabs that decide
// which panels the column shows, including that a panel keeps its fold while it is tabbed away.
//
// The assertion worth making is the one a screenshot cannot make: that the thing passes
// through heights between full and nothing. A collapse that jumps looks identical in
// stills, so the height is sampled while it moves.
import { mkdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { installBridge, makeProfile } from './acceptance-fixture.mjs'
import { checker, launchArguments, launchLive, SHOTS_DIR } from './drive-support.mjs'

mkdirSync(SHOTS_DIR, { recursive: true })

/** Launch as `drive-support.mjs` does, on X11 under Linux so it runs under `xvfb-run`. */
const launch = async (userData, env = {}) => {
  const { _electron: electron } = await import('playwright-core')
  const x11 = process.platform === 'linux' ? ['--ozone-platform=x11'] : []
  return electron.launch({ args: [...launchArguments(userData), ...x11], cwd: process.cwd(), timeout: 40000, env: { ...process.env, ...env } })
}

/** Draw the window in one theme for a screenshot. A fresh profile follows the system's. */
const theme = async (page, name) => {
  await page.emulateMedia({ colorScheme: name })
  await page.waitForTimeout(300)
}

const { check, problems } = checker()

await checklist()

const { app, page } = await launchLive(() => launch())
await page.waitForTimeout(2500)

// Not `sessionsOrSkip`, which exits 0 and would drop what the checklist found.
const sessions = await page.locator('.session').count()
if (sessions === 0) {
  console.log('  --   the real profile has no sessions; the folds and the tabs are untested')
  await app.close()
  console.log(problems.length ? `\nRESULT: ${problems.length} problem(s)` : '\nRESULT: ok')
  process.exit(problems.length ? 1 : 0)
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
await page.screenshot({ path: `${SHOTS_DIR}/07-panels-closed.png` })

await head.click()
const opening = await sample()
check(
  opening.filter((h) => h > shut + 1 && h < open - 1).length > 0,
  'expand passes through part-heights',
)
check(Math.abs(opening[opening.length - 1] - open) < 1, 'expand ends back at full height')
await page.screenshot({ path: `${SHOTS_DIR}/08-panels-open.png` })

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
  check(label.includes(`${rows} steps`), `the header counts what it is hiding (${label})`)
  // A saved conversation has no turn running, so every run in it is already put away.
  check(!(await isOpen(runFold)), 'a finished run starts folded')
  await runHead.click()
  await page.waitForTimeout(400)
  check(await isOpen(runFold), 'and opens from its header')

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
  await page.screenshot({ path: `${SHOTS_DIR}/11-run-closed.png` })

  await runHead.click()
  await page.waitForTimeout(400)
  check(Math.abs((await runHeight()) - runOpen) < 1, 'the run reopens to its full height')
  await page.screenshot({ path: `${SHOTS_DIR}/12-run-open.png` })
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
await page.screenshot({ path: `${SHOTS_DIR}/13-panel-tabs.png` })

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

// --- the Choose sections checklist (PANEL-20) --------------------------------------------
// Run before the rest, which skips when the real profile has no sessions. Against a user-data
// directory and agent state of its own and the acceptance drivers' bridge, so turning sections off
// and reading the file back never touches the person's own `bravebot-ui.json` or records.
async function checklist() {
const { profile: userData, directory, state } = makeProfile()
const sections = page2 => page2.locator('[data-test="context-sections"]')
const row = (page2, name) => page2.locator(`leo-menu-item[data-panel="${name}"]`)
const shown = (page2, name) => page2.locator(`#panel-${name}`).isVisible()
const hidden = (page2, name) => page2.locator(`#panel-${name}.off`).count().then((n) => n === 1)
const gutter = (page2) => page2.getByRole('separator', { name: 'Resize the context panel' })
const width = async (page2) => Number(await gutter(page2).getAttribute('aria-valuenow'))
const folded = async (page2) => (await page2.locator('.app.right-folded').count()) > 0
const stored = () => JSON.parse(readFileSync(join(userData, 'bravebot-ui.json'), 'utf8'))
const HIDEABLE = ['plan', 'read', 'writes', 'confined', 'notes', 'asides']

/**
 * Launch on the fixture profile with the fixture bridge through `launchLive`, and open its first
 * conversation. The column is unfolded only when asked, so a relaunch can check the fold it finds.
 */
const openFixture = async ({ unfold = true } = {}) => {
  // The window opens 900px wide under a virtual display, and the app folds the column of any
  // window 1120px wide or less at load and stores that fold. Widening it as it is made keeps the
  // fold the relaunch is checking the one this script left.
  const { app, page } = await launchLive(async () => {
    const app = await launch(userData, { BRAVEBOT_HOME: state })
    await app.evaluate(({ app: electronApp, BrowserWindow }) => {
      for (const w of BrowserWindow.getAllWindows()) w.setSize(1350, 900)
      electronApp.on('browser-window-created', (_, w) => w.setSize(1350, 900))
    })
    return app
  })
  await page.setViewportSize({ width: 1350, height: 900 })
  await installBridge(app, { directory })
  // The fixture answers every method it does not know with `{}`, which the composer's spend limit
  // and variant picker cannot draw, so those two are answered here and the rest are passed on.
  await app.evaluate(({ ipcMain }) => {
    const fixture = ipcMain._invokeHandlers.get('bravebot:request')
    ipcMain.removeHandler('bravebot:request')
    ipcMain.handle('bravebot:request', (event, method, params) =>
      method === 'session.limit' ? { ok: { limit: null, spent: 0 } }
        : method === 'session.variant' ? { error: { message: 'No variants in this fixture.' } }
          : fixture(event, method, params))
  })
  await page.reload()
  await page.locator('.session').filter({ hasText: 'Review the sample project' }).click()
  await page.waitForTimeout(1200)
  if (unfold && (await folded(page))) await page.getByRole('button', { name: 'Context panel', exact: true }).click()
  await page.waitForTimeout(600)
  return { app, page }
}

let before
{
  const { app, page } = await openFixture()

  // Move the column off its default width, so the relaunch has a width of its own to keep.
  const start = await width(page)
  await gutter(page).focus()
  await page.keyboard.press('Shift+ArrowLeft')
  await page.waitForTimeout(300)
  check((await width(page)) !== start, `the context column's width moves from ${start} to ${await width(page)}`)

  check(await sections(page).isVisible(), 'the context head has a Choose sections button')
  check(await shown(page, 'notes'), 'Notes shows before anything is turned off')
  await sections(page).click()
  await page.waitForTimeout(400)
  check(await row(page, 'notes').isVisible(), 'the button opens the checklist')
  await theme(page, 'light')
  await page.screenshot({ path: `${SHOTS_DIR}/14-sections-menu-light.png` })
  await theme(page, 'dark')
  await page.screenshot({ path: `${SHOTS_DIR}/14-sections-menu-dark.png` })
  await theme(page, 'light')

  await row(page, 'notes').click()
  await page.waitForTimeout(300)
  check(await hidden(page, 'notes'), 'unticking Notes hides #panel-notes at once')
  check(!(await shown(page, 'notes')), 'and Notes is no longer drawn')
  check(await row(page, 'plan').isVisible(), 'and the checklist stays open')
  check((await row(page, 'notes').getAttribute('aria-checked')) === 'false', 'and Notes reads as unticked')
  await page.screenshot({ path: `${SHOTS_DIR}/15-sections-notes-off-light.png` })
  await theme(page, 'dark')
  await page.screenshot({ path: `${SHOTS_DIR}/15-sections-notes-off-dark.png` })
  await theme(page, 'light')

  await page.keyboard.press('Escape')
  await page.waitForTimeout(400)
  check(!(await row(page, 'plan').isVisible()), 'Escape closes the checklist')
  check(await hidden(page, 'notes'), 'and keeps Notes hidden')
  before = {
    start,
    width: await width(page),
    folded: await folded(page),
    box: (await page.locator('#context-column').boundingBox()).width,
  }
  await page.waitForTimeout(500)
  await app.close()
}

check(JSON.stringify(stored().hidden) === '["notes"]', `bravebot-ui.json holds "hidden": ${JSON.stringify(stored().hidden)}`)

{
  const { app, page } = await openFixture({ unfold: false })
  check(await hidden(page, 'notes'), 'after a relaunch Notes is still hidden')
  check(await shown(page, 'plan'), 'and Plan shows')
  check((await folded(page)) === before.folded, `and the column's fold is as it was (folded: ${before.folded})`)
  check((await width(page)) === before.width, `and the column's width is as it was (${await width(page)} of ${before.width})`)
  const box = (await page.locator('#context-column').boundingBox()).width
  check(Math.abs(box - before.box) < 1, `and the column is drawn as wide as before (${Math.round(box)}px of ${Math.round(before.box)}px)`)

  // Every section off: the column keeps its head, the button and one line saying so.
  await sections(page).click()
  await page.waitForTimeout(400)
  for (const name of HIDEABLE.filter((n) => n !== 'notes')) {
    await row(page, name).click()
    await page.waitForTimeout(150)
  }
  await page.keyboard.press('Escape')
  await page.waitForTimeout(400)
  check((await page.locator('.panel:not(.off)').count()) === 0, 'every section off leaves no panel drawn')
  check(await page.locator('.context-head').isVisible(), 'and keeps the column head')
  check(await page.locator('#context-all-off').isVisible(), 'and draws the line saying every section is off')
  check(await sections(page).isVisible(), 'and keeps the Choose sections button')
  check(await page.locator('#context-column').isVisible(), 'and the column stays drawn')
  check(!(await folded(page)), 'and the column is not folded')
  await page.screenshot({ path: `${SHOTS_DIR}/16-sections-all-off-light.png` })
  await theme(page, 'dark')
  await page.screenshot({ path: `${SHOTS_DIR}/16-sections-all-off-dark.png` })
  await theme(page, 'light')

  // Put the profile back as it started: every section on and the column at its default width.
  await sections(page).click()
  await page.waitForTimeout(400)
  for (const name of HIDEABLE) {
    await row(page, name).click()
    await page.waitForTimeout(150)
  }
  await page.keyboard.press('Escape')
  await page.waitForTimeout(400)
  await gutter(page).focus()
  await page.keyboard.press('Home')
  await page.waitForTimeout(500)
  const back = []
  for (const name of HIDEABLE) if (await hidden(page, name)) back.push(name)
  check(back.length === 0, `ticking every section again turns every panel back on (still off: ${back.join(', ') || 'none'})`)
  check(!(await page.locator('#context-all-off').isVisible()), 'and takes away the every-section-off line')
  await page.waitForTimeout(500)
  await app.close()
}

check(JSON.stringify(stored().hidden) === '[]', `the restored bravebot-ui.json holds "hidden": ${JSON.stringify(stored().hidden)}`)
check(
  stored().layout.right === before.start && stored().layout.collapsed.right === false,
  `and the column back at ${stored().layout.right}px of ${before.start}px, unfolded`,
)
}
