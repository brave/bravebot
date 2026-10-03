// The session list with more sessions than it draws at once, and the row menu that is mounted
// only while it is open.
//
// Real Electron renderer, isolated profile (a temp --user-data-dir, never the real one) and a
// mocked bridge listing 260 sessions, the oldest 110 of them archived. No provider requests, no
// live agent. Screenshots go to SESSION_LIST_OUTPUT, or a temp directory the run prints.
//
//   npx electron-vite build && node scripts/drive-session-list.mjs
import assert from 'node:assert/strict'
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { _electron as electron } from 'playwright-core'

// PAGE in components/Sessions.tsx.
const PAGE = 100
const SESSIONS = 260
const ACTIVE = 150
const ARCHIVED = SESSIONS - ACTIVE
// Active sessions from here on are in a fifth checkout, every one of them past the first page.
const LATE = 130

const output = process.env.SESSION_LIST_OUTPUT || mkdtempSync(join(tmpdir(), 'bravebot-session-list-'))
mkdirSync(output, { recursive: true })
const profile = mkdtempSync(join(tmpdir(), 'bravebot-session-list-profile-'))
const directory = mkdtempSync(join(tmpdir(), 'bravebot-session-list-project-'))
const checkouts = Array.from({ length: 5 }, (_, n) => `${directory}-${n}`)
for (const checkout of checkouts) mkdirSync(checkout)
const now = Math.floor(Date.now() / 1000)
// Newest first, ten minutes apart, over four checkouts and the late one.
const rows = Array.from({ length: SESSIONS }, (_, n) => {
  const checkout = n >= LATE && n < ACTIVE ? 4 : n % 4
  return {
    id: `00000000-0000-4000-8000-${String(n).padStart(12, '0')}`,
    title: `Conversation ${n}`,
    updated: now - 60 - n * 600,
    directory: checkouts[checkout],
    project: `project-${checkout}`,
    branch: 'main',
    bytes: 20,
  }
})
const archived = Object.fromEntries(rows.slice(ACTIVE).map((row) => [JSON.stringify([row.directory, row.id]), { archived: true }]))
writeFileSync(join(profile, 'experience.json'), JSON.stringify({ conversations: archived }))

