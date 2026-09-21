// Launch the app and poke it, so a change can be seen rather than inferred.
// macOS has a real display, so no xvfb: this drives the actual window.
import { _electron as electron } from 'playwright-core'
import { existsSync, mkdirSync } from 'node:fs'

const shots = '/tmp/bravebot-ui'
mkdirSync(shots, { recursive: true })

const app = await electron.launch({
  args: ['.'],
  cwd: process.cwd(),
  timeout: 40000,
})

const page = await app.firstWindow()
await page.waitForLoadState('domcontentloaded')

// Surface renderer errors here rather than letting them vanish into the window.
page.on('console', (m) => m.type() === 'error' && console.log('CONSOLE ERROR:', m.text()))
page.on('pageerror', (e) => console.log('PAGE ERROR:', e.message))

// The session list is populated by an async round-trip to bravebot-rpc.
await page.waitForTimeout(2500)

const sessions = await page.locator('.session').count()
const columns = await page.locator('.sessions, .transcript, .context').count()
console.log(`columns rendered : ${columns}/3`)
console.log(`sessions listed  : ${sessions}`)

if (sessions > 0) {
  const titles = await page.locator('.session-title').allTextContents()
  const where = await page.locator('.session-where').allTextContents()
  titles.forEach((t, i) => console.log(`  - ${t.slice(0, 54)}  [${where[i]}]`))
}

const build = await page.locator('.build').textContent().catch(() => null)
console.log(`agent build      : ${build ?? '(none shown)'}`)

await page.screenshot({ path: `${shots}/01-launched.png`, fullPage: false })

