// That the side columns fold away and come back, one at a time.
//
// Two things here cannot be seen in a screenshot and are the reason this exists. One is
// that the fold is a fold: a column that jumps from 340px to nothing looks identical in
// stills, so the width is sampled while it moves. The other is that a column comes back
// where it was rather than at the default — which only shows up if the column was dragged
// somewhere non-default first, so the run starts by doing exactly that.
import { _electron as electron } from 'playwright-core'
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

mkdirSync('/tmp/bravebot-ui', { recursive: true })

const problems = []
const check = (ok, what) => {
  console.log(`${ok ? '  ok  ' : ' FAIL '} ${what}`)
  if (!ok) problems.push(what)
}

const launch = () => electron.launch({ args: ['.'], cwd: process.cwd(), timeout: 40000 })

/** Whichever columns are folded, unfolded — the state on disk outlives any one run. */
async function unfoldAll(page) {
  for (const side of ['left', 'right']) {
    const toggle = page.locator(`.fold-toggle.${side}`)
    if ((await toggle.getAttribute('aria-expanded')) === 'false') {
      await toggle.click()
      await page.waitForTimeout(300)
    }
  }
}

const ready = async (app, { fresh = false } = {}) => {
  const page = await app.firstWindow()
  await page.waitForLoadState('domcontentloaded')
  page.on('pageerror', (e) => console.log('PAGE ERROR:', e.message))
  page.on('console', (m) => m.type() === 'error' && console.log('CONSOLE ERROR:', m.text()))
  await page.waitForTimeout(1800)
  // The layout is remembered between launches and shared with the other drivers, so a run
  // starts by putting the columns back rather than assuming it inherited them open. The
  // relaunch checks opt out: what they are testing is precisely what was inherited.
  if (fresh) await unfoldAll(page)
  // Opened so the header under test is the one with something in it. The empty state has
  // the same bar, and folding it away there is exactly the trap the bar exists to avoid.
  //
  // Visibility, not count: on a relaunch that starts folded the sessions are still in the
  // document and still unclickable, which is the point of the fold.
  if (await page.locator('.session').first().isVisible().catch(() => false)) {
    await page.locator('.session').first().click()
    await page.waitForTimeout(1200)
  }
  return page
}

const box = async (page, selector) => (await page.locator(selector).boundingBox()) ?? { width: 0, x: 0, y: 0 }
const width = async (page, selector) => (await box(page, selector)).width

/** Drag a divider by `dx`, the way a hand would. */
async function drag(page, which, dx) {
  const bounds = await page.locator('.gutter').nth(which).boundingBox()
  const y = bounds.y + bounds.height / 2
  const x = bounds.x + bounds.width / 2
  await page.mouse.move(x, y)
  await page.mouse.down()
  await page.mouse.move(x + dx, y, { steps: 12 })
  await page.mouse.up()
  await page.waitForTimeout(150)
}

/** Watch something move, so a fold and a jump can be told apart. */
async function sample(page, read) {
  const seen = []
  const until = Date.now() + 400
  while (Date.now() < until) {
    seen.push(await read())
    await page.waitForTimeout(16)
  }
  return seen
}

let app = await launch()
let page = await ready(app, { fresh: true })

const userData = await app.evaluate(({ app }) => app.getPath('userData'))

// --- the controls --------------------------------------------------------------------
check((await page.locator('.fold-toggle').count()) === 2, 'both fold toggles are present')
check(
  (await page.locator('.fold-toggle.left').getAttribute('aria-expanded')) === 'true' &&
    (await page.locator('.fold-toggle.right').getAttribute('aria-expanded')) === 'true',
  'both columns start expanded',
)

// --- a width worth remembering -------------------------------------------------------
// Measured rather than assumed: this app remembers its layout between runs, so the width
// the window opens at is whatever the last driver left behind.
const started = await width(page, '.sessions')
await drag(page, 0, 60)
const dragged = await width(page, '.sessions')
check(
  Math.abs(dragged - (started + 60)) <= 4,
  `the session list was dragged somewhere non-default (${Math.round(started)} -> ${Math.round(dragged)}px)`,
)

// --- folding -------------------------------------------------------------------------
const centreBefore = await width(page, '.transcript')
await page.locator('.fold-toggle.left').click()
const closing = await sample(page, () => width(page, '.sessions'))
check(
  closing.filter((w) => w > 1 && w < dragged - 1).length > 0,
  `the fold passes through part-widths (${closing.filter((w) => w > 1 && w < dragged - 1).length} frames)`,
)
check(closing[closing.length - 1] < 1, 'the fold ends at nothing')
check(
  (await page.locator('.fold-toggle.left').getAttribute('aria-expanded')) === 'false',
  'the toggle reports the column collapsed',
)
check(
  (await page.locator('.sessions').evaluate((el) => getComputedStyle(el).visibility)) === 'hidden',
  'a folded column leaves the tab order and the accessibility tree',
)
check(
  Math.abs((await width(page, '.transcript')) - (centreBefore + dragged)) <= 3,
  'the transcript took exactly the space that was freed',
)

// The drivers either side of this one count on the shape of the layout, not just its
// widths: a folded column is zeroed, never unmounted.
check((await page.locator('.sessions, .transcript, .context').count()) === 3, 'all three columns are still rendered')
check((await page.locator('.gutter').count()) === 2, 'both dividers are still rendered')
await page.screenshot({ path: '/tmp/bravebot-ui/09-left-folded.png' })