const app = await electron.launch({
  args: ['.', '--disable-renderer-backgrounding', '--disable-background-timer-throttling', `--user-data-dir=${profile}`],
  cwd: process.cwd(), timeout: 40000,
})
let page
try {
  page = await app.firstWindow()
  page.setDefaultTimeout(10000)
  const errors = []
  page.on('pageerror', (e) => { errors.push(e.message); console.error('RENDERER', e.message) })
  await app.evaluate(({ ipcMain, BrowserWindow }, rows) => {
    const emit = (event, data, session) => BrowserWindow.getAllWindows()[0].webContents.send('bravebot:event', { event, data, session })
    ipcMain.removeHandler('bravebot:request')
    ipcMain.handle('bravebot:request', async (_, method, p = {}) => {
      if (method === 'agent.info') return { ok: { configured: true, build: 'session list fixture', version: '1' } }
      if (method === 'session.list') return { ok: { sessions: rows } }
      if (method === 'models.list') return { ok: { defaultModel: 'sample/fast', warnings: [], models: [{ id: 'sample/fast', name: 'Fast model', provider: 'Sample', premium: false, contextWindow: 200000, capabilities: ['text', 'tools'] }] } }
      if (method === 'session.open') {
        const record = { ...rows.find((row) => row.id === p.id), started: 1, turns: 0, tokens: 0, build: 'fixture' }
        return { ok: { session: `s-${p.id}`, model: 'sample/fast', record, said: [], todos: {}, context: '', trust: { known: true, rules: [] }, archived: 0, branchNote: null, buildNote: null, frontNote: null } }
      }
      // A turn that starts and never ends, so the session stays working after another is opened.
      if (method === 'turn.send') { emit('turn.started', { turn: 1 }, p.session); return { ok: { turn: 1 } } }
      return { ok: {} }
    })
  }, rows)
  await page.setViewportSize({ width: 1350, height: 900 })
  await page.reload()

  const snap = async (name) => { await page.waitForTimeout(300); await page.screenshot({ path: join(output, `${name}.png`), scale: 'css' }); console.log('VISUAL', name) }
  // The Bots tab has a list of the same class, after this one.
  const list = page.locator('.session-list').first()
  const drawn = list.locator('.session-row')
  const foot = list.locator(':scope > [data-test="show-more-sessions"]')
  const archive = page.locator('[data-test="session-archive"]')
  const menus = list.locator('leo-menu')
  const focused = (locator) => locator.evaluate((e) => e === document.activeElement || e.getRootNode().host === document.activeElement)
  const viewOption = async (name) => {
    await page.locator('[data-test="view-options"]').click()
    await page.getByRole('menuitemcheckbox', { name }).click()
  }

  // 1. One page of the active sessions, newest first, and a row that offers the rest. Waited on
  // by its words, since the archived marks arrive after the list does.
  await foot.filter({ hasText: /^Show 50 more$/ }).waitFor()
  assert.equal(await drawn.count(), PAGE, 'the list draws one page of rows')
  assert.equal(await drawn.first().locator('.session-name').textContent(), 'Conversation 0')
  assert.equal(await drawn.last().locator('.session-name').textContent(), `Conversation ${PAGE - 1}`)
  assert.equal(await archive.count(), 0, 'the archive stays hidden until asked for')

  // 2. No closed row has a menu mounted, and every row still has its actions button.
  assert.equal(await list.locator('leo-menu, leo-buttonmenu, leo-menu-item').count(), 0, 'no closed row mounts a menu')
  assert.equal(await page.getByRole('button', { name: /^Actions for Conversation \d+$/ }).count(), PAGE)
  const row = drawn.filter({ hasText: 'Conversation 3' }).first()
  const trigger = page.getByRole('button', { name: 'Actions for Conversation 3', exact: true })
  assert.equal(await trigger.getAttribute('aria-haspopup'), 'menu')
  assert.equal(await trigger.getAttribute('aria-expanded'), 'false')

  // 3. A click opens one menu, on that row; Escape shuts it and gives focus back to the button.
  await row.hover()
  await trigger.click()
  const pin = page.getByRole('menuitem', { name: 'Pin conversation' })
  await pin.waitFor()
  await page.getByRole('menuitem', { name: 'Archive conversation' }).waitFor()
  assert.equal(await menus.count(), 1, 'opening a row mounts one menu')
  assert.equal(await row.evaluate((e) => e.classList.contains('menu-open')), true)
  assert.equal(await trigger.getAttribute('aria-expanded'), 'true')
  await snap('01-row-menu-open')
  await page.keyboard.press('Escape')
  await menus.waitFor({ state: 'detached' })
  assert.equal(await focused(trigger), true, 'Escape returns focus to the actions button')
  assert.equal(await row.evaluate((e) => e.classList.contains('menu-open')), false)

  // 4. The button shuts the menu it opened, and a click elsewhere shuts it too.
  await trigger.click()
  await pin.waitFor()
  await trigger.click()
  await menus.waitFor({ state: 'detached' })
  await trigger.click()
  await pin.waitFor()
  await page.mouse.click(1000, 820)
  await menus.waitFor({ state: 'detached' })
  assert.equal(await focused(trigger), false, 'a click elsewhere leaves focus where it landed')

  // 5. From the keyboard: Enter opens it, the arrows reach an item, Enter chooses it. Pinning
  // moves the row to the top, and focus stays on its button.
  await trigger.focus()
  await page.keyboard.press('Enter')
  await pin.waitFor()
  await page.keyboard.press('ArrowDown')
  assert.equal(await pin.evaluate((e) => e === document.activeElement), true, 'ArrowDown reaches the first item')
  await page.keyboard.press('Enter')
  await menus.waitFor({ state: 'detached' })
  await drawn.first().getByLabel('Pinned', { exact: true }).waitFor()
  assert.equal(await drawn.first().locator('.session-name').textContent(), 'Conversation 3')
  assert.equal(await focused(trigger), true, 'choosing an item returns focus to the actions button')
  assert.equal(await drawn.count(), PAGE, 'a pinned row is still one of the page')
  await trigger.click()
  await page.getByRole('menuitem', { name: 'Unpin conversation' }).click()
  await menus.waitFor({ state: 'detached' })
  assert.equal(await drawn.first().locator('.session-name').textContent(), 'Conversation 0')

  // 6. The search runs over every session, not the page that is drawn: one past the page, and
  // one past the archive's page.
  const search = page.getByRole('searchbox', { name: 'Filter sessions', exact: true })
  await search.fill('Conversation 140')
  await drawn.filter({ hasText: 'Conversation 140' }).waitFor()
  assert.equal(await drawn.count(), 1, 'a search finds a session past the first page')
  assert.equal(await foot.count(), 0)
  await search.fill(`Conversation ${SESSIONS - 5}`)
  await archive.locator('.session-row').filter({ hasText: `Conversation ${SESSIONS - 5}` }).waitFor()
  assert.equal(await drawn.count(), 1, 'a search finds an archived session past the archive page')
  await search.fill('')
  await foot.filter({ hasText: /^Show 50 more$/ }).waitFor()

  // 7. A session opened from past the page stays drawn when the search is cleared, after the
  // page, and so does one still working once another is opened.
  const late = drawn.filter({ hasText: 'Conversation 140' })
  await search.fill('Conversation 140')
  await late.locator('button.session').click()
  await late.and(page.locator('.current')).waitFor()
  await search.fill('')
  await foot.filter({ hasText: /^Show 49 more$/ }).waitFor()
  assert.equal(await drawn.count(), PAGE + 1, 'the open session is drawn past the page')
  assert.equal(await drawn.nth(PAGE).locator('.session-name').textContent(), 'Conversation 140')
  await page.getByRole('textbox', { name: 'Message the agent' }).fill('Keep going')
  await page.getByRole('button', { name: 'Send', exact: true }).click()
  await late.locator('[data-status="working"]').waitFor()
  await drawn.first().locator('button.session').click()
  await drawn.first().and(page.locator('.current')).waitFor()
  await late.locator('[data-status="working"]').waitFor()
  assert.equal(await late.evaluate((e) => e.classList.contains('current')), false)
  assert.equal(await drawn.count(), PAGE + 1, 'a working session is drawn past the page')

  // 8. Grouped: every checkout keeps its heading, which counts all of its sessions, even the
  // late one with nothing on the first page. Each group draws its share of that page, the
  // working session where it falls, and a row for the rest of its own.
  await viewOption('Group by project')
  const groups = list.locator(':scope > .session-group-section')
  await groups.first().waitFor()
  const counts = (await groups.locator('.session-group-head .count').allTextContents()).map(Number)
  assert.deepEqual(counts, [33, 33, 32, 32, ACTIVE - LATE], 'each heading counts every session in its checkout')
  await page.getByRole('button', { name: 'New session in project-4', exact: true }).waitFor()
  assert.equal(await drawn.count(), PAGE + 1)
  const groupFeet = groups.locator('[data-test="show-more-sessions"]')
  assert.deepEqual(await groupFeet.allTextContents(), ['Show 8 more', 'Show 8 more', 'Show 7 more', 'Show 7 more', 'Show 19 more'])
  assert.equal(await foot.count(), 0, 'grouped, each group has its own row instead')
  const lateGroup = groups.nth(4)
  await lateGroup.locator('[data-test="show-more-sessions"]').scrollIntoViewIfNeeded()
  await snap('02-grouped-foot')
  await lateGroup.locator('[data-test="show-more-sessions"]').click()
  await lateGroup.locator('.session-row').nth(ACTIVE - LATE - 1).waitFor()
  assert.equal(await lateGroup.locator('[data-test="show-more-sessions"]').count(), 0)
  assert.equal(await focused(lateGroup.locator('.session-row').first().locator('button.session')), true,
    'focus moves to the first row Show more drew')
  assert.equal(await lateGroup.locator('.session-row').first().locator('.session-name').textContent(), `Conversation ${LATE}`)
  await viewOption('Group by project')
  await groups.first().waitFor({ state: 'detached' })

  // 9. The row draws the rest; with nothing left, it goes, and focus moves to the first row it drew.
  await list.evaluate((e) => { e.scrollTop = e.scrollHeight })
  await foot.hover()
  await snap('03-show-more')
  await foot.click()
  await drawn.nth(ACTIVE - 1).waitFor()
  assert.equal(await drawn.count(), ACTIVE, 'Show more draws the rest')
  assert.equal(await foot.count(), 0, 'and is gone once nothing is left')
  assert.equal(await focused(drawn.nth(PAGE).locator('button.session')), true, 'focus moves to the first row Show more drew')
  assert.equal(await drawn.nth(PAGE).locator('.session-name').textContent(), `Conversation ${PAGE}`)

  // 10. The archive has a page of its own, under a heading that counts all of it.
  await viewOption('Show archived')
  await archive.waitFor()
  const archivedRows = archive.locator('.session-row')
  const archiveFoot = archive.locator('[data-test="show-more-sessions"]')
  await archiveFoot.filter({ hasText: `Show ${ARCHIVED - PAGE} more` }).waitFor()
  assert.equal(await archivedRows.count(), PAGE, 'the archive draws one page of rows')
  assert.equal(await archive.locator('.session-group-head .count').textContent(), String(ARCHIVED), 'the archive heading counts every archived session')
  await list.evaluate((e) => { e.scrollTop = e.scrollHeight })
  await snap('04-archive-foot')
  await archiveFoot.click()
  await archivedRows.nth(ARCHIVED - 1).waitFor()
  assert.equal(await archivedRows.count(), ARCHIVED)
  assert.equal(await archiveFoot.count(), 0)
  assert.equal(await focused(archivedRows.nth(PAGE).locator('button.session')), true, 'focus moves to the first archived row Show more drew')
  await viewOption('Show archived')
  await archive.waitFor({ state: 'detached' })

  // 11. Dark, with the menu open, for the record.
  await page.emulateMedia({ colorScheme: 'dark' })
  await list.evaluate((e) => { e.scrollTop = 0 })
  await drawn.first().hover()
  await page.getByRole('button', { name: 'Actions for Conversation 0', exact: true }).click()
  await pin.waitFor()
  await snap('05-row-menu-open-dark')
  await page.keyboard.press('Escape')
  await menus.waitFor({ state: 'detached' })

  assert.deepEqual(errors, [])
  console.log('PASS: session list', { output })
} catch (error) {
  if (page) await page.screenshot({ path: join(output, 'failure.png') }).catch(() => {})
  throw error
} finally {
  await app.close()
  rmSync(profile, { recursive: true, force: true })
  rmSync(directory, { recursive: true, force: true })
  for (const checkout of checkouts) rmSync(checkout, { recursive: true, force: true })
}