// The filter box. Driven before anything is opened, and left empty, because every other
// driver clicks `.session` first and expects that to be the newest session.
if (sessions > 0) {
  const box = page.locator('.session-find')
  const first = (await page.locator('.session-title').first().textContent()) ?? ''
  // A word from the newest session's title, long enough not to be in every other one.
  const word = first.split(/\s+/).find((w) => w.length > 4) ?? first.slice(0, 6)
  // Read before anything is filtered: the project is the first field of the secondary line.
  const firstWhere = (await page.locator('.session-where').first().textContent()) ?? ''

  await box.fill(word)
  await page.waitForTimeout(200)
  const byTitle = await page.locator('.session').count()
  const kept = await page.locator('.session-title').first().textContent()
  console.log(`filter "${word}"`.padEnd(17) + `: ${byTitle}/${sessions} rows, first is ${kept === first ? 'the same' : 'DIFFERENT'}`)

  // A project, which no title need contain: the secondary line has to be searchable too, or
  // "which session was that, in bravebot?" has no answer here.
  const project = firstWhere.split(' · ')[0] ?? ''
  await box.fill(project)
  await page.waitForTimeout(200)
  const byProject = await page.locator('.session').count()
  const lines = await page.locator('.session-where').allTextContents()
  const allInIt = lines.every((line) => line.startsWith(project))
  console.log(`filter "${project}"`.padEnd(17) + `: ${byProject}/${sessions} rows, all in that project: ${allInIt}`)

  await box.fill('zzzznothingmatchesthis')
  await page.waitForTimeout(200)
  const none = await page.locator('.session').count()
  const empty = (await page.locator('.empty').textContent().catch(() => '')) ?? ''
  console.log(`filter with junk : ${none} rows, says ${empty.includes('matches') ? '"no match"' : `"${empty.slice(0, 30)}"`}`)
  await page.screenshot({ path: `${shots}/01-filtered.png` })

  // Escape clears it, and the whole list comes back.
  await box.press('Escape')
  await page.waitForTimeout(200)
  const back = await page.locator('.session').count()
  console.log(`after Escape     : ${back}/${sessions} rows${back === sessions ? '' : '  FAIL'}`)

  // Grouping by checkout. The preference is remembered in a file that outlives the run, so
  // this starts by putting the column flat rather than assuming it inherited it that way,
  // and puts it back flat at the end — the drivers share persisted state, and one that left
  // the list grouped would change what the next one's `.session` counts mean.
  const toggle = page.locator('.session-group')
  const pressed = async () => (await toggle.getAttribute('aria-pressed')) === 'true'
  if (await pressed()) {
    await toggle.click()
    await page.waitForTimeout(200)
  }
  const flatHeads = await page.locator('.session-group-head').count()
  console.log(`flat list        : ${flatHeads} headings${flatHeads === 0 ? '' : '  FAIL'}`)

  await toggle.click()
  await page.waitForTimeout(200)
  const heads = await page.locator('.session-group-head').count()
  const grouped = await page.locator('.session').count()
  console.log(
    `grouped          : ${heads} headings over ${grouped}/${sessions} rows` +
      `${(await pressed()) && heads > 0 && grouped === sessions ? '' : '  FAIL'}`
  )

  // Every row is under a heading, and each heading's count is the number of rows beneath
  // it. Counted per section rather than by walking the list, because a heading whose badge
  // disagrees with its own contents is the failure worth catching here.
  const tallies = await page
    .locator('.session-group-section')
    .evaluateAll((sections) =>
      sections.map((section) => ({
        said: Number(section.querySelector('.count')?.textContent ?? -1),
        rows: section.querySelectorAll('.session').length,
      }))
    )
  const honest = tallies.every((t) => t.said === t.rows)
  const covered = tallies.reduce((sum, t) => sum + t.rows, 0)
  console.log(
    `headings honest  : ${tallies.filter((t) => t.said === t.rows).length}/${tallies.length}, ` +
      `covering ${covered}/${sessions} rows${honest && covered === sessions ? '' : '  FAIL'}`
  )
  await page.screenshot({ path: `${shots}/01-grouped.png` })

  // A heading folds its own group away. `Fold` keeps the rows mounted so the collapse has
  // something to animate, so this counts what is *visible* rather than what is in the
  // document — the same distinction `drive-columns.mjs` makes about a folded column.
  // The fullest group rather than the first, which on a fresh checkout is often a group of
  // one — a fold that hides a single row proves much less than one that hides eight.
  const biggest = tallies.reduce((best, t, i) => (t.rows > (tallies[best]?.rows ?? 0) ? i : best), 0)
  const firstHead = page.locator('.session-group-head').nth(biggest)
  const inFirst = Number(await firstHead.locator('.count').textContent())
  // Held by name, not by position: filtering drops and reorders the sections, so an `nth`
  // taken now would be pointing at somebody else's group by the time the query is typed.
  const groupName = (await firstHead.locator('.session-group-name').textContent()) ?? ''
  const firstSection = page
    .locator('.session-group-section')
    .filter({ has: page.locator('.session-group-name', { hasText: new RegExp(`^${groupName}$`) }) })
  await firstHead.click()
  await page.waitForTimeout(400)
  const stillVisible = await firstSection.locator('.session:visible').count()
  const headStays = await firstHead.isVisible()
  console.log(
    `group collapsed  : ${stillVisible}/${inFirst} rows visible, heading still shown: ${headStays}` +
      `${stillVisible === 0 && headStays && (await firstHead.locator('.session-group-fold').getAttribute('aria-expanded')) === 'false' ? '' : '  FAIL'}`
  )
  // The others are untouched: folding one group is not folding the list.
  const elsewhere = await page.locator('.session:visible').count()
  console.log(
    `others untouched : ${elsewhere}/${sessions - inFirst} rows still visible` +
      `${elsewhere === sessions - inFirst ? '' : '  FAIL'}`
  )
  await page.screenshot({ path: `${shots}/01-collapsed.png` })

  // A query reaches into a folded group. A heading with nothing under it is the opposite of
  // what somebody who just typed a search asked for.
  const hidden = (await firstSection.locator('.session-title').first().textContent()) ?? ''
  const hiddenWord = hidden.split(/\s+/).find((w) => w.length > 4) ?? hidden.slice(0, 6)
  await box.fill(hiddenWord)
  await page.waitForTimeout(400)
  const found = await firstSection.locator('.session:visible').count()
  console.log(
    `search reaches in: ${found} row${found === 1 ? '' : 's'} visible in the folded group` +
      `${found > 0 ? '' : '  FAIL'}`
  )
  await box.press('Escape')
  await page.waitForTimeout(400)
  const refolded = await firstSection.locator('.session:visible').count()
  console.log(
    `fold survives it : ${refolded} rows visible after clearing${refolded === 0 ? '' : '  FAIL'}`
  )

  await firstHead.click()
  await page.waitForTimeout(400)
  const reopened = await page.locator('.session:visible').count()
  console.log(
    `group reopened   : ${reopened}/${sessions} rows visible${reopened === sessions ? '' : '  FAIL'}`
  )

  // Filtering and grouping compose: the headings left are the ones the query names, and
  // none of them is empty — a group whose rows all went is a heading with nothing under it.
  //
  // Not "exactly one heading", which is what this asked for until a machine had both
  // `bravebot` and `bravebot-ui` checked out. The query is the first row's project name,
  // and a name is free to be a prefix of another; two headings there is the filter being
  // right, not wrong. Counting them was reading the driver's own choice of query back as a
  // fact about the list.
  await box.fill(project)
  await page.waitForTimeout(200)
  const narrowed = await page.locator('.session-group-section').evaluateAll((sections) =>
    sections.map((section) => ({
      path: section.querySelector('.session-group-fold')?.getAttribute('title') ?? '',
      rows: section.querySelectorAll('.session').length,
    }))
  )
  const named = narrowed.every((section) => section.path.includes(project))
  const peopled = narrowed.every((section) => section.rows > 0)
  console.log(
    `grouped + filter : ${narrowed.length} heading${narrowed.length === 1 ? '' : 's'} for "${project}"` +
      `, all named by it: ${named}, none empty: ${peopled}` +
      `${narrowed.length > 0 && named && peopled ? '' : '  FAIL'}`
  )
  await box.press('Escape')
  await page.waitForTimeout(200)

  // The plus on a heading starts a session in that checkout, with no folder picker in the
  // way. Safe to press: the bridge writes nothing until the first turn, so an
  // opened-and-abandoned session leaves no record behind for the next driver to trip over.
  //
  // Driven against a checkout that is still on disk. The list remembers projects that have
  // since been deleted or moved, and the bridge rightly refuses those with `not_a_directory`
  // — a real answer, but not the one this assertion is about.
  const paths = await page.locator('.session-group-fold').evaluateAll((heads) =>
    heads.map((head) => head.getAttribute('title') ?? '')
  )
  const where = paths.find((path) => path && existsSync(path))
  if (!where) {
    console.log('plus starts one  : skipped, no listed checkout is still on disk')
  } else {
    const head = page
      .locator('.session-group-head')
      .filter({ has: page.locator(`.session-group-fold[title="${where}"]`) })
    await head.locator('.session-group-new').click()
    await page.waitForTimeout(900)
    const asked = (await page.locator('.trust .path').textContent().catch(() => null)) ?? ''
    console.log(
      `plus starts one  : asks about ${asked || '(nothing)'}${asked === where ? '' : '  FAIL'}`
    )
  }
  await page.screenshot({ path: `${shots}/01-group-new.png` })
  // Declined rather than trusted: this is not a session anybody meant to keep, and saying
  // yes here would be answering a question about somebody's real checkout on their behalf.
  if (await page.locator('.trust').isVisible().catch(() => false)) {
    await page.locator('.trust-actions .decline').click()
    await page.waitForTimeout(400)
  }

  // Back to flat, for the next driver as much as for this assertion.
  await toggle.click()
  await page.waitForTimeout(200)
  const restored = await page.locator('.session-group-head').count()
  const rows = await page.locator('.session').count()
  console.log(
    `back to flat     : ${restored} headings, ${rows}/${sessions} rows` +
      `${restored === 0 && rows === sessions ? '' : '  FAIL'}`
  )
}