// --- the seam beside a folded column does nothing -------------------------------------
const seam = page.locator('.gutter').first()
check((await seam.getAttribute('tabindex')) === '-1', 'the folded column’s divider is not focusable')
check(
  (await seam.evaluate((el) => getComputedStyle(el).cursor)) === 'default',
  'the folded column’s divider does not offer a resize cursor',
)
await seam.dblclick({ force: true })
await page.waitForTimeout(200)
check((await width(page, '.sessions')) < 1, 'double-clicking it does not bring the column back')

// --- the traffic lights, at every frame and not just at the ends ----------------------
const lefts = await sample(page, async () => {
  const b = await box(page, '.fold-toggle.left')
  return b.y < 40 ? b.x : 999
})
check(Math.min(...lefts) >= 70, `the left toggle clears the traffic lights throughout (min x ${Math.round(Math.min(...lefts))})`)

// --- and back ------------------------------------------------------------------------
await page.locator('.fold-toggle.left').click()
const opening = await sample(page, () => width(page, '.sessions'))
check(opening.filter((w) => w > 1 && w < dragged - 1).length > 0, 'the unfold passes through part-widths')
check(
  Math.abs(opening[opening.length - 1] - dragged) <= 3,
  `the column comes back where it was, not at the default (${Math.round(opening[opening.length - 1])} vs ${Math.round(dragged)})`,
)

// --- focus stays on the button it was on ----------------------------------------------
await page.locator('.fold-toggle.right').focus()
await page.keyboard.press('Enter')
await page.waitForTimeout(300)
check(
  await page.evaluate(() => document.activeElement?.classList.contains('fold-toggle') ?? false),
  'the toggle keeps focus through a fold, so a second press undoes the first',
)
check((await width(page, '.context')) < 1, 'the right column folded independently')
check(Math.abs((await width(page, '.sessions')) - dragged) <= 3, 'and the left column did not move')
await page.keyboard.press('Enter')
await page.waitForTimeout(300)

// --- a narrow window -------------------------------------------------------------------
await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].setBounds({ width: 900 }))
await page.waitForTimeout(300)
await page.locator('.fold-toggle.left').click()
await page.locator('.fold-toggle.right').click()
await page.waitForTimeout(400)
await page.locator('.fold-toggle.left').click()
await page.waitForTimeout(300)
await page.locator('.fold-toggle.right').click()
await page.waitForTimeout(400)
const narrow = {
  left: await width(page, '.sessions'),
  right: await width(page, '.context'),
  centre: await width(page, '.transcript'),
}
check(narrow.centre >= 380, `the transcript keeps its floor at 900px (${Math.round(narrow.centre)} >= 380)`)
check(narrow.left >= 200, `the session list comes back no smaller than its minimum (${Math.round(narrow.left)} >= 200)`)
check(narrow.right >= 240, `the context panel comes back no smaller than its minimum (${Math.round(narrow.right)} >= 240)`)
await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].setBounds({ width: 1280 }))
await page.waitForTimeout(300)

// --- what gets remembered ---------------------------------------------------------------
await page.locator('.fold-toggle.left').click()
await page.waitForTimeout(400)
await app.close()

app = await launch()
page = await ready(app)
check((await width(page, '.sessions')) < 1, 'a folded column is still folded after a relaunch')
check(
  (await page.locator('.fold-toggle.left').getAttribute('aria-expanded')) === 'false',
  'and the toggle says so',
)
await page.locator('.fold-toggle.left').click()
await page.waitForTimeout(400)
check(
  (await width(page, '.sessions')) >= 200,
  'the width behind the fold survived the relaunch too',
)
await app.close()

// --- a stored layout written before folding existed ---------------------------------------
// Written into the one file the app keeps, under its own key and leaving the other keys alone —
// the same read-modify-write the main process does, because clobbering somebody's recents list
// to test a column width would be a driver doing more than it said.
{
  const file = join(userData, 'bravebot-ui.json')
  const held = existsSync(file) ? JSON.parse(readFileSync(file, 'utf8')) : {}
  writeFileSync(file, JSON.stringify({ ...held, layout: { left: 300, right: 300 } }), 'utf8')
}
app = await launch()
page = await ready(app, { fresh: true })
const old = { left: await width(page, '.sessions'), right: await width(page, '.context') }
check(
  Math.abs(old.left - 300) <= 2 && Math.abs(old.right - 300) <= 2,
  `a layout file with no folds in it still opens both columns (${Math.round(old.left)}, ${Math.round(old.right)})`,
)
await page.screenshot({ path: '/tmp/bravebot-ui/10-columns.png' })

// The layout file is shared with the other drivers, so this one puts it back the way it
// found it. A column left folded here would make their opening measurements nonsense.
await page.locator('.gutter').first().dblclick()
await page.locator('.gutter').nth(1).dblclick()
await page.waitForTimeout(300)
await app.close()

if (problems.length > 0) {
  console.log(`\nRESULT: ${problems.length} failed`)
  process.exit(1)
}
console.log('\nRESULT: ok — shots in /tmp/bravebot-ui/09-left-folded.png, 10-columns.png')
