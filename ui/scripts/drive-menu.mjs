// That the app has its own menu, that it kept what the default was already doing, and that
// nothing in it decides anything.
//
// The three assertions worth the trouble are the ones nobody would notice going wrong.
// Replacing the application menu silently takes copy and paste away from the composer if
// the Edit role is dropped — nothing errors, the keystroke simply stops working — so the
// standard roles are checked as a regression guard rather than as a feature. A release
// build must not offer Developer Tools, because a console in this renderer is a way to
// approve a write without reading it. And no menu item may answer one of the agent's five
// questions, which is asserted directly rather than left to the reader of the command list.
//
// Costs nothing: it never sends a prompt. It does drive the fold commands, which share the
// persisted layout with every other driver, so it puts the columns back before it leaves.
import { _electron as electron } from 'playwright-core'
import { mkdirSync, readFileSync, writeFileSync, existsSync, rmSync } from 'node:fs'
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
await page.waitForTimeout(2000)

/** The whole menu, flattened to something assertable. */
const readMenu = () =>
  app.evaluate(({ Menu }) => {
    const menu = Menu.getApplicationMenu()
    if (!menu) return null
    // Flattened all the way down rather than one level, because File > Export puts real
    // commands inside a submenu — and an item this function could not see would be one the
    // assertions below silently stopped covering. A menu item that decides something must
    // fail this run no matter how deeply somebody nested it.
    const descend = (item) => [
      {
        id: item.id,
        label: item.label,
        role: item.role,
        type: item.type,
        accelerator: item.accelerator,
        enabled: item.enabled,
      },
      ...(item.submenu?.items ?? []).flatMap(descend),
    ]
    return menu.items.map((top) => ({
      label: top.label,
      role: top.role,
      items: (top.submenu?.items ?? []).flatMap(descend),
    }))
  })

const menu = await readMenu()
check(menu !== null, 'there is an application menu at all')
const titles = menu.map((m) => m.label)
const every = menu.flatMap((m) => m.items)
const roles = (title) =>
  (menu.find((m) => m.label === title)?.items ?? []).map((i) => (i.role ?? '').toLowerCase())

// --- it is ours ---------------------------------------------------------------------
check(titles.includes('File'), `there is a File menu (${titles.join(', ')})`)
check(titles.includes('View'), 'there is a View menu')
check(titles.includes('Session'), 'there is a Session menu — the app has its own verbs')
check(titles.includes('Help'), 'there is a Help menu')
check(
  !every.some((i) => /learn more/i.test(i.label ?? '')),
  'Help no longer points at electronjs.org',
)
check(
  every.some((i) => i.id === 'app.about' && /Brave Bot/.test(i.label ?? '')),
  'the About item names this app, not the package',
)
check(
  every.some((i) => i.role === 'quit' && i.label === 'Quit Brave Bot'),
  'Quit names this app — without app.setName, which would move userData',
)
// The bold word beside the Apple menu comes from the bundle's CFBundleName, which no
// template can reach — so it is asserted at its actual source rather than through the menu.
const bundleName = await app.evaluate(({ app }) => app.getPath('exe'))
const plist = bundleName.replace(/\/Contents\/MacOS\/.*$/, '/Contents/Info.plist')
check(
  existsSync(plist) && /Brave Bot/.test(readFileSync(plist, 'utf8')),
  'the running bundle is named "Brave Bot", so the menu bar title is not "Electron"',
)

// --- what the default was already doing, and must keep doing --------------------------
const edit = roles('Edit')
check(
  ['undo', 'redo', 'cut', 'copy', 'paste', 'selectall'].every((r) => edit.includes(r)),
  `Edit still has the clipboard roles (${edit.join(', ')})`,
)
const windowRoles = roles('Window')
check(
  ['minimize', 'zoom'].every((r) => windowRoles.includes(r)),
  'Window still has minimize and zoom',
)
// Electron reports a role's built-in accelerator in its long spelling, so both forms count:
// they are the same key, and asserting only one would fail on a rename that changed nothing.
check(
  every.some((i) => i.role === 'close' && /^(CmdOrCtrl|CommandOrControl)\+W$/.test(i.accelerator ?? '')),
  'Cmd+W still closes the window rather than the session',
)

