// That a conversation can be written to a file, and that only what was asked for is.
//
// The negative assertions are the interesting ones, as in `drive-markdown.mjs`. By default an
// export carries what was asked and what came back — not the tool lines, not the diffs, not
// the approval cards — and a file that quietly included one of those would read like a record
// of the exchange without being one. So this checks what must NOT be in the file as carefully
// as what must.
//
// "Include Tool Calls" moves exactly one of those things across the line, and the second half
// of the run holds it to that: with the setting on the calls appear and the diffs and cards
// still do not, and the footer changes to match what the file actually carried.
//
// Two behaviours here are covered nowhere else in the suite: that a menu anchored to a control
// at the bottom of the window flips *above* it, and that the offscreen window a PDF is printed
// from is destroyed afterwards. A leaked print window is invisible and would never be noticed.
//
// Costs nothing: it opens a stored session rather than sending a prompt. The save sheet is
// modal and would hang the run, so `dialog.showSaveDialog` is replaced in the main process —
// the same trick `drive-menu.mjs` plays on `Menu.prototype.popup`.
import { _electron as electron } from 'playwright-core'
import { mkdirSync, existsSync, readFileSync, statSync, rmSync } from 'node:fs'
import { join } from 'node:path'

const OUT = '/tmp/bravebot-ui'
mkdirSync(OUT, { recursive: true })

const problems = []
const check = (ok, what) => {
  console.log(`${ok ? '  ok  ' : ' FAIL '} ${what}`)
  if (!ok) problems.push(what)
}
const skip = (what) => console.log(`  --   ${what}`)

const written = []
const cleanup = () => {
  for (const path of written) rmSync(path, { force: true })
}

const app = await electron.launch({ args: ['.'], cwd: process.cwd(), timeout: 40000 })
const page = await app.firstWindow()
await page.waitForLoadState('domcontentloaded')
page.on('pageerror', (e) => console.log('PAGE ERROR:', e.message))
page.on('console', (m) => m.type() === 'error' && console.log('CONSOLE ERROR:', m.text()))
await page.waitForTimeout(2200)

/** The whole menu, flattened, so the File > Export items can be found wherever they sit. */
const flatten = (items) =>
  items.flatMap((i) => [i, ...(i.items ? flatten(i.items) : [])])

const readMenu = () =>
  app.evaluate(({ Menu }) => {
    const walk = (menu) =>
      menu.items.map((i) => ({
        id: i.id,
        label: i.label,
        enabled: i.enabled,
        checked: i.checked,
        items: i.submenu ? walk(i.submenu) : null,
      }))
    const menu = Menu.getApplicationMenu()
    return menu ? walk(menu) : null
  })

const FORMAT_IDS = ['session.export-text', 'session.export-markdown', 'session.export-pdf']

const exportItems = async () => {
  const menu = await readMenu()
  return flatten(menu ?? []).filter((i) => FORMAT_IDS.includes(i.id))
}

/** The File menu's copy of the tool-calls setting, which carries the tick. */
const toolsItem = async () => {
  const menu = await readMenu()
  return flatten(menu ?? []).find((i) => i.id === 'session.export-tools') ?? null
}

// --- with nothing open ------------------------------------------------------------------
const button = page.locator('.export-open')

// The empty state draws no composer at all, so with nothing open the button is absent rather
// than grey. Both are correct ways of saying "there is nothing to export"; what matters is
// that it cannot be pressed, and that the menu bar — which is always there — says the same.
check(await button.count() === 0, 'there is no export button with no session open')
const idleItems = await exportItems()
check(idleItems.length === 3, 'the File menu offers three export formats')
check(
  idleItems.every((i) => !i.enabled),
  'and every one of them is grey with no session open',
)
const idleTools = await toolsItem()
check(idleTools !== null, 'and a row above them for what goes in the file')
// Enabled with nothing open, unlike the formats: it is a setting, not an action.
check(idleTools?.enabled === true, 'which is not greyed with the formats')
check(idleTools?.checked === false, 'and starts off, so an export carries the conversation')

// --- open a stored session --------------------------------------------------------------
const rows = page.locator('.session-list .session')
const count = await rows.count()