// Open the newest session and let the transcript fill.
if (sessions > 0) {
  await page.locator('.session').first().click()
  await page.waitForTimeout(1800)
  const bubbles = await page.locator('.bubble, .tool.replayed').count()
  console.log(`transcript rows  : ${bubbles}`)
  const head = await page.locator('.transcript-head h1').textContent().catch(() => null)
  console.log(`opened           : ${head}`)
  await page.screenshot({ path: `${shots}/02-session.png` })
}

// Tooltips. Asserted as attributes rather than seen, because the popup itself is drawn by
// the OS and never lands in a screenshot. What is being checked is that the controls a
// screenshot *cannot* explain — a 1px divider, an icon, a path clipped by its column — each
// carry the sentence that explains them.
{
  const named = async (what, selector) => {
    const found = page.locator(selector).first()
    if (!(await found.count())) return console.log(`  – ${what}: not on screen, skipped`)
    const said = await found.getAttribute('title')
    console.log(`  ${said ? '·' : '✗'} ${what}: ${said ?? 'NO TITLE  FAIL'}`)
  }
  console.log('tooltips')
  await named('divider', '.gutter:not(.inert)')
  await named('filter box', '.session-find')
  await named('group toggle', '.session-group')
  await named('session title', '.session-title')
  await named('header path', '.transcript-head .where')
  await named('context section', '.panel-head')
  await named('a file read', '.files code')
  await named('a run of steps', '.tool-run-head')
  await named('a link the model wrote', '.bubble a[href]')
  await named('a confined origin', '.quarantine-head .origin')
}

await app.close()
console.log(`screenshots in ${shots}`)