// --- nothing here decides anything ----------------------------------------------------
const DECIDING = /approve|reject|vouch|run once|apply this change|don't run|let the planner/i
check(
  !every.some((i) => DECIDING.test(i.label ?? '')),
  'no menu item answers one of the agent questions',
)
// The command ids the menu is allowed to carry, spelled a second time on purpose: it is
// what turns "somebody added a menu item" into a check a human has to consciously update.
const ALLOWED_IDS = [
  'session.new', 'session.close', 'turn.send', 'turn.cancel',
  'view.fold-left', 'view.fold-right', 'view.reset-columns', 'app.about', 'help.doctor',
  'session.export-text', 'session.export-markdown', 'session.export-pdf',
  // A setting, not an action: it decides what goes in an exported file, and like everything
  // else in this list it reaches no agent method.
  'session.export-tools',
  // Opens the theme picker. Nothing labelled is involved: the names are read off disk and drawn
  // for a person, they never reach a model, and choosing one is not an answer to anything the
  // agent asked. It reaches no agent method either — what crosses is a name, checked against the
  // list the main process built.
  'view.theme',
]
const ours = every.filter((i) => i.id && !i.role).map((i) => i.id)
check(
  ours.every((id) => ALLOWED_IDS.includes(id)),
  `every non-role item is a declared command (${ours.join(', ')})`,
)
// What the window may ask the agent for, counted. The number is checked rather than described
// because it is the one place a feature can widen this app's reach without anybody noticing,
// and a count that has to be edited is a change somebody has to look at.
//
// Export was the case for keeping it: writing a file to disk is the largest capability this app
// has taken on and it reaches no agent method at all. Forking is the opposite case and the
// reason the number became 15. Model discovery adds one read-only method, `models.list`.
const allowed = (readFileSync('src/main/index.ts', 'utf8').match(/const ALLOWED = new Set\(\[([^\]]*)\]/) ?? [])[1]
check(
  allowed !== undefined && !/approve|decide/.test(allowed) && allowed.includes("'models.list'") && allowed.split(',').filter((s) => s.trim()).length === 16,
  'the main-process allow-list is 16 methods including model discovery, none of which decides anything',
)

// --- accelerators are declared (they cannot be *dispatched* from here) -----------------
// Playwright's keyboard goes into the web contents over CDP; AppKit key equivalents never
// see it. So the accelerator string is asserted as a contract and the effect is driven by
// clicking the item.
const accel = (id) => every.find((i) => i.id === id)?.accelerator
check(accel('session.new') === 'CmdOrCtrl+N', `New Session is Cmd+N (${accel('session.new')})`)
check(accel('turn.send') === 'CmdOrCtrl+Enter', 'Send is Cmd+Enter')
check(accel('turn.cancel') === 'CmdOrCtrl+.', 'Cancel Turn is Cmd+.')
check(
  accel('session.close') === 'CmdOrCtrl+Shift+W',
  'Close Session is Cmd+Shift+W, leaving Cmd+W to the window',
)

// --- developer items are gated --------------------------------------------------------
const packaged = await app.evaluate(({ app }) => app.isPackaged)
check(packaged === false, 'this run is unpackaged, so the dev branch is the one under test')
const view = roles('View')
check(view.includes('toggledevtools'), 'unpackaged, View offers Developer Tools')
// The packaged case cannot be driven from here; it is a manual check in the README.

// --- enablement tracks what the window can do -----------------------------------------
const fresh = await readMenu()
const freshItem = (id) => fresh.flatMap((m) => m.items).find((i) => i.id === id)
check(freshItem('session.close').enabled === false, 'with no session open, Close Session is grey')
check(freshItem('turn.send').enabled === false, 'with no session open, Send is grey')
check(freshItem('turn.cancel').enabled === false, 'with nothing running, Cancel Turn is grey')

const click = (id) =>
  app.evaluate(({ Menu }, id) => Menu.getApplicationMenu().getMenuItemById(id).click(), id)

