// That the file tree in the context column lists the session's folder, expands, and refuses
// to leave it.
//
// The assertions worth making here are the ones no screenshot makes. Two are about the
// boundary rather than the drawing: that a listing needs a session the main process has a
// root for, and that a symlink pointing out of the project is refused rather than followed —
// which is the half of the promise `isSubpath` cannot keep, since `link` is a perfectly good
// relative path. The symlink is made inside the real project the first session runs in and
// removed again in a `finally`, because a driver that litters somebody's checkout is worse
// than one that skips a check.
//
// What this does not do is double-click a file. That would launch whatever app the machine
// running it assigns the type, which is not something a test may do to somebody's desktop;
// that step is in the manual pass.
import { _electron as electron } from 'playwright-core'
import { mkdirSync, symlinkSync, rmSync } from 'node:fs'
import { join } from 'node:path'

mkdirSync('/tmp/bravebot-ui', { recursive: true })

const problems = []
const check = (ok, what) => {
  console.log(`${ok ? '  ok  ' : ' FAIL '} ${what}`)
  if (!ok) problems.push(what)
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

// The layout is remembered between launches and shared with every other driver, so the run
// starts by putting the columns back rather than assuming it inherited them open. A folded
// right column would make every locator below invisible.
for (const side of ['left', 'right']) {
  const toggle = page.locator(`.fold-toggle.${side}`)
  if ((await toggle.getAttribute('aria-expanded')) === 'false') {
    await toggle.click()
    await page.waitForTimeout(300)
  }
}

if ((await page.locator('.session').count()) === 0) {
  console.log('RESULT: skipped — no sessions to open')
  await app.close()
  process.exit(0)
}
await page.locator('.session').first().click()
await page.waitForTimeout(1600)

// The file tree can be turned off from the bar at the top of the column, and that choice is
// remembered between launches — so a run puts it back rather than assuming it inherited a column
// with a tree in it. The same courtesy the columns get above. After the session is opened, because
// the bar belongs to a column with something in it and there is no bar before then.
const filesPick = page.locator('.panel-pick').last()
if ((await filesPick.getAttribute('aria-pressed')) === 'false') {
  await filesPick.click()
  await page.waitForTimeout(400)
}

const rows = page.locator('.tree-list[role="tree"] > li > .tree-row')
// The names alone, not the rows: a row also holds a chevron and the two-letter type badge, and
// reading those as part of the name is how an earlier version of this file talked itself into
// believing a project had no dotfiles in it.
const names = () =>
  page.locator('.tree-list[role="tree"] > li > .tree-row .tree-name').allInnerTexts()

// --- the folder is on screen -----------------------------------------------------------
const root = await page.locator('.tree-root').getAttribute('title')
check(typeof root === 'string' && root.startsWith('/'), `the panel names the folder (${root})`)
await page.waitForTimeout(600)
const first = await rows.count()
check(first > 0, `the root of the folder is listed (${first} rows)`)

// --- dotfiles are behind the toggle ----------------------------------------------------
const dotty = (list) => list.filter((name) => name.trim().startsWith('.')).length
check(dotty(await names()) === 0, 'no dot-prefixed entry is listed until asked for')
await page.locator('.tree-tool').first().click()
await page.waitForTimeout(500)
const shown = await names()
check(shown.length >= first, `showing hidden entries never lists fewer (${first} → ${shown.length})`)
if (dotty(shown) > 0) {
  check(shown.length > first, `the toggle brought dot-prefixed entries in (${dotty(shown)})`)
} else {
  console.log('  --   this project has no dotfiles in its root; nothing for the toggle to add')
}
await page.locator('.tree-tool').first().click()
await page.waitForTimeout(400)

// --- a directory expands ---------------------------------------------------------------
// A file carries no `aria-expanded` at all, so this names the folders and nothing else.
const folders = page.locator('.tree-list[role="tree"] > li[aria-expanded]')
const shut = await folders.count()
check(
  shut === 0 || (await folders.first().getAttribute('aria-expanded')) === 'false',
  'a folder starts shut',
)

// An empty folder is a perfectly good folder and says so when opened, which is not what the
// indent below is measured on. So the folders are tried in order until one has something in
// it, and each one that does not is shut again — the same hunt `drive-panels.mjs` makes for a
// session with a run of tool calls in it.
let item = null
let inner = null
for (let index = 0; index < Math.min(shut, 8); index++) {
  const candidate = folders.nth(index)
  await candidate.locator('.tree-row').first().click()
  await page.waitForTimeout(800)
  const within = candidate.locator('[role="group"] > li > .tree-row')
  if ((await within.count()) > 0) {
    item = candidate
    inner = within
    break
  }
  await candidate.locator('.tree-row').first().click()
  await page.waitForTimeout(300)
}

if (item === null) {
  console.log('  --   nothing in the root of this project has children; nothing to expand')
} else {
  const folder = item.locator('.tree-row').first()
  check((await item.getAttribute('aria-expanded')) === 'true', 'clicking a folder opens it')
  const deep = await inner.count()
  check(deep > 0, `the folder's own entries appear under it (${deep})`)
  if (deep > 0) {
    const outer = await folder.boundingBox()
    const child = await inner.first().boundingBox()
    // The indent is the whole reason a tree reads as a tree. It is a padding rather than a
    // nested box, so this measures where the text starts, not where the row does.
    const outerText = await folder.locator('.tree-name').boundingBox()
    const childText = await inner.first().locator('.tree-name').boundingBox()
    check(
      childText.x > outerText.x && child.width === outer.width,
      `a child is indented (${Math.round(outerText.x)} → ${Math.round(childText.x)}) without narrowing the row`,
    )
  }
  await page.screenshot({ path: '/tmp/bravebot-ui/11-tree.png' })
  await folder.click()
  await page.waitForTimeout(500)
  check((await item.getAttribute('aria-expanded')) === 'false', 'clicking it again shuts it')
}

// --- the filter ------------------------------------------------------------------------
// What is asserted here is that the filter narrows and that the path to a match survives it: a
// query that deleted the folders its results are in would be a list of names with nowhere to
// click. The term is taken from a row that is actually on screen, so this works in whatever
// project the first session happens to be in.
const before = await rows.count()
// The longest name on screen, and a slice out of the middle of it rather than its start: a term
// that is also a prefix would pass even if the filter only ever matched from the front.
const sample = (await names())
  .map((name) => name.trim())
  .sort((left, right) => right.length - left.length)[0] ?? ''
if (sample.length < 5) {
  console.log('  --   nothing in this root has a name long enough to filter on')
} else {
  const term = sample.slice(2, 5)
  await page.locator('.tree-find').fill(term)
  await page.waitForTimeout(500)
  const after = await rows.count()
  check(after > 0 && after <= before, `filtering on "${term}" narrows the list (${before} → ${after})`)
  check(
    (await names()).every((name) => name.toLowerCase().includes(term.toLowerCase())) ||
      (await page.locator('.tree-list[role="tree"] > li[aria-expanded="true"]').count()) > 0,
    'a row that does not match itself is only there to hold a match underneath it',
  )
  check(
    await page.locator('.tree-note').isVisible(),
    'and the panel says it has only read the folders that were opened',
  )
  await page.locator('.tree-find').press('Escape')
  await page.waitForTimeout(400)
  check((await rows.count()) === before, 'Escape clears the filter')
}

// --- the boundary ----------------------------------------------------------------------
// A handle the main process has no root for gets nothing, even for the one path that is
// always legal. This is the check that the tree cannot ask about a folder no session of this
// window is running in.
const strangerListed = await page.evaluate(() =>
  window.bravebot.listFiles('not-a-live-session', ''),
)
check(strangerListed === null, 'a session the main process never opened lists nothing')

// Lexically impossible paths, refused before they become a syscall.
for (const path of ['..', '../..', 'src/../..', '/etc', 'src/../../etc']) {
  const answer = await page.evaluate(
    (p) => window.bravebot.listFiles('not-a-live-session', p),
    path,
  )
  check(answer === null, `\`${path}\` is not a path this app will resolve`)
}
const openedStranger = await page.evaluate(() => window.bravebot.openFile('not-a-live-session', '/etc/hosts'))
check(openedStranger.status === 'failed', 'an absolute path is not a file this app will open')

// A symlink out of the project, through the real session's real handle — the one refusal a
// lexical check cannot make.
const link = root ? join(root, 'bravebot-tree-probe-link') : null
try {
  if (link) {
    symlinkSync('/etc', link)
    await page.locator('.tree-tool').nth(1).click()
    await page.waitForTimeout(900)
    const probe = page
      .locator('.tree-list[role="tree"] > li')
      .filter({ hasText: 'bravebot-tree-probe-link' })
      .first()
    if ((await probe.count()) === 0) {
      check(false, 'the refreshed listing picked up the probe link')
    } else {
      await probe.locator('.tree-row').first().click()
      await page.waitForTimeout(800)
      const said = await probe.innerText()
      check(
        /cannot be read/.test(said),
        'a symlink pointing out of the project is refused, not followed',
      )
    }
  }
} finally {
  if (link) rmSync(link, { force: true })
}

await app.close()

if (problems.length > 0) {
  console.log(`\nRESULT: ${problems.length} failed`)
  process.exit(1)
}
console.log('\nRESULT: ok — shot in /tmp/bravebot-ui/11-tree.png')
