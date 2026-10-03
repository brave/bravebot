// The session list's row menu, which is mounted only while it is open.
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

const SESSIONS = 260
const ACTIVE = 150

const output = process.env.SESSION_LIST_OUTPUT || mkdtempSync(join(tmpdir(), 'bravebot-session-list-'))
mkdirSync(output, { recursive: true })
const profile = mkdtempSync(join(tmpdir(), 'bravebot-session-list-profile-'))
const directory = mkdtempSync(join(tmpdir(), 'bravebot-session-list-project-'))
const now = Math.floor(Date.now() / 1000)
// Newest first, over four checkouts, ten minutes apart.
const rows = Array.from({ length: SESSIONS }, (_, n) => ({
  id: `00000000-0000-4000-8000-${String(n).padStart(12, '0')}`,
  title: `Conversation ${n}`,
  updated: now - 60 - n * 600,
  directory: `${directory}-${n % 4}`,
  project: `project-${n % 4}`,
  branch: 'main',
  bytes: 20,
}))
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
  await app.evaluate(({ ipcMain }, rows) => {
    ipcMain.removeHandler('bravebot:request')
    ipcMain.handle('bravebot:request', async (_, method) => {
      if (method === 'agent.info') return { ok: { configured: true, build: 'session list fixture', version: '1' } }
      if (method === 'session.list') return { ok: { sessions: rows } }
      return { ok: {} }
    })
  }, rows)
  await page.setViewportSize({ width: 1350, height: 900 })
  await page.reload()

  const snap = async (name) => { await page.waitForTimeout(300); await page.screenshot({ path: join(output, `${name}.png`), scale: 'css' }); console.log('VISUAL', name) }
  // The Bots tab has a list of the same class, after this one.
  const list = page.locator('.session-list').first()
  const drawn = list.locator('.session-row')
  const menus = list.locator('leo-menu')
  const focused = (locator) => locator.evaluate((e) => e === document.activeElement || e.getRootNode().host === document.activeElement)

  // 1. The active sessions, newest first. Waited on by their number, since the archived marks
  // arrive after the list does.
  await page.waitForFunction((n) => document.querySelector('.session-list').querySelectorAll('.session-row').length === n, ACTIVE)
  assert.equal(await drawn.first().locator('.session-name').textContent(), 'Conversation 0')

  // 2. No closed row has a menu mounted, and every row still has its actions button.
  assert.equal(await list.locator('leo-menu, leo-buttonmenu, leo-menu-item').count(), 0, 'no closed row mounts a menu')
  assert.equal(await page.getByRole('button', { name: /^Actions for Conversation \d+$/ }).count(), ACTIVE)
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
  await trigger.click()
  await page.getByRole('menuitem', { name: 'Unpin conversation' }).click()
  await menus.waitFor({ state: 'detached' })
  assert.equal(await drawn.first().locator('.session-name').textContent(), 'Conversation 0')

  // 6. Dark, with the menu open, for the record.
  await page.emulateMedia({ colorScheme: 'dark' })
  await list.evaluate((e) => { e.scrollTop = 0 })
  await drawn.first().hover()
  await page.getByRole('button', { name: 'Actions for Conversation 0', exact: true }).click()
  await pin.waitFor()
  await snap('02-row-menu-open-dark')
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
}