const hasSessions = (await page.locator('.session').count()) > 0
if (hasSessions) {
  await page.locator('.session').first().click()
  await page.waitForTimeout(1200)
  if (await page.locator('.trust').isVisible().catch(() => false)) {
    await page.locator('.trust-actions .approve').click()
    await page.waitForTimeout(500)
  }
  let open = await readMenu()
  const openItem = (id) => open.flatMap((m) => m.items).find((i) => i.id === id)
  check(openItem('session.close').enabled === true, 'with a session open, Close Session lights up')
  check(openItem('turn.send').enabled === false, 'an empty composer still leaves Send grey')

  // The assertion that proves the state channel is live in both directions.
  await page.locator('.composer textarea').fill('hello')
  await page.waitForTimeout(400)
  open = await readMenu()
  check(
    open.flatMap((m) => m.items).find((i) => i.id === 'turn.send').enabled === true,
    'typing into the composer enables Send',
  )
  await page.locator('.composer textarea').fill('')
  await page.waitForTimeout(400)
  open = await readMenu()
  check(
    open.flatMap((m) => m.items).find((i) => i.id === 'turn.send').enabled === false,
    'clearing it greys Send again',
  )
} else {
  console.log('  --   no sessions on this machine; skipped the session-dependent checks')
}

// --- a menu item actually does something ----------------------------------------------
const widthOf = (selector) =>
  page.locator(selector).evaluate((el) => el.getBoundingClientRect().width)

const before = await widthOf('.sessions')
await click('view.fold-left')
await page.waitForTimeout(400)
check((await widthOf('.sessions')) < 1, `View → Hide Session List folds it (was ${Math.round(before)})`)
check(
  (await page.locator('.fold-toggle.left').getAttribute('aria-expanded')) === 'false',
  'and the transcript header agrees the column is folded',
)
let folded = await readMenu()
check(
  folded.flatMap((m) => m.items).find((i) => i.id === 'view.fold-left').label ===
    'Show Session List',
  'the item renames itself to the thing it will now do',
)
await click('view.fold-left')
await page.waitForTimeout(400)
check((await widthOf('.sessions')) > 1, 'and it comes back')

// --- the panels a menu item opens ------------------------------------------------------
await click('app.about')
await page.waitForTimeout(600)
const about = page.getByRole('dialog', { name: 'About Brave Bot' })
check(await about.isVisible(), 'About opens a panel')
await about.locator('summary').click()
check(
  /Agent/.test(await about.locator('dl').textContent()),
  'and it carries the agent build, which is the first thing worth knowing',
)
await page.screenshot({ path: '/tmp/bravebot-ui/11-menu-about.png' })
await page.keyboard.press('Escape')
await page.waitForTimeout(400)
check(!(await about.isVisible()), 'Escape closes it — nothing here traps anybody')

// The recents list is this machine's, and a fresh checkout has none — which would leave the
// keyboard walk below with a single disabled row and nothing to walk. So a known list is put
// in place for the duration and the original is put back at the end, the way
// `drive-columns.mjs` treats the layout it shares. One key of the file, not the file: the
// other four are somebody's arrangement of this window.
const userData = await app.evaluate(({ app }) => app.getPath('userData'))
const stateFile = join(userData, 'bravebot-ui.json')
const hadState = existsSync(stateFile) ? readFileSync(stateFile, 'utf8') : null
writeFileSync(
  stateFile,
  JSON.stringify({
    ...(hadState === null ? {} : JSON.parse(hadState)),
    // The third is deliberately not a path, to prove the validator drops it rather than
    // refusing the two good ones alongside it.
    recents: ['/tmp/alpha-project', '/tmp/beta-project', 'relative/nope'],
  }),
  'utf8',
)

// --- the in-window picker -----------------------------------------------------------------
// Native menus cover right-clicks; this one is in the window because it has to hand focus
// back to the button that opened it, which `Menu.popup` gives no way to do.
const chevron = page.locator('.new-recent')
check(await chevron.isVisible(), 'the New session button has a recents chevron')
check(
  (await chevron.getAttribute('aria-expanded')) === 'false',
  'and it says it is closed before it is opened',
)
await chevron.click()
await page.waitForTimeout(350)
check(await page.locator('[role="menu"]').isVisible(), 'clicking it opens a menu')
check(
  (await chevron.getAttribute('aria-expanded')) === 'true',
  'and the trigger now says it is open',
)
const rows = await page.locator('[role="menuitem"]').count()
check(rows === 2, `the two valid recents are listed and the bad one was dropped (${rows})`)
check(
  (await page.locator('[role="menuitem"]').first().textContent()).includes('alpha-project'),
  'newest first, by folder name',
)
check(
  (await page.locator('.popitem-detail').first().textContent()) === '/tmp/alpha-project',
  'with the full path under it, because two checkouts share a basename',
)
check(
  await page.evaluate(() => document.activeElement?.getAttribute('role') === 'menuitem'),
  'focus moved into the menu',
)
if (rows > 1) {
  const before = await page.evaluate(() => document.activeElement?.textContent)
  await page.keyboard.press('ArrowDown')
  await page.waitForTimeout(150)
  check(
    (await page.evaluate(() => document.activeElement?.textContent)) !== before,
    'ArrowDown moves to another row',
  )
  await page.keyboard.press('Home')
  await page.waitForTimeout(150)
  check(
    (await page.evaluate(() => document.activeElement?.textContent)) === before,
    'and Home comes back to the first',
  )
}
await page.screenshot({ path: '/tmp/bravebot-ui/12-popmenu.png' })
await page.keyboard.press('Escape')
await page.waitForTimeout(300)
check(!(await page.locator('[role="menu"]').isVisible()), 'Escape closes it')
check(
  await page.evaluate(() => document.activeElement?.classList.contains('new-recent')),
  'and focus went back to the button that opened it',
)

