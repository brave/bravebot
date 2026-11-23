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
// Costs nothing. It runs in a temporary world from `drive-world.mjs`: a `HOME`, a profile and a
// project of its own, and a model service this script serves on 127.0.0.1, which the scratch
// `settings.json` names as the provider. The one prompt it sends goes to that service, so the
// transcript has a prompt and a reply to right-click, and the run needs no credentials from the
// environment or from `~/.bravebot`. The fold commands it drives change a layout of its own, and
// nothing of a person's `bravebot-ui.json` or `~/.bravebot` is read or written.
import { mkdirSync, readFileSync, writeFileSync, existsSync, realpathSync, rmSync } from 'node:fs'
import { constants } from 'node:os'
import { checker, launchLive, COMPOSER_IDLE, COMPOSER_INPUT, COMPOSER_SEND, ASSISTANT_BUBBLE, approveTrustIfAsked, SHOTS_DIR } from './drive-support.mjs'
import { launchWorld, makeWorld, openProject, startModelService } from './drive-world.mjs'
import { allowedMethods } from './allow-list.mjs'

mkdirSync(SHOTS_DIR, { recursive: true })

const { check, problems } = checker()

// The model service answers every request with this sentence. A chat opened in an empty project has
// no entries, and a transcript entry has no menu to right-click until it has some.
const REPLY = 'The stand-in model service answers every request with this sentence.'
const { service } = await startModelService(() => ({ content: REPLY }))
// A `HOME`, a profile and a project of their own, with the service above as the provider in the
// scratch `settings.json`. Whether the window can send then does not depend on the credentials a
// bridge was built with or on the person's environment: the app is launched with only the variables
// `makeWorld` names. The directory is removed when the program ends, by an exit or by `process.exit`.
const world = makeWorld('bravebot-drive-menu-', { service })
process.on('exit', () => rmSync(world.root, { recursive: true, force: true }))
for (const signal of ['SIGINT', 'SIGTERM', 'SIGHUP']) {
  process.on(signal, () => process.exit(128 + constants.signals[signal]))
}

const { app, page } = await launchLive(async () => (await launchWorld({ profile: world.profile, env: world.env })).app)
await page.waitForTimeout(2000)
// Asked of the app, since a launch that lost its `--user-data-dir` would run on the real profile and
// look the same from here. A run on any other profile stops here, before it clicks or types anything.
const userData = await app.evaluate(({ app }) => app.getPath('userData'))
const onScratchProfile = realpathSync(userData) === realpathSync(world.profile)
check(onScratchProfile, `the run is on its scratch profile, not the person's (${userData})`)
if (!onScratchProfile) {
  await app.close()
  console.log(`\nRESULT: ${problems.length} failed`)
  process.exit(1)
}

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
check(titles.includes('Chat'), 'there is a Chat menu, the app has its own verbs')
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
  // Shows the release notes the agent carries, as plain text. It reaches one read-only method.
  'help.changelog',
  'session.export-text', 'session.export-markdown', 'session.export-pdf',
  // A setting, not an action: it decides what goes in an exported file, and like everything
  // else in this list it reaches no agent method.
  'session.export-tools',
  // Opens the theme picker. Nothing labelled is involved: the names are read off disk and drawn
  // for a person, they never reach a model, and choosing one is not an answer to anything the
  // agent asked. It reaches no agent method either — what crosses is a name, checked against the
  // list the main process built.
  'view.theme',
  // Open the find bar and put the caret in the composer. Both move focus inside the window.
  'view.find', 'view.focus-composer',
  // Chooses what the session asks. It answers no question already put: the running turn follows
  // it from its next question (MODE-8).
  'mode.cycle',
  // Opens a dialog that names every file it would put back. Only that dialog's button rewinds.
  'turn.rewind',
  // Open About and the bug report. Neither decides anything, and the report is written only where
  // its Save button and a native sheet put it.
  'help.update', 'help.bug-report',
  // Opens a dialog for a note kept on the record. It reaches `session.note`, which sends the
  // model nothing.
  'session.note',
  // Session Status, Token Spend, Context Breakdown and Last Request: each draws a report from a
  // store the turn writes into (CMD-8). None of them answers a question the agent asked.
  'session.status', 'session.cost', 'session.context', 'session.request',
  // Compact Conversation, Hand Off to New Chat and Continue in Background. Each opens a dialog first
  // (a focus, the next goal, a confirmation) before it reaches its method.
  'session.compact', 'session.handoff', 'session.background',
  // A setting, not an action: it keeps the computer awake while a turn runs. It reaches no agent
  // method, and the first time it is turned on a dialog says what it does.
  'app.caffeinate',
  // Unfolds the context column and opens its section checklist. It only changes what the column
  // draws, and reaches no agent method.
  'app.sections',
]
const ours = every.filter((i) => i.id && !i.role).map((i) => i.id)
check(
  ours.every((id) => ALLOWED_IDS.includes(id)),
  `every non-role item is a declared command (${ours.join(', ')})`,
)
// What the window may ask the agent for: the method names in the main process's allow-list, read
// by `scripts/allow-list.mjs`, which takes the `//` comments off the literal first, since a comment
// can hold commas and quotes of its own.
//
// The first check is a naming check. It stops a method whose name says it approves or decides from
// being added to the list without a person noticing. It does not decide what the window may do: the
// `*.reply` methods (`confirm.reply`, `vouch.reply`, `trust.reply` and the rest) are on the list
// because they carry the person's answers to the agent's questions through the window, and
// `scripts/session-directory.test.mjs` classifies each of them. `models.list` is the read-only model
// discovery the picker needs, so a list without it was read wrong, and a literal that could not be
// read has no names and fails the second check. `scripts/allow-list.test.mjs` holds the list to the
// same rules in `make check-ui-build`. How many names there are is not asserted: a number written
// here has to be edited by hand with every method, and `scripts/session-directory.test.mjs` already
// fails for a name added to the list and not placed there.
const allowed = allowedMethods(readFileSync('src/main/index.ts', 'utf8')) ?? []
check(
  !allowed.some((name) => /approve|decide/i.test(name)),
  `none of the ${allowed.length} method names the window may send approves or decides`,
)
check(allowed.includes('models.list'), 'models.list, the read-only model discovery, is on the allow-list')

