// Close Chat on a sidebar row's right-click menu, for a chat that is not the one on screen.
//
// Every chat the window opens stays open behind it after another is chosen, and its row says so
// while a turn runs. Close Chat on such a row has to tell the agent to close that chat's handle
// and drop the row's working mark, and it has to do the same for a draft row, whose id is the
// draft's and not a record's.
//
// Real Electron renderer, isolated profile (a temp --user-data-dir) and a mocked bridge. The
// bridge records each `session.close` it is sent. The menu is the real native one built by the
// main process: `Menu.prototype.popup` is replaced to choose the item named in `__choose` at once,
// which is the route a click takes (menu item, `bravebot:command`, the command router, `App`).
// No provider requests, no live agent.
//
//   npx electron-vite build && node scripts/drive-close-chat.mjs
import assert from 'node:assert/strict'
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { _electron as electron } from 'playwright-core'
import { oneModelList } from './acceptance-fixture.mjs'
import { COMPOSER_NAME } from './drive-support.mjs'

const profile = mkdtempSync(join(tmpdir(), 'bravebot-close-chat-profile-'))
const directory = mkdtempSync(join(tmpdir(), 'bravebot-close-chat-project-'))
writeFileSync(join(profile, 'experience.json'), JSON.stringify({ conversations: {} }))
const now = Math.floor(Date.now() / 1000)
const rows = ['First chat', 'Second chat', 'Third chat'].map((title, n) => ({
  id: `00000000-0000-4000-8000-${String(n).padStart(12, '0')}`,
  title,
  updated: now - 60 - n * 600,
  directory,
  project: 'project',
  branch: 'main',
  bytes: 20,
}))

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
  await app.evaluate(({ ipcMain, BrowserWindow, Menu }, { rows, models, directory }) => {
    const emit = (event, data, session) => BrowserWindow.getAllWindows()[0].webContents.send('bravebot:event', { event, data, session })
    globalThis.__closed = []
    globalThis.__drafts = 0
    ipcMain.removeHandler('bravebot:choose-directory')
    ipcMain.handle('bravebot:choose-directory', () => directory)
    ipcMain.removeHandler('bravebot:request')
    ipcMain.handle('bravebot:request', async (_, method, p = {}) => {
      if (method === 'agent.info') return { ok: { configured: true, build: 'close chat fixture', version: '1' } }
      if (method === 'session.list') return { ok: { sessions: rows } }
      if (method === 'models.list') return { ok: models }
      if (method === 'session.open') {
        const record = { ...rows.find((row) => row.id === p.id), started: 1, turns: 0, tokens: 0, build: 'fixture' }
        return { ok: { session: `s-${p.id}`, model: 'sample/fast', record, said: [], todos: {}, context: '', trust: { known: true, rules: [] }, archived: 0, branchNote: null, buildNote: null, frontNote: null } }
      }
      if (method === 'session.new') {
        globalThis.__drafts += 1
        return { ok: { session: `s-draft-${globalThis.__drafts}`, branch: 'main', model: 'sample/fast', shownDirectory: p.directory, remembered: { at: 1, path: p.directory, shownPath: p.directory }, autoVetting: false, permissionMode: 'ask', sandboxMode: 'workspace-write' } }
      }
      if (method === 'session.close') { globalThis.__closed.push(p.session); return { ok: {} } }
      // A turn that starts and never ends, so the session stays working after another is opened.
      if (method === 'turn.send') { emit('turn.started', { turn: 1 }, p.session); return { ok: { turn: 1 } } }
      return { ok: {} }
    })
    // A real popup is modal. This chooses the named item of the menu the main process built.
    Menu.prototype.popup = function popup() {
      const item = this.items.find((each) => each.id === globalThis.__choose)
      if (item) item.click()
    }
  }, { rows, models: oneModelList, directory })
  await page.setViewportSize({ width: 1350, height: 900 })
  await page.reload()

  const list = page.locator('.session-list').first()
  const rowOf = (title) => list.locator('.session-row').filter({ hasText: title })
  const closed = () => app.evaluate(() => globalThis.__closed)
  const closeFromMenu = async (row) => {
    await app.evaluate(() => { globalThis.__choose = 'context.session.close' })
    await row.locator('button.session').click({ button: 'right' })
  }
  const showChat = async (title) => {
    await rowOf(title).locator('button.session').click()
    await rowOf(title).and(page.locator('.current')).waitFor()
  }
  const newChat = async () => {
    const before = await list.locator('.session-row').count()
    await page.getByRole('button', { name: 'New chat', exact: true }).click()
    await rowOf('New Chat').and(page.locator('.current')).waitFor()
    return before
  }
  const sessionRows = list.locator('.session-row')
  await rowOf('First chat').waitFor()

  // 1. A chat still working behind the one on screen is closed from its own row.
  await showChat('First chat')
  await page.getByRole('textbox', { name: COMPOSER_NAME }).fill('Keep going')
  await page.getByRole('button', { name: 'Send', exact: true }).click()
  await rowOf('First chat').locator('[data-status="working"]').waitFor()
  await showChat('Second chat')
  await rowOf('First chat').locator('[data-status="working"]').waitFor()
  assert.deepEqual(await closed(), [])
  await closeFromMenu(rowOf('First chat'))
  await rowOf('First chat').locator('[data-status="working"]').waitFor({ state: 'detached' })
  assert.deepEqual(await closed(), [`s-${rows[0].id}`], 'the chat behind the open one was closed, and only it')
  assert.equal(await rowOf('Second chat').evaluate((e) => e.classList.contains('current')), true, 'the chat on screen is still on screen')

  // 2. A draft behind the one on screen is closed from its own row, whose id is the draft's.
  await newChat()
  await showChat('Second chat')
  assert.equal(await rowOf('New Chat').count(), 1, 'the draft is listed while it is open behind')
  await closeFromMenu(rowOf('New Chat'))
  await rowOf('New Chat').waitFor({ state: 'detached' })
  assert.deepEqual(await closed(), [`s-${rows[0].id}`, 's-draft-1'], 'the draft behind the open chat was closed')
  assert.equal(await rowOf('Second chat').evaluate((e) => e.classList.contains('current')), true)

  // 3. The draft on screen is closed from its own row too, as File > Close Chat closes it.
  await newChat()
  await closeFromMenu(rowOf('New Chat'))
  await rowOf('New Chat').waitFor({ state: 'detached' })
  assert.deepEqual(await closed(), [`s-${rows[0].id}`, 's-draft-1', 's-draft-2'], 'the draft on screen was closed')

  // 4. A row nothing is open for has nothing to close, and says nothing about it.
  await closeFromMenu(rowOf('Third chat'))
  await page.waitForTimeout(500)
  assert.deepEqual(await closed(), [`s-${rows[0].id}`, 's-draft-1', 's-draft-2'], 'a row with no session open closed nothing')
  assert.equal(await sessionRows.count(), rows.length, 'every recorded row is still listed')

  assert.deepEqual(errors, [])
  console.log('PASS: close chat from a row')
} catch (error) {
  if (page) await page.screenshot({ path: join(tmpdir(), 'bravebot-close-chat-failure.png') }).catch(() => {})
  throw error
} finally {
  await app.close()
  rmSync(profile, { recursive: true, force: true })
  rmSync(directory, { recursive: true, force: true })
}