// --- recents are the main process's own record ---------------------------------------------
// Typeahead, which is the part of a menu people only miss when it is absent.
await chevron.click()
await page.waitForTimeout(300)
await page.keyboard.press('b')
await page.waitForTimeout(200)
check(
  (await page.evaluate(() => document.activeElement?.textContent))?.includes('beta'),
  'typing a letter jumps to the row that starts with it',
)
await page.keyboard.press('Escape')
await page.waitForTimeout(250)

// --- context menus ----------------------------------------------------------------------
// A real popup is modal and would block the run, so `popup` is replaced with something that
// records what it was about to show. That is also the only way to see the item set at all.
await app.evaluate(({ Menu }) => {
  globalThis.__pops = []
  Menu.prototype.popup = function popup() {
    globalThis.__pops.push(this.items.map((i) => ({ id: i.id, label: i.label })))
  }
})

if (hasSessions) {
  await page.locator('.session').first().click({ button: 'right' })
  await page.waitForTimeout(400)
  const pops = await app.evaluate(() => globalThis.__pops)
  check(pops.length === 1, `right-clicking a session opens one menu (${pops.length})`)
  const ids = (pops[0] ?? []).map((i) => i.id)
  check(
    ids.join() === 'context.session.open,context.session.close,context.session.copy-path',
    `and it offers open, close and copy path (${ids.join(', ')})`,
  )
}

await app.evaluate(() => { globalThis.__pops = [] })
// A reply rather than any entry: tool lines inside a folded run carry no right-click at all,
// which is its own small fact and not the one being checked here.
const bubble = page.locator('.bubble.assistant').first()
if (await bubble.isVisible().catch(() => false)) {
  await bubble.click({ button: 'right' })
  await page.waitForTimeout(400)
  const pops = await app.evaluate(() => globalThis.__pops)
  check(pops.length === 1, 'right-clicking a transcript entry opens one menu')
  const ids = (pops[0] ?? []).map((i) => i.id)
  // The assertion the whole design is for: a transcript entry offers copying and nothing
  // else. A confirm card gets the same one item as a plain reply.
  check(
    ids.join() === 'context.entry.copy',
    `and it offers copying and nothing that decides (${ids.join(', ')})`,
  )
}

// A prompt is the one exception, and it is not an exception to the rule above: forking decides
// nothing. It opens a session holding what was said before that point, and every question the
// new session raises is asked in its transcript. `drive-fork.mjs` walks what it does.
await app.evaluate(() => { globalThis.__pops = [] })
const prompt = page.locator('.bubble.user').first()
if (await prompt.isVisible().catch(() => false)) {
  await prompt.click({ button: 'right' })
  await page.waitForTimeout(400)
  const ids = ((await app.evaluate(() => globalThis.__pops))[0] ?? []).map((i) => i.id)
  check(
    ids.join() === 'context.entry.copy,context.entry.fork',
    `and a prompt offers copying and forking, and still nothing that decides (${ids.join(', ')})`,
  )
}

// Put the preferences file back the way it was found.
if (hadState === null) rmSync(stateFile, { force: true })
else writeFileSync(stateFile, hadState, 'utf8')

// The columns are shared with the other drivers, so this one puts them back.
await page.locator('.gutter').first().dblclick()
await page.locator('.gutter').nth(1).dblclick()
await page.waitForTimeout(300)
await app.close()

if (problems.length > 0) {
  console.log(`\nRESULT: ${problems.length} failed`)
  process.exit(1)
}
console.log('\nRESULT: ok — shot in /tmp/bravebot-ui/11-menu-about.png')
