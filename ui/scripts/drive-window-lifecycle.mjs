// File > New Chat after the main window was closed, on macOS where closing the last window leaves
// the app and its menu bar running.
//
// Before the fix the menu still pointed at the destroyed window, so choosing New Chat raised
// "Object has been destroyed" in the main process. Now the items that need a window grey out when
// it closes, New Chat builds a new window, and the command reaches that window's page once the
// page has subscribed, so a chat starts.
//
// Real Electron, a temporary profile and state directory, and a mocked bridge that records each
// `session.new`. The folder is the one a mocked picker returns, as in `drive-close-chat.mjs`. The
// main process's uncaught exceptions are collected and must stay empty.
//
//   npx electron-vite build && node scripts/drive-window-lifecycle.mjs
import assert from 'node:assert/strict'
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { oneModelList } from './acceptance-fixture.mjs'
import { launchMocked } from './drive-mock.mjs'

if (process.platform !== 'darwin') {
  console.log('SKIP: closing the last window quits the app off macOS, so there is no menu left to choose from')
  process.exit(0)
}

const profile = mkdtempSync(join(tmpdir(), 'bravebot-lifecycle-profile-'))
const home = mkdtempSync(join(tmpdir(), 'bravebot-lifecycle-home-'))
const directory = mkdtempSync(join(tmpdir(), 'bravebot-lifecycle-project-'))
writeFileSync(join(profile, 'experience.json'), JSON.stringify({ conversations: {} }))

const { app, page: first } = await launchMocked({ profile, env: { HOME: home, BRAVEBOT_HOME: join(home, '.bravebot') }, timeout: 15000 })
try {
  await app.evaluate(({ ipcMain }, { models, directory }) => {
    globalThis.__uncaught = []
    process.on('uncaughtException', (error) => globalThis.__uncaught.push(String(error?.message ?? error)))
    globalThis.__drafts = 0
    ipcMain.removeHandler('bravebot:choose-directory')
    ipcMain.handle('bravebot:choose-directory', () => directory)
    ipcMain.removeHandler('bravebot:request')
    ipcMain.handle('bravebot:request', async (_, method, p = {}) => {
      if (method === 'agent.info') return { ok: { configured: true, build: 'lifecycle fixture', version: '1' } }
      if (method === 'session.list') return { ok: { sessions: [] } }
      if (method === 'models.list') return { ok: models }
      if (method === 'session.new') {
        globalThis.__drafts += 1
        return { ok: { session: `s-draft-${globalThis.__drafts}`, branch: 'main', model: 'sample/fast', shownDirectory: p.directory, remembered: { at: 1, path: p.directory, shownPath: p.directory }, autoVetting: false, permissionMode: 'ask', sandboxMode: 'workspace-write' } }
      }
      return { ok: {} }
    })
  }, { models: oneModelList, directory })
  await first.reload()
  await first.getByRole('button', { name: 'New chat', exact: true }).waitFor()

  const menuItem = (id) => app.evaluate(({ Menu }, id) => {
    const found = Menu.getApplicationMenu().getMenuItemById(id)
    return { enabled: found.enabled }
  }, id)
  const choose = (id) => app.evaluate(({ Menu }, id) => Menu.getApplicationMenu().getMenuItemById(id).click(), id)
  const uncaught = () => app.evaluate(() => globalThis.__uncaught)
  const windowCount = () => app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows().length)

  // A chat is open before the window closes, so Close Chat and Cancel Turn were last drawn for it.
  await first.getByRole('button', { name: 'New chat', exact: true }).click()
  await first.locator('.session-row.current').waitFor()
  assert.equal((await menuItem('session.close')).enabled, true, 'Close Chat is enabled while a chat is open')

  let current = first
  for (const cycle of [1, 2]) {
    const reopened = app.waitForEvent('window', { timeout: 20000 })
    reopened.catch(() => {})
    const closed = current.waitForEvent('close')
    closed.catch(() => {})
    await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows().forEach((window) => window.close()))
    await closed
    const gone = await windowCount()
    assert.equal(gone, 0, `cycle ${cycle}: the app is running with no window`)
    for (const id of ['session.close', 'turn.send', 'turn.cancel', 'view.find']) {
      assert.equal((await menuItem(id)).enabled, false, `cycle ${cycle}: ${id} is grey with no window`)
    }
    assert.equal((await menuItem('session.new')).enabled, true, `cycle ${cycle}: New Chat stays available`)

    const before = await app.evaluate(() => globalThis.__drafts)
    await choose('session.close')
    await choose('turn.send')
    assert.equal(await windowCount(), 0, `cycle ${cycle}: a command that needs a window builds none`)

    await choose('session.new')
    const next = await reopened
    next.setDefaultTimeout(15000)
    current = next
    await next.locator('.session-row.current').waitFor()
    assert.equal(await app.evaluate(() => globalThis.__drafts), before + 1, `cycle ${cycle}: New Chat started a chat in the new window`)
    assert.equal(await windowCount(), 1, `cycle ${cycle}: one window was built`)
    assert.deepEqual(await uncaught(), [], `cycle ${cycle}: no exception in the main process`)
    assert.equal((await menuItem('session.close')).enabled, true, `cycle ${cycle}: Close Chat is enabled again for the new chat`)
  }

  console.log('PASS: File > New Chat after the main window was closed, twice')
} finally {
  await app.close()
  rmSync(profile, { recursive: true, force: true })
  rmSync(home, { recursive: true, force: true })
  rmSync(directory, { recursive: true, force: true })
}