// --- accelerators are declared (they cannot be *dispatched* from here) -----------------
// Playwright's keyboard goes into the web contents over CDP; AppKit key equivalents never
// see it. So the accelerator string is asserted as a contract and the effect is driven by
// clicking the item.
const accel = (id) => every.find((i) => i.id === id)?.accelerator
check(accel('session.new') === 'CmdOrCtrl+N', `New Chat is Cmd+N (${accel('session.new')})`)
check(accel('turn.send') === 'CmdOrCtrl+Enter', 'Send is Cmd+Enter')
check(accel('turn.cancel') === 'CmdOrCtrl+.', 'Cancel Turn is Cmd+.')
check(accel('mode.cycle') === 'CmdOrCtrl+Shift+M', 'Cycle Permission Mode is Cmd+Shift+M, leaving Shift+Tab to focus')
check(
  accel('session.close') === 'CmdOrCtrl+Shift+W',
  'Close Chat is Cmd+Shift+W, leaving Cmd+W to the window',
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
check(freshItem('session.close').enabled === false, 'with no chat open, Close Chat is grey')
check(freshItem('turn.send').enabled === false, 'with no session open, Send is grey')
check(freshItem('turn.cancel').enabled === false, 'with nothing running, Cancel Turn is grey')

const click = (id) =>
  app.evaluate(({ Menu }, id) => Menu.getApplicationMenu().getMenuItemById(id).click(), id)

// A scratch profile has no sessions, so the checks that need a chat open are given one: opening a
// project starts a chat in it. The folder is inside the temporary directory and is removed with it.
await openProject(page, app, world.project)
await page.waitForTimeout(1200)

const hasSessions = (await page.locator('.session').count()) > 0
check(hasSessions, 'opening a project starts a chat, which the checks below need')
if (hasSessions) {
  await page.locator('.session').first().click()
  await page.waitForTimeout(1200)
  await approveTrustIfAsked(page, 500)
  let open = await readMenu()
  const openItem = (id) => open.flatMap((m) => m.items).find((i) => i.id === id)
  check(openItem('session.close').enabled === true, 'with a chat open, Close Chat lights up')
  check(openItem('turn.send').enabled === false, 'an empty composer still leaves Send grey')

  // The assertion that proves the state channel is live in both directions.
  await page.locator(COMPOSER_INPUT).fill('hello')
  await page.waitForTimeout(400)
  open = await readMenu()
  check(
    open.flatMap((m) => m.items).find((i) => i.id === 'turn.send').enabled === true,
    'typing into the composer enables Send',
  )
  await page.locator(COMPOSER_INPUT).fill('')
  await page.waitForTimeout(400)
  open = await readMenu()
  check(
    open.flatMap((m) => m.items).find((i) => i.id === 'turn.send').enabled === false,
    'clearing it greys Send again',
  )

  // One prompt to the stand-in model service gives the transcript a prompt and a reply, which the
  // context-menu checks below right-click. The wait is on the reply and then on the composer being
  // idle again, since a prompt cannot be forked from while a turn runs.
  await page.locator(COMPOSER_INPUT).fill('hello')
  await page.locator(COMPOSER_SEND).click()
  await page.locator(ASSISTANT_BUBBLE).first().waitFor()
  await page.locator(COMPOSER_IDLE).waitFor()
  check(
    ((await page.locator(ASSISTANT_BUBBLE).first().textContent()) ?? '').includes(REPLY),
    'a prompt sent to the stand-in model service is answered in the transcript',
  )
} else {
  console.log('  --   no chat is open; skipped the session-dependent checks')
}

// --- a menu item actually does something ----------------------------------------------
const widthOf = (selector) =>
  page.locator(selector).evaluate((el) => el.getBoundingClientRect().width)

const before = await widthOf('.sessions')
await click('view.fold-left')
await page.waitForTimeout(400)
const afterFold = await widthOf('.sessions')
check(afterFold < 1, `View → Hide Chat List folds it (was ${Math.round(before)}, now ${Math.round(afterFold)})`)
check(
  (await page.locator('.fold-toggle.left').getAttribute('aria-expanded')) === 'false',
  'and the transcript header agrees the column is folded',
)
let folded = await readMenu()
check(
  folded.flatMap((m) => m.items).find((i) => i.id === 'view.fold-left').label ===
    'Show Chat List',
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
await page.screenshot({ path: `${SHOTS_DIR}/11-menu-about.png` })
await page.keyboard.press('Escape')
await page.waitForTimeout(400)
check(!(await about.isVisible()), 'Escape closes it — nothing here traps anybody')

// The recents chevron beside New session is gone: a new chat starts in the project used last, and
// the projects opened before are offered by the project menu in a fresh chat's composer and by
// File ▸ Open Recent. Neither is driven here.

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

  await app.evaluate(() => { globalThis.__pops = [] })
  // A reply rather than any entry: tool lines inside a folded run carry no right-click at all,
  // which is its own small fact and not the one being checked here.
  await page.locator(ASSISTANT_BUBBLE).first().click({ button: 'right' })
  await page.waitForTimeout(400)
  const replyPops = await app.evaluate(() => globalThis.__pops)
  check(replyPops.length === 1, 'right-clicking a transcript entry opens one menu')
  const replyIds = (replyPops[0] ?? []).map((i) => i.id)
  // The assertion the whole design is for: a transcript entry offers copying and nothing
  // else. A confirm card gets the same one item as a plain reply.
  check(
    replyIds.join() === 'context.entry.copy',
    `and it offers copying and nothing that decides (${replyIds.join(', ')})`,
  )

  // A prompt is the one exception, and it is not an exception to the rule above: forking decides
  // nothing. It opens a session holding what was said before that point, and every question the
  // new session raises is asked in its transcript. `drive-fork.mjs` walks what it does. A prompt
  // whose turn has ended also offers Rewind to Before This, which asks first in a dialog naming every
  // file it puts back and decides nothing either. `drive-rewind.mjs` walks that one.
  await app.evaluate(() => { globalThis.__pops = [] })
  await page.locator('.bubble.user').first().click({ button: 'right' })
  await page.waitForTimeout(400)
  const promptIds = ((await app.evaluate(() => globalThis.__pops))[0] ?? []).map((i) => i.id)
  check(
    promptIds.join() === 'context.entry.copy,context.entry.fork,context.entry.rewind',
    `and a prompt offers copying, forking and rewinding, and still nothing that decides (${promptIds.join(', ')})`,
  )
}

// Double-clicking a divider puts its column back at the default width. The right divider is hidden
// while no chat is open (`.app.no-session .gutter.right` in shell.css), so this needs the chat the
// run opened above: with none open, the second double-click waits for the divider to show and times out.
await page.locator('.gutter').first().dblclick()
await page.locator('.gutter').nth(1).dblclick()
await page.waitForTimeout(300)
await app.close()
await new Promise((resolve) => service.close(resolve))

if (problems.length > 0) {
  console.log(`\nRESULT: ${problems.length} failed`)
  process.exit(1)
}
console.log(`\nRESULT: ok — shot in ${SHOTS_DIR}/11-menu-about.png`)