if (count === 0) {
  skip('no stored sessions on this machine — the content assertions need one')
} else {
  await rows.first().click()
  await page.waitForTimeout(2500)

  check(await button.count() === 1, 'opening a session puts an export button in the composer')
  const enabled = await button.isEnabled().catch(() => false)
  check(enabled, 'and it is live once something has been said')
  check(
    (await exportItems()).every((i) => i.enabled),
    'and un-greys the three File > Export items',
  )

  // --- the menu -------------------------------------------------------------------------
  await button.click()
  await page.waitForTimeout(250)

  const menu = page.locator('[role="menu"]')
  check(await menu.count() === 1, 'the button opens exactly one menu')
  check(
    (await button.getAttribute('aria-expanded')) === 'true',
    'and says so with aria-expanded',
  )

  // `menuitem` and not `menuitemcheckbox`, so this counts the formats and not the setting
  // sitting above them.
  const labels = await page.locator('[role="menuitem"] .popitem-label').allTextContents()
  check(
    labels.join() === 'Plain Text,Markdown,PDF',
    `it offers the three formats in order (${labels.join(', ')})`,
  )
  const details = await page.locator('[role="menuitem"] .popitem-detail').allTextContents()
  check(details.join() === '.txt,.md,.pdf', `each named by its extension (${details.join(', ')})`)

  const toggle = page.locator('[role="menuitemcheckbox"]')
  check(await toggle.count() === 1, 'above them sits the tool-calls setting')
  check(
    (await toggle.getAttribute('aria-checked')) === 'false',
    'announced as a checkbox that is off',
  )

  // The one behaviour a bottom-of-window menu depends on, and nothing else covers it.
  const anchorBox = await button.boundingBox()
  const menuBox = await menu.boundingBox()
  check(
    menuBox && anchorBox && menuBox.y + menuBox.height <= anchorBox.y + 1,
    'the menu flips above the button rather than off the bottom of the window',
  )

  await page.screenshot({ path: join(OUT, '15-export-menu.png') })

  // Escape closes it and hands focus back — `PopMenu`'s contract, also otherwise untested.
  await page.keyboard.press('Escape')
  await page.waitForTimeout(200)
  check(await page.locator('[role="menu"]').count() === 0, 'Escape closes the menu')
  check(
    await button.evaluate((el) => el === document.activeElement),
    'and focus goes back to the button that opened it',
  )

  // --- stub the save sheet --------------------------------------------------------------
  await app.evaluate(({ dialog }, dir) => {
    globalThis.__asked = []
    globalThis.__cancel = false
    globalThis.__target = ''
    dialog.showSaveDialog = async (_window, options) => {
      globalThis.__asked.push({ defaultPath: options.defaultPath, filters: options.filters })
      return globalThis.__cancel
        ? { canceled: true, filePath: undefined }
        : { canceled: false, filePath: globalThis.__target }
    }
  }, OUT)

  const choose = async (label) => {
    await button.click()
    await page.waitForTimeout(200)
    await page.locator('[role="menuitem"]', { hasText: label }).first().click()
  }

  // --- markdown -------------------------------------------------------------------------
  const mdPath = join(OUT, 'export-test.md')
  written.push(mdPath)
  rmSync(mdPath, { force: true })
  await app.evaluate((_e, p) => { globalThis.__target = p }, mdPath)
  await choose('Markdown')
  await page.waitForTimeout(1500)

  const asked = await app.evaluate(() => globalThis.__asked)
  const suggested = asked[0]?.defaultPath ?? ''
  check(suggested.endsWith('.md'), `the sheet suggests a .md name (${suggested})`)
  const base = suggested.split('/').pop() ?? ''
  check(
    !/[\u0000-\u001f\u007f:\\]/.test(base) && !base.startsWith('.'),
    'and the suggested name carries nothing that could escape a directory',
  )
  check(asked[0]?.filters?.[0]?.name === 'Markdown', 'with the Markdown filter')

  check(existsSync(mdPath), 'the markdown file is written')
  if (existsSync(mdPath)) {
    const text = readFileSync(mdPath, 'utf8')
    check(text.startsWith('# '), 'it opens with the session title as a heading')
    check(text.includes('**You**'), 'it names who spoke')
    check(
      text.includes('Tool calls, diffs and approvals are not part of this export.'),
      'and says plainly what it left out',
    )
    // The assertion the whole scope decision is for, done by counting rather than by
    // pattern-matching. An earlier version of this looked for lines starting `- ` or `+ `
    // as a proxy for diff hunks, and a reply containing an ordinary markdown bullet list
    // failed it — the file was correct and the test was wrong. What actually needs proving
    // is that the file holds the conversation and nothing besides, so: one heading per
    // bubble on screen, no more and no fewer. A leaked tool line, diff or approval card
    // would push the count up; a dropped reply would push it down.
    const onScreen = await page.locator('.bubble.user, .bubble.assistant').count()
    const headings = (text.match(/^\*\*(You|Brave Bot)\*\*$/gm) ?? []).length
    check(
      headings === onScreen && onScreen > 0,
      `the file holds exactly the conversation (${headings} turns for ${onScreen} bubbles)`,
    )
    // The markers the transcript uses for the things an export leaves out. Narrow strings
    // rather than words a reply could legitimately contain.
    check(
      !/unchanged line/.test(text) && !text.includes('⋯'),
      'no diff hunk or its elision marker reached the file',
    )
  }
  check(
    await page.locator('.notice').count() === 1,
    'a saved file is reported in the window',
  )
  await page.screenshot({ path: join(OUT, '16-export-saved.png') })
  await page.locator('.notice button').click()
  await page.waitForTimeout(200)

  // --- the same session, with the calls in it ---------------------------------------------
  //
  // The setting is flipped from the in-window menu and read back off the *File* menu: those
  // are two views of one value, and the whole reason it is published to the main process is
  // that they must not be able to disagree.
  const onScreenCalls = await page.locator('.tool').count()
  await button.click()
  await page.waitForTimeout(200)
  await page.locator('[role="menuitemcheckbox"]').click()
  await page.waitForTimeout(400)
  check((await toolsItem())?.checked === true, 'ticking the row ticks the File menu item too')
  await button.click()
  await page.waitForTimeout(200)
  check(
    (await page.locator('[role="menuitemcheckbox"]').getAttribute('aria-checked')) === 'true',
    'and the menu shows it on when it is reopened',
  )
  await page.keyboard.press('Escape')
  await page.waitForTimeout(200)

  const withToolsPath = join(OUT, 'export-test-tools.md')
  written.push(withToolsPath)
  rmSync(withToolsPath, { force: true })
  await app.evaluate((_e, p) => { globalThis.__target = p }, withToolsPath)
  await choose('Markdown')
  await page.waitForTimeout(1500)

  check(existsSync(withToolsPath), 'the file is written with the setting on')
  if (existsSync(withToolsPath)) {
    const text = readFileSync(withToolsPath, 'utf8')
    const calls = (text.match(/^- `/gm) ?? []).length
    check(
      calls === onScreenCalls && onScreenCalls > 0,
      `it carries every call and no more (${calls} lines for ${onScreenCalls} on screen)`,
    )
    // The conversation is untouched by the setting: the same turns, in the same places.
    const onScreenTurns = await page.locator('.bubble.user, .bubble.assistant').count()
    const headings = (text.match(/^\*\*(You|Brave Bot)\*\*$/gm) ?? []).length
    check(headings === onScreenTurns, 'and still exactly the conversation besides')
    // The footer is the file's own account of what it dropped, so it has to have changed
    // with the contents. A file full of tool lines saying it left them out is worse than one
    // that never offered the setting.
    check(
      text.includes('Diffs, approvals and confined output are not part of this export.') &&
        !text.includes('Tool calls, diffs and approvals are not part of this export.'),
      'and its footer no longer claims to have left the calls out',
    )
    // The line the setting does *not* move: the evidence is still absent.
    check(
      !/unchanged line/.test(text) && !text.includes('⋯'),
      'no diff hunk crossed with the calls',
    )
  }
  await page.locator('.notice button').click().catch(() => undefined)
  await page.waitForTimeout(200)

  // Back off, so the PDF below is the default document.
  await button.click()
  await page.waitForTimeout(200)
  await page.locator('[role="menuitemcheckbox"]').click()
  await page.waitForTimeout(300)
  check((await toolsItem())?.checked === false, 'and the setting turns back off')

  // --- pdf ------------------------------------------------------------------------------
  const pdfPath = join(OUT, 'export-test.pdf')
  written.push(pdfPath)
  rmSync(pdfPath, { force: true })
  await app.evaluate((_e, p) => { globalThis.__target = p }, pdfPath)
  await choose('PDF')
  // Laying out and printing a document takes longer than writing a string.
  for (let waited = 0; waited < 30000 && !existsSync(pdfPath); waited += 500) {
    await page.waitForTimeout(500)
  }
  check(existsSync(pdfPath), 'the pdf is written')
  if (existsSync(pdfPath)) {
    check(
      readFileSync(pdfPath).subarray(0, 5).toString() === '%PDF-',
      'and it really is a pdf',
    )
    check(statSync(pdfPath).size > 2000, 'with a document in it rather than an empty page')
  }
  await page.waitForTimeout(500)
  check(
    (await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows().length)) === 1,
    'the window the pdf was printed from is gone afterwards',
  )
  await page.locator('.notice button').click().catch(() => undefined)
  await page.waitForTimeout(200)

  // --- a cancelled sheet is silent ------------------------------------------------------
  const txtPath = join(OUT, 'export-test.txt')
  written.push(txtPath)
  rmSync(txtPath, { force: true })
  await app.evaluate((_e, p) => {
    globalThis.__cancel = true
    globalThis.__target = p
  }, txtPath)
  await choose('Plain Text')
  await page.waitForTimeout(800)
  check(!existsSync(txtPath), 'cancelling the sheet writes nothing')
  check(await page.locator('.notice').count() === 0, 'and says nothing about it')
}

// KEEP=1 leaves the exports on disk, for looking at what a change actually produced.
if (!process.env.KEEP) cleanup()
await app.close()

if (problems.length > 0) {
  console.log(`\nRESULT: ${problems.length} failed`)
  process.exit(1)
}
console.log(`\nRESULT: ok — shots in ${OUT}/15-export-menu.png, ${OUT}/16-export-saved.png`)
