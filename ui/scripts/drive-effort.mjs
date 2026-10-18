// Exercise the composer's reasoning effort menu with deterministic replies; no paid inference.
import assert from 'node:assert/strict'
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { _electron as electron } from 'playwright-core'
import { COMPOSER_INPUT } from './drive-support.mjs'

const directory = '/tmp/bravebot-effort-project'
const profile = mkdtempSync(join(tmpdir(), 'bravebot-effort-'))
const app = await electron.launch({ args: ['.', `--user-data-dir=${profile}`], cwd: process.cwd(), timeout: 40000 })
try {
  const page = await app.firstWindow()
  page.setDefaultTimeout(15000)
  const errors = []
  page.on('pageerror', (error) => errors.push(error.message))
  mkdirSync(directory, { recursive: true })
  await app.evaluate(({ ipcMain, BrowserWindow, dialog }, directory) => {
    const defaultModel = 'reads'
    const rows = ['A', 'B'].map((id) => ({ id, directory, title: `Conversation ${id}`,
      project: 'effort-project', branch: null, updated: 1, bytes: 1 }))
    globalThis.effortTest = { sent: [] }
    dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [directory] })
    ipcMain.removeHandler('bravebot:request')
    ipcMain.handle('bravebot:request', async (_, method, params) => {
      if (method === 'models.list') return { ok: { defaultModel, warnings: [], models: [
        { id: 'reads', name: 'Reads effort', provider: 'Test', premium: false, contextWindow: 200000, readsEffort: true },
        { id: 'ignores', name: 'Ignores effort', provider: 'Test', premium: false, contextWindow: 200000, readsEffort: false },
      ] } }
      if (method === 'session.list') return { ok: { sessions: rows } }
      if (method === 'session.new') return { ok: { session: 's-new', directory, branch: null, model: defaultModel } }
      if (method === 'session.open') return { ok: { session: `s-${params.id}`, model: defaultModel,
        record: { ...rows.find((row) => row.id === params.id), started: 1, turns: 0, tokens: 0, build: 'fixture' },
        said: [], todos: {}, context: '', trust: { known: true, rules: [] }, archived: 0,
        branchNote: null, buildNote: null, frontNote: null } }
      if (method === 'turn.send') { globalThis.effortTest.sent.push(params); return { ok: { turn: 1 } } }
      if (method === 'manifest.run') { globalThis.effortTest.planned = params; return { ok: { run: 1, target: 't' } } }
      if (method === 'agent.info') return { ok: { build: 'fixture', version: '1', home: null } }
      return { ok: {} }
    })
    globalThis.finishTurn = (session = 's-new') => BrowserWindow.getAllWindows()[0].webContents.send('bravebot:event', {
      event: 'turn.done', session, data: { id: 'A', turn: 1, reply: 'Done', archived: 0 },
    })
  }, directory)
  await page.reload()
  await page.locator('[data-test="new-project"]').click()
  await page.getByRole('button', { name: "Don't trust", exact: true }).click()

  const trigger = page.locator('[data-test="effort-trigger"]')
  await trigger.waitFor()
  const disabled = () => trigger.evaluate((host) => host.shadowRoot?.querySelector('button')?.disabled)
  assert.equal(await trigger.getAttribute('aria-label'), 'Reasoning effort: Default')

  await trigger.click()
  await page.locator('[data-test="effort-option-xhigh"]').click()
  await page.waitForFunction(() => document.querySelector('[data-test="effort-trigger"]')?.getAttribute('aria-label') === 'Reasoning effort: Extra high')
  await page.screenshot({ path: '/tmp/bravebot-effort-picked.png' })

  const entry = page.locator(COMPOSER_INPUT)
  await entry.fill('Think hard')
  await entry.press('Enter')
  await page.waitForFunction(() => document.querySelector('[data-test="effort-trigger"]')?.shadowRoot?.querySelector('button')?.disabled)
  const sent = await app.evaluate(() => globalThis.effortTest.sent)
  assert.equal(sent[0].effort, 'xhigh')
  await app.evaluate(() => globalThis.finishTurn())
  await page.waitForFunction(() => !document.querySelector('[data-test="effort-trigger"]')?.shadowRoot?.querySelector('button')?.disabled)
  await page.waitForFunction((d) => localStorage.getItem(`bravebot.conversation-effort:${JSON.stringify([d, 'A'])}`) === 'xhigh', directory)

  // Back to Default sends null, which clears the session's level rather than keeping it.
  await trigger.click()
  await page.locator('[data-test="effort-option-default"]').click()
  await entry.fill('Leave it to settings')
  await entry.press('Enter')
  await page.waitForFunction(() => document.querySelector('[data-test="effort-trigger"]')?.shadowRoot?.querySelector('button')?.disabled)
  const cleared = await app.evaluate(() => globalThis.effortTest.sent)
  assert.equal(cleared[1].effort, null)
  await app.evaluate(() => globalThis.finishTurn())
  await page.waitForFunction(() => !document.querySelector('[data-test="effort-trigger"]')?.shadowRoot?.querySelector('button')?.disabled)
  await page.waitForFunction((d) => localStorage.getItem(`bravebot.conversation-effort:${JSON.stringify([d, 'A'])}`) === null, directory)

  // Kept per conversation, and across a reload.
  await trigger.click()
  await page.locator('[data-test="effort-option-low"]').click()
  await page.locator('.session').filter({ hasText: 'Conversation B' }).click()
  await page.waitForFunction(() => document.querySelector('[data-test="effort-trigger"]')?.getAttribute('aria-label') === 'Reasoning effort: Default')
  await page.locator('.session').filter({ hasText: /Conversation A|Think hard/ }).click()
  await page.waitForFunction(() => document.querySelector('[data-test="effort-trigger"]')?.getAttribute('aria-label') === 'Reasoning effort: Low')
  await page.reload()
  await page.locator('.session').filter({ hasText: /Conversation A|Think hard/ }).click()
  await page.waitForFunction(() => document.querySelector('[data-test="effort-trigger"]')?.getAttribute('aria-label') === 'Reasoning effort: Low')

  // A model that does not take a level greys the menu and says why.
  await page.locator('.model-trigger').click()
  await page.locator('leo-option').filter({ hasText: 'Ignores effort' }).click()
  await page.waitForFunction(() => document.querySelector('[data-test="effort-trigger"]')?.shadowRoot?.querySelector('button')?.disabled)
  assert.equal(await trigger.getAttribute('data-tooltip'), 'This model does not take a reasoning effort level')
  assert.equal(await disabled(), true)
  await page.screenshot({ path: '/tmp/bravebot-effort-ignored.png' })

  // The kept level is not sent to a model that ignores it, as the terminal does.
  await entry.fill('No level for this one')
  await entry.press('Enter')
  await page.waitForFunction(() => document.querySelector('.model-trigger')?.shadowRoot?.querySelector('button')?.disabled)
  const ignoredSent = (await app.evaluate(() => globalThis.effortTest.sent)).at(-1)
  assert.equal(ignoredSent.model, 'ignores')
  assert.equal(ignoredSent.effort, null)
  await app.evaluate(() => globalThis.finishTurn('s-A'))
  await page.waitForFunction(() => !document.querySelector('.model-trigger')?.shadowRoot?.querySelector('button')?.disabled)

  // A plan run carries the level, as an ordinary turn does.
  await page.locator('.model-trigger').click()
  await page.locator('leo-option').filter({ hasText: 'Reads effort' }).click()
  await page.waitForFunction(() => !document.querySelector('[data-test="effort-trigger"]')?.shadowRoot?.querySelector('button')?.disabled)
  assert.equal(await trigger.getAttribute('aria-label'), 'Reasoning effort: Low')
  await entry.fill('Plan this')
  await page.locator('[data-test="composer-mode"]').click()
  await page.locator('[data-test="mode-plan"]').click()
  await page.locator('[data-test="send-message"]').click()
  await page.waitForFunction(() => document.querySelector('[data-test="effort-trigger"]')?.shadowRoot?.querySelector('button')?.disabled)
  const planned = await app.evaluate(() => globalThis.effortTest.planned)
  assert.equal(planned.task, 'Plan this')
  assert.equal(planned.model, 'reads')
  assert.equal(planned.effort, 'low')

  assert.deepEqual(errors, [])
  console.log('PASS: effort default, choice, turn payload, clearing, running state, per-conversation persistence, a model that ignores it, and a plan run')
} finally {
  await app.close()
  rmSync(profile, { recursive: true, force: true })
  rmSync(directory, { recursive: true, force: true })
}
