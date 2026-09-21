// Exercise the real composer with deterministic model/session replies; no paid inference.
import assert from 'node:assert/strict'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { _electron as electron } from 'playwright-core'

const profile = mkdtempSync(join(tmpdir(), 'bravebot-model-picker-'))
const app = await electron.launch({ args: ['.', `--user-data-dir=${profile}`], cwd: process.cwd(), timeout: 40000 })
try {
  const page = await app.firstWindow()
  page.setDefaultTimeout(15000)
  const errors = []
  page.on('pageerror', (error) => errors.push(error.message))
  await app.evaluate(({ ipcMain, BrowserWindow }) => {
    const defaultModel = 'openrouter/anthropic/claude-haiku-4.5'
    const directory = '/tmp/bravebot-model-picker-project'
    const rows = ['A', 'B'].map((id) => ({ id, directory, title: `Conversation ${id}`,
      project: 'model-picker-project', branch: null, updated: 1, bytes: 1 }))
    globalThis.modelTest = { sent: [], fail: false, loading: false }
    ipcMain.removeHandler('bravebot:choose-directory')
    ipcMain.handle('bravebot:choose-directory', () => directory)
    ipcMain.removeHandler('bravebot:request')
    ipcMain.handle('bravebot:request', async (_, method, params) => {
      if (method === 'models.list') {
        if (globalThis.modelTest.fail) return { error: { code: 'offline', message: 'Model listing is offline.' } }
        if (globalThis.modelTest.loading) await new Promise((resolve) => setTimeout(resolve, 500))
        return { ok: { defaultModel, warnings: [], models: [
          { id: defaultModel, name: 'Claude Haiku 4.5', provider: 'OpenRouter', premium: false, contextWindow: 200000, capabilities: ['text', 'vision', 'tools'] },
          { id: 'openrouter/anthropic/claude-sonnet-4.5', name: 'Claude Sonnet 4.5', provider: 'OpenRouter', premium: false, contextWindow: 200000, capabilities: ['text', 'vision', 'tools', 'reasoning'] },
          { id: 'brave-model', name: 'Brave model', provider: 'Brave', premium: false, contextWindow: 24000 },
        ] } }
      }
      if (method === 'session.list') return { ok: { sessions: rows } }
      if (method === 'session.new') return { ok: { session: 's-new', directory, branch: null, model: defaultModel } }
      if (method === 'session.open') return { ok: { session: `s-${params.id}`, model: defaultModel,
        record: { ...rows.find((row) => row.id === params.id), started: 1, turns: 0, tokens: 0, build: 'fixture' },
        said: [], todos: {}, context: '', trust: { known: true, rules: [] }, archived: 0,
        branchNote: null, buildNote: null } }
      if (method === 'turn.send') {
        globalThis.modelTest.sent.push(params)
        return { ok: { turn: 1 } }
      }
      if (method === 'agent.info') return { ok: { build: 'fixture', version: '1', home: null } }
      return { ok: {} }
    })
    globalThis.finishModelTurn = () => BrowserWindow.getAllWindows()[0].webContents.send('bravebot:event', {
      event: 'turn.done', session: 's-new', data: { id: 'A', turn: 1, reply: 'Done', archived: 0 },
    })
  })
  await page.reload()
  await page.getByRole('button', { name: /^\+ New session$/ }).click()
  await page.getByRole('button', { name: "Don't trust", exact: true }).click()
  const trigger = page.locator('.model-trigger')
  await trigger.waitFor()
  assert.match(await trigger.getAttribute('aria-label'), /claude-haiku-4.5/)
  assert.equal(await page.locator('.model-current').innerText(), 'claude-haiku-4.5')
  const triggerBox = await trigger.boundingBox()
  const entryBox = await page.locator('.composer textarea').boundingBox()
  assert(triggerBox.y >= entryBox.y + entryBox.height, 'model picker is below the text entry');
  const sendBox = await page.locator('.composer .send').boundingBox();
  assert(Math.abs(triggerBox.y + triggerBox.height / 2 - sendBox.y - sendBox.height / 2) < 2,
    'model picker and Send align in the composer toolbar');
  assert.equal(await page.locator('.conversation-toolbar .export-open').count(), 1,
    'Export is in the conversation header');

  await trigger.click()
  await page.getByRole('option', { name: /Claude Sonnet/ }).waitFor()
  assert.deepEqual(await page.getByRole('option', { name: /Claude Sonnet/ }).locator('.model-capability').allTextContents(), ['Text', 'Tools'])
  assert.equal(await page.getByRole('option', { name: /Brave model/ }).locator('.model-capability').count(), 0)
  const search = page.getByRole('combobox', { name: 'Search models' })
  await search.fill('VISION')
  assert.equal(await page.getByRole('option').count(), 2)
  await search.fill('openrouter reasoning')
  assert.equal(await page.getByRole('option').count(), 1)
  assert.match(await page.getByRole('option').innerText(), /Sonnet/)
  await search.fill('brave tools')
  assert.equal(await page.getByRole('option').count(), 0)
  await search.fill('sonnet')
  assert.equal(await page.getByRole('option').count(), 1)
  await search.press('Enter')
  assert.equal(await page.locator('.model-popover').count(), 0)
  assert.equal(await trigger.evaluate((element) => element === document.activeElement), true)
  assert.match(await trigger.getAttribute('aria-label'), /Sonnet|sonnet/)
  assert.equal(await page.locator('.model-current').innerText(), 'Claude Sonnet 4.5')
  await page.screenshot({ path: '/tmp/bravebot-model-label.png' })

  const entry = page.locator('.composer textarea')
  await entry.fill('')
  await entry.press('Enter')
  assert.equal(await app.evaluate(() => globalThis.modelTest.sent.length), 0)
  await entry.fill('Use this conversation’s selected model')
  await entry.press('Shift+Enter')
  await entry.pressSequentially('On a second line')
  assert.equal(await entry.inputValue(), 'Use this conversation’s selected model\nOn a second line')
  assert.equal(await app.evaluate(() => globalThis.modelTest.sent.length), 0)
  await entry.press('Enter')
  assert.equal(await trigger.isDisabled(), true)
  const sent = await app.evaluate(() => globalThis.modelTest.sent)
  assert.equal(sent[0].model, 'openrouter/anthropic/claude-sonnet-4.5')
  assert.equal(sent[0].prompt, 'Use this conversation’s selected model\nOn a second line')
  await app.evaluate(() => globalThis.finishModelTurn())
  await page.waitForFunction(() => !document.querySelector('.model-trigger').disabled)
  await page.waitForFunction(() => localStorage.getItem('bravebot.conversation-model:["/tmp/bravebot-model-picker-project","A"]')?.includes('sonnet'))

  await page.locator('.session').filter({ hasText: 'Conversation B' }).click()
  await page.waitForFunction(() => document.querySelector('.model-trigger')?.getAttribute('aria-label')?.includes('haiku'))
  assert.match(await trigger.getAttribute('aria-label'), /haiku/)
  await page.locator('.session').filter({ hasText: /Conversation A|Use this conversation/ }).click()
  await page.waitForFunction(() => document.querySelector('.model-trigger')?.getAttribute('aria-label')?.includes('sonnet'))
  assert.match(await trigger.getAttribute('aria-label'), /sonnet/)
  await page.reload()
  await page.locator('.session').filter({ hasText: /Conversation A|Use this conversation/ }).click()
  await page.waitForFunction(() => document.querySelector('.model-trigger')?.getAttribute('aria-label')?.includes('sonnet'))
  assert.match(await trigger.getAttribute('aria-label'), /sonnet/)

  await trigger.click()
  await search.fill('no-such-model')
  await page.getByText('No models match your search.').waitFor()
  await search.press('Escape')
  assert.equal(await trigger.evaluate((element) => element === document.activeElement), true)
  await app.evaluate(() => { globalThis.modelTest.fail = true })
  await trigger.click()
  await page.getByRole('alert').waitFor()
  assert.match(await page.getByRole('alert').innerText(), /offline/)
  await app.evaluate(() => { globalThis.modelTest.fail = false })
  await search.fill('')
  await page.getByRole('button', { name: 'Refresh', exact: true }).click()
  await page.getByRole('option', { name: /Claude Haiku/ }).waitFor()
  await page.screenshot({ path: '/tmp/bravebot-model-picker.png' })
  await search.press('Escape')

  // Exercise real bot-definition IPC and disk persistence with deterministic inference.
  await app.evaluate(({ ipcMain }) => {
    ipcMain.removeHandler('bravebot:bots:send')
    ipcMain.handle('bravebot:bots:send', (_, params) => {
      globalThis.modelTest.botSent = params
      return { ok: { turn: 1 } }
    })
  })
  await page.locator('.sidebar-tab').nth(1).click()
  await page.getByRole('button', { name: 'New bot', exact: true }).click()
  const form = page.locator('.bot-form')
  await form.getByLabel('Name', { exact: true }).fill('Web dev model test')
  await form.getByLabel('Purpose', { exact: true }).fill('Build accessible websites.')
  await form.getByRole('button', { name: 'Choose a folder…' }).click()
  const preview = form.locator('[data-avatar]')
  const firstFace = await preview.getAttribute('data-avatar')
  await form.getByRole('button', { name: 'Refresh avatar', exact: true }).click()
  await page.waitForFunction((first) => document.querySelector('.bot-form [data-avatar]')?.getAttribute('data-avatar') !== first, firstFace)
  const chosenFace = await preview.getAttribute('data-avatar')
  const avatarBox = await preview.boundingBox()
  const nameBox = await form.getByLabel('Name', { exact: true }).boundingBox()
  assert(nameBox.x > avatarBox.x + avatarBox.width, 'name is beside the avatar')
  await form.locator('.model-trigger').click()
  await search.fill('no-such-model')
  await search.press('Enter')
  assert.equal(await form.count(), 1, 'search Enter with no matches must not create the bot')
  assert.equal((await page.evaluate(() => window.bravebot.readBots())).length, 0)
  await search.fill('sonnet')
  await search.press('Enter')
  await form.getByRole('button', { name: 'Create', exact: true }).click()
  const botRow = page.locator('.bot').filter({ hasText: 'Web dev model test' })
  await botRow.waitFor()
  const stored = await page.evaluate(async () => (await window.bravebot.readBots())[0])
  assert.equal(stored.model, 'openrouter/anthropic/claude-sonnet-4.5')
  assert.equal(await botRow.locator('[data-avatar]').getAttribute('data-avatar'), chosenFace)
  await botRow.click()
  await page.getByRole('button', { name: 'New conversation', exact: true }).click()
  await page.getByRole('button', { name: "Don't trust", exact: true }).click()
  await page.getByRole('dialog').waitFor({state:'hidden'})
  const botTrigger = page.locator('.composer .model-trigger')
  assert.match(await botTrigger.getAttribute('aria-label'), /sonnet/)
  await botTrigger.click()
  await search.fill('haiku')
  await search.press('Enter')
  await page.waitForFunction(async () => (await window.bravebot.readBots())[0]?.model?.includes('haiku'))
  await page.reload()
  await page.locator('.sidebar-tab').nth(1).click()
  await botRow.click()
  await page.getByRole('button', { name: 'New conversation', exact: true }).click()
  await page.getByRole('button', { name: "Don't trust", exact: true }).click()
  await page.getByRole('dialog').waitFor({state:'hidden'})
  assert.match(await botTrigger.getAttribute('aria-label'), /haiku/)
  await page.locator('.composer textarea').fill('Use the saved bot model')
  await page.locator('.composer .send').click()
  await page.waitForFunction(() => document.querySelector('.composer .model-trigger')?.disabled)
  const botSent = await app.evaluate(() => globalThis.modelTest.botSent)
  assert.equal(botSent.model, 'openrouter/anthropic/claude-haiku-4.5')
  assert.equal(botSent.slug, stored.slug)
  await page.screenshot({ path: '/tmp/bravebot-bot-model-persistence.png' })
  console.log('PASS: bot creation, avatar refresh/layout, saved model, composer model changes, reload persistence, and bot turn payload')
  assert.deepEqual(errors, [])
  console.log('PASS: default, picker placement, search, keyboard/focus, turn payload, running state, per-conversation persistence, empty/error/retry states')
} finally {
  await app.close()
  rmSync(profile, { recursive: true, force: true })
}
