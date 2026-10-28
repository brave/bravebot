// Exercise the real composer with deterministic model/session replies; no paid inference.
import assert from 'node:assert/strict'
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { _electron as electron } from 'playwright-core'
import { COMPOSER_INPUT, BOT_OPEN_BUTTON, COMPOSER_SEND } from './drive-support.mjs'

// What `models.list` says of a service whose models could not be listed: a sentence, and the command
// it names beside it, which the picker draws as code (CHATGPT-9).
const WARNING_COMMAND = 'bravebot auth login chatgpt'
const WARNING = `Could not load models from ChatGPT, so try again, or run ${WARNING_COMMAND} in a terminal to sign in again.`
const MINIMUM = { width: 900, height: 560 }
// How long `modelTest.loading` holds `models.list` back, in milliseconds.
const LISTING_DELAY = 1500

const profile = mkdtempSync(join(tmpdir(), 'bravebot-model-picker-'))
const app = await electron.launch({ args: ['.', `--user-data-dir=${profile}`], cwd: process.cwd(), timeout: 40000 })
try {
  const page = await app.firstWindow()
  page.setDefaultTimeout(15000)
  const errors = []
  page.on('pageerror', (error) => errors.push(error.message))
  mkdirSync('/tmp/bravebot-model-picker-project', { recursive: true })
  await app.evaluate(({ ipcMain, BrowserWindow, dialog }, { warning, delay }) => {
    const defaultModel = 'openrouter/anthropic/claude-haiku-4.5'
    const directory = '/tmp/bravebot-model-picker-project'
    const rows = ['A', 'B'].map((id) => ({ id, directory, title: `Conversation ${id}`,
      project: 'model-picker-project', branch: null, updated: 1, bytes: 1 }))
    globalThis.modelTest = { sent: [], fail: false, loading: false }
    // The native picker opens a project; it is answered here rather than through the channel.
    dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [directory] })
    ipcMain.removeHandler('bravebot:request')
    ipcMain.handle('bravebot:request', async (_, method, params) => {
      if (method === 'models.list') {
        if (globalThis.modelTest.fail) return { error: { code: 'offline', message: 'Model listing is offline.' } }
        if (globalThis.modelTest.loading) await new Promise((resolve) => setTimeout(resolve, delay))
        return { ok: { defaultModel, warnings: [warning.sentence], warningCommands: [warning.command], models: [
          { id: defaultModel, name: 'Claude Haiku 4.5', provider: 'OpenRouter', premium: false, contextWindow: 200000, capabilities: ['text', 'vision', 'tools'] },
          { id: 'openrouter/anthropic/claude-sonnet-4.5', name: 'Claude Sonnet 4.5', provider: 'OpenRouter', premium: false, contextWindow: 200000, capabilities: ['text', 'vision', 'tools', 'reasoning'] },
          { id: 'brave-model', name: 'Brave model', provider: 'Brave', premium: false, contextWindow: 24000 },
        ] } }
      }
      if (method === 'session.list') return { ok: { sessions: rows } }
      if (method === 'session.new') return { ok: { session: 's-new', directory, shownDirectory: directory, branch: null, model: defaultModel } }
      if (method === 'session.open') return { ok: { session: `s-${params.id}`, model: defaultModel,
        record: { ...rows.find((row) => row.id === params.id), started: 1, turns: 0, tokens: 0, build: 'fixture' },
        said: [], todos: {}, context: '', trust: { known: true, rules: [] }, archived: 0,
        branchNote: null, buildNote: null, frontNote: null } }
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
  }, { warning: { sentence: WARNING, command: WARNING_COMMAND }, delay: LISTING_DELAY })
  await page.reload()
  await page.locator('[data-test="new-project"]').click()
  await page.getByRole('button', { name: "Don't trust", exact: true }).click()
  const trigger = page.locator('.model-trigger')
  await trigger.waitFor()
  assert.match(await trigger.getAttribute('aria-label'), /claude-haiku-4.5/)
  assert.equal(await page.locator('.model-current').innerText(), 'claude-haiku-4.5')
  const triggerBox = await trigger.boundingBox()
  const entryBox = await page.locator(COMPOSER_INPUT).boundingBox()
  assert(triggerBox.y >= entryBox.y + entryBox.height, 'model picker is below the text entry');
  const sendBox = await page.locator(COMPOSER_SEND).boundingBox();
  assert(Math.abs(triggerBox.y + triggerBox.height / 2 - sendBox.y - sendBox.height / 2) < 2,
    'model picker and Send align in the composer toolbar');
  assert.equal(await page.locator('.conversation-toolbar .export-open').count(), 1,
    'Export is in the conversation header');

  // The first open, with the listing held back: the menu is drawn and its search field is focused
  // before the answer, the loading line is drawn, and the only row is the model already in force.
  // What is typed meanwhile narrows the rows when they arrive, and Enter chooses nothing until then.
  // This runs before any other open because the picker keeps the catalogue of an earlier one.
  await app.evaluate(() => { globalThis.modelTest.loading = true })
  const opened = Date.now()
  await trigger.click()
  await page.locator('.model-popover').waitFor({ state: 'visible' })
  assert(Date.now() - opened < LISTING_DELAY, 'the menu is drawn before the listing arrives')
  await page.locator('.model-notices p.model-status:not([data-delayed])').filter({ hasText: 'Loading available models' }).waitFor()
  assert(Date.now() - opened < LISTING_DELAY, 'the loading line is drawn before the listing arrives')
  assert.equal(await page.evaluate(() => document.activeElement?.classList.contains('model-search')), true,
    'the search field is focused before the listing arrives')
  assert.deepEqual(await page.locator('.model-menu').getByRole('option').evaluateAll((rows) => rows.map((row) => row.getAttribute('value'))),
    ['openrouter/anthropic/claude-haiku-4.5'], 'only the model in force is a row before the listing arrives')
  await page.getByRole('combobox', { name: 'Search models' }).fill('sonnet')
  assert.equal(await page.locator('.model-menu').getByRole('option').count(), 0, 'typing narrows the rows still to come')
  await page.getByRole('combobox', { name: 'Search models' }).press('Enter')
  assert.equal(await page.locator('.model-popover').isVisible(), true, 'Enter before the listing arrives leaves the menu open')
  assert.equal(await app.evaluate(() => globalThis.modelTest.sent.length), 0)
  assert.match(await trigger.getAttribute('aria-label'), /haiku/, 'Enter before the listing arrives chooses nothing')
  await page.getByRole('option', { name: /Claude Sonnet/ }).waitFor()
  assert(Date.now() - opened >= LISTING_DELAY, 'the rows arrive with the listing')
  assert.equal(await page.locator('.model-menu').getByRole('option').count(), 1, 'the typed words narrow the rows that arrive')
  assert.equal(await page.locator('.model-notices p.model-status', { hasText: 'Loading available models' }).count(), 0,
    'the loading line goes when the rows arrive')
  await page.getByRole('combobox', { name: 'Search models' }).press('Escape')
  await page.locator('.model-popover').waitFor({ state: 'hidden' })
  await app.evaluate(() => { globalThis.modelTest.loading = false })
  console.log('PASS: the picker opens and shows its loading line before the listing arrives, and the typed words narrow the rows that arrive')

  // A warning the model list carries is drawn in the picker with the command it names as code, at the
  // window's size and at its minimum, in the code type role and inside the menu.
  const normal = await page.evaluate(() => ({ width: window.innerWidth, height: window.innerHeight }))
  for (const [where, size] of [['the window size', normal], ['the minimum size', MINIMUM]]) {
    await page.setViewportSize(size)
    assert((await page.evaluate(() => window.innerWidth)) <= size.width, `the window is at ${where}`)
    await trigger.click()
    const warning = page.locator('.model-notices p.model-status').filter({ hasText: 'Could not load models from ChatGPT' })
    await warning.waitFor()
    assert.equal(await warning.innerText(), WARNING, `${where}: the warning is the sentence the bridge sent`)
    assert.equal(await warning.locator('code').count(), 1, `${where}: the command is drawn as code`)
    const code = warning.locator('code')
    assert.equal(await code.innerText(), WARNING_COMMAND, `${where}: the code is the command`)
    assert.equal(await code.evaluate((element) => {
      const probe = document.createElement('span')
      probe.style.font = 'var(--type-code)'
      document.body.append(probe)
      const role = getComputedStyle(probe).font
      probe.remove()
      return getComputedStyle(element).font === role
    }), true, `${where}: the command is in the code type role`)
    // The menu fades in, and its place is read once it has finished.
    await page.evaluate(() => Promise.all(document.getAnimations().map((animation) => animation.finished)))
    const box = await code.boundingBox()
    const popover = await page.locator('.model-popover').boundingBox()
    assert(box && popover && box.x >= popover.x - 1 && box.x + box.width <= popover.x + popover.width + 1, `${where}: the command stays inside the menu`)
    assert.equal(await code.evaluate((element) => {
      const at = element.getBoundingClientRect()
      return element.contains(document.elementFromPoint(at.x + at.width / 2, at.y + at.height / 2))
    }), true, `${where}: nothing is drawn over the command`)
    await page.screenshot({ path: `/tmp/bravebot-model-warning-${where === 'the minimum size' ? 'minimum' : 'normal'}.png` })
    await page.getByRole('combobox', { name: 'Search models' }).press('Escape')
    await page.locator('.model-popover').waitFor({ state: 'hidden' })
  }
  await page.setViewportSize(normal)
  console.log('PASS: a picker warning draws its command as code, in the code type role and inside the menu, at the window size and at the minimum')

  await trigger.click()
  const sonnet = page.getByRole('option', { name: /Claude Sonnet/ })
  await sonnet.waitFor()
  await page.locator('leo-menu-section').hover()
  await sonnet.hover()
  assert.equal(await page.locator('.model-popover').isVisible(), true, 'hovering a model keeps the menu open')
  assert.equal(await sonnet.isVisible(), true)
  assert.deepEqual(await sonnet.locator('.model-capability').evaluateAll((badges) => badges.map((badge) => badge.getAttribute('aria-label'))), ['Text', 'Tools'])
  assert.equal(await page.getByRole('option', { name: /Brave model/ }).locator('.model-capability').count(), 0)
  const search = page.getByRole('combobox', { name: 'Search models' })
  const models = page.locator('.model-menu')
  await search.fill('VISION')
  assert.equal(await models.getByRole('option').count(), 2)
  await search.fill('openrouter reasoning')
  assert.equal(await models.getByRole('option').count(), 1)
  assert.match(await models.getByRole('option').innerText(), /Sonnet/)
  await search.fill('brave tools')
  assert.equal(await models.getByRole('option').count(), 0)
  await search.fill('sonnet')
  assert.equal(await models.getByRole('option').count(), 1)
  await search.press('Enter')
  assert.equal(await page.locator('.model-popover').isVisible(), false)
  assert.equal(await trigger.evaluate((element) => element === document.activeElement), true)
  assert.match(await trigger.getAttribute('aria-label'), /Sonnet|sonnet/)
  assert.equal(await page.locator('.model-current').innerText(), 'Claude Sonnet 4.5')
  await page.screenshot({ path: '/tmp/bravebot-model-label.png' })

  const entry = page.locator(COMPOSER_INPUT)
  await entry.fill('')
  await entry.press('Enter')
  assert.equal(await app.evaluate(() => globalThis.modelTest.sent.length), 0)
  await entry.fill('Use this conversation’s selected model')
  await entry.press('Shift+Enter')
  await entry.pressSequentially('On a second line')
  assert.equal(await entry.inputValue(), 'Use this conversation’s selected model\nOn a second line')
  assert.equal(await app.evaluate(() => globalThis.modelTest.sent.length), 0)
  await entry.press('Enter')
  assert.equal(await trigger.evaluate((host) => host.shadowRoot?.querySelector('button')?.disabled), true)
  const sent = await app.evaluate(() => globalThis.modelTest.sent)
  assert.equal(sent[0].model, 'openrouter/anthropic/claude-sonnet-4.5')
  assert.equal(sent[0].prompt, 'Use this conversation’s selected model\nOn a second line')
  await app.evaluate(() => globalThis.finishModelTurn())
  await page.waitForFunction(() => !document.querySelector('.model-trigger')?.shadowRoot?.querySelector('button')?.disabled)
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
  await page.locator('[data-test="sidebar-tabs"]').getByText('Bots', { exact: true }).click()
  await page.getByRole('button', { name: 'New bot', exact: true }).click()
  const form = page.locator('.bot-form')
  await form.getByRole('textbox', { name: 'Name', exact: true }).fill('Web dev model test')
  await form.getByRole('textbox', { name: 'Purpose', exact: true }).fill('Build accessible websites.')
  const preview = form.locator('.bot-form-avatar > .bot-avatar-frame [data-avatar]')
  const firstFace = await preview.getAttribute('data-avatar')
  const editor = form.locator('[data-test="avatar-editor"]')
  const openEditor = form.getByRole('button', { name: 'Edit avatar', exact: true })
  await openEditor.click()
  await editor.waitFor()
  assert.equal(await editor.locator('[data-body]').count(), 8, 'the editor offers the eight shapes')
  assert.equal(await editor.locator('[data-colour]').count(), 8, 'the editor offers the eight colours')
  await editor.evaluate((element) => Promise.all(element.getAnimations().map((animation) => animation.finished)))
  await page.screenshot({ path: '/tmp/bravebot-avatar-editor-create.png' })
  // Choosing the pair the editor opened on turns a generated face into the one the thumbnail shows.
  await editor.locator('[data-body][aria-pressed="true"]').click()
  assert.equal(await preview.getAttribute('data-avatar'),
    await editor.locator('[data-body][aria-pressed="true"] [data-avatar]').getAttribute('data-avatar'), 'the face is the thumbnail that was chosen')
  // A colour recolours every shape on offer and the face as soon as it is chosen.
  const other = editor.locator('[data-colour][aria-pressed="false"]').first()
  const otherColour = await other.getAttribute('data-colour')
  await other.click()
  const shapeFaces = await editor.locator('[data-body] [data-avatar]').evaluateAll((faces) => faces.map((face) => face.getAttribute('data-avatar')))
  assert.equal(shapeFaces.length, 8)
  assert.ok(shapeFaces.every((face) => face.startsWith(`pixel-${otherColour}-`)), `shapes are drawn in ${otherColour}: ${shapeFaces}`)
  assert.ok((await preview.getAttribute('data-avatar')).startsWith(`pixel-${otherColour}-`))
  // Escape closes the editor, keeps the choice and leaves the create dialog open.
  const otherFace = await preview.getAttribute('data-avatar')
  await page.keyboard.press('Escape')
  await editor.waitFor({ state: 'hidden' })
  assert.equal(await preview.getAttribute('data-avatar'), otherFace, 'Escape keeps what was chosen')
  assert.notEqual(otherFace, firstFace)
  assert.equal(await form.count(), 1, 'Escape in the editor does not close the create dialog')
  // Escape closes it from outside the panel as well, and the dialog stays.
  await openEditor.click()
  await editor.waitFor()
  await form.getByRole('textbox', { name: 'Name', exact: true }).focus()
  await page.keyboard.press('Escape')
  await editor.waitFor({ state: 'hidden' })
  assert.equal(await form.count(), 1, 'Escape with focus outside the editor does not close the create dialog')
  await openEditor.click()
  await editor.waitFor()
  await editor.getByRole('button', { name: 'Randomize', exact: true }).click()
  await page.waitForFunction((first) => document.querySelector('.bot-form-avatar > .bot-avatar-frame [data-avatar]')?.getAttribute('data-avatar') !== first, otherFace)
  await editor.getByRole('button', { name: 'Diamond', exact: true }).click()
  await editor.getByRole('button', { name: 'Cyan to indigo', exact: true }).click()
  assert.equal(await editor.getByRole('button', { name: 'Diamond', exact: true }).getAttribute('aria-pressed'), 'true')
  // A press on the backdrop closes the editor, and the create dialog stays.
  await page.mouse.click(4, 4)
  await editor.waitFor({ state: 'hidden' })
  assert.equal(await form.count(), 1, 'closing the editor does not close the create dialog')
  const chosenFace = await preview.getAttribute('data-avatar')
  assert.match(chosenFace, /^pixel-cyan-indigo-.*-diamond-/)
  const avatarBox = await preview.boundingBox()
  const nameBox = await form.getByRole('textbox', { name: 'Name', exact: true }).boundingBox()
  assert(nameBox.x > avatarBox.x + avatarBox.width, 'name is beside the avatar')
  // The form asks for no model; a new bot has none until one is picked on its page.
  assert.equal(await form.locator('.model-trigger').count(), 0, 'the creation form has no model picker')
  await form.getByRole('button', { name: 'Create', exact: true }).click()
  const botRow = page.locator('.bot').filter({ hasText: 'Web dev model test' })
  await botRow.waitFor()
  const stored = await page.evaluate(async () => (await window.bravebot.readBots())[0])
  assert.equal(stored.model, null)
  assert.equal(stored.avatar, 'v4:diamond/cyan-indigo')
  assert.equal(await botRow.locator('[data-avatar]').getAttribute('data-avatar'), chosenFace)
  // The bot's own page has a composer that starts its next conversation, and picks the bot's model.
  await botRow.locator(BOT_OPEN_BUTTON).click()
  await page.locator('[data-test="bot-page"]').waitFor()
  // The details panel edits the face with the same editor, drawn over the panel rather than cut off by it.
  // The window is back at its own size, where the details are a column rather than an overlay.
  await page.setViewportSize(normal)
  const contextToggle = page.getByRole('button', { name: 'Context panel', exact: true })
  if (await contextToggle.getAttribute('aria-expanded') !== 'true') await contextToggle.click()
  const details = page.locator('[data-test="bot-details"]')
  const detailsEditor = details.locator('[data-test="avatar-editor"]')
  await details.getByRole('button', { name: 'Edit avatar', exact: true }).click()
  await detailsEditor.waitFor()
  const editorBox = await detailsEditor.boundingBox()
  const viewport = await page.evaluate(() => ({ width: innerWidth, height: innerHeight }))
  assert.ok(editorBox.x >= 0 && editorBox.x + editorBox.width <= viewport.width && editorBox.y + editorBox.height <= viewport.height, 'the editor is inside the window')
  assert.equal(await detailsEditor.locator('[data-body="diamond"]').getAttribute('aria-pressed'), 'true', 'the editor opens on the stored shape')
  assert.equal(await detailsEditor.locator('[data-colour="cyan-indigo"]').getAttribute('aria-pressed'), 'true', 'the editor opens on the stored colour')
  await detailsEditor.evaluate((element) => Promise.all(element.getAnimations().map((animation) => animation.finished)))
  await page.screenshot({ path: '/tmp/bravebot-avatar-editor-details.png' })
  // Each choice is saved as it is made, with the editor still open.
  await detailsEditor.getByRole('button', { name: 'Block', exact: true }).click()
  await page.waitForFunction(async () => (await window.bravebot.readBots())[0]?.avatar === 'v4:block/cyan-indigo')
  await detailsEditor.getByRole('button', { name: 'Chartreuse to teal', exact: true }).click()
  await page.waitForFunction(async () => (await window.bravebot.readBots())[0]?.avatar === 'v4:block/chartreuse-teal')
  assert.equal(await detailsEditor.isVisible(), true, 'the editor stays open while choosing')
  await detailsEditor.getByRole('button', { name: 'Randomize', exact: true }).click()
  await page.waitForFunction(async () => ![undefined, 'v4:block/chartreuse-teal'].includes((await window.bravebot.readBots())[0]?.avatar))
  // Escape and a click outside only close it.
  const randomised = (await page.evaluate(() => window.bravebot.readBots()))[0].avatar
  await page.keyboard.press('Escape')
  await detailsEditor.waitFor({ state: 'hidden' })
  assert.equal((await page.evaluate(() => window.bravebot.readBots()))[0].avatar, randomised, 'Escape keeps what was saved')
  await details.getByRole('button', { name: 'Edit avatar', exact: true }).click()
  await detailsEditor.waitFor()
  await detailsEditor.getByRole('button', { name: 'Block', exact: true }).click()
  await detailsEditor.getByRole('button', { name: 'Chartreuse to teal', exact: true }).click()
  await page.waitForFunction(async () => (await window.bravebot.readBots())[0]?.avatar === 'v4:block/chartreuse-teal')
  await page.locator('[data-test="bot-page"]').click({ position: { x: 20, y: 200 } })
  await detailsEditor.waitFor({ state: 'hidden' })
  await botRow.locator('[data-avatar^="pixel-chartreuse-teal-"][data-avatar*="-block-"]').waitFor()
  const botTrigger = page.locator('.composer .model-trigger')
  await botTrigger.click()
  await search.fill('no-such-model')
  await search.press('Enter')
  assert.equal((await page.evaluate(() => window.bravebot.readBots()))[0].model, null, 'search Enter with no matches must not pick a model')
  await search.fill('sonnet')
  await search.press('Enter')
  await page.waitForFunction(async () => (await window.bravebot.readBots())[0]?.model === 'openrouter/anthropic/claude-sonnet-4.5')
  await page.waitForFunction(() => /sonnet/.test(document.querySelector('.composer .model-trigger')?.getAttribute('aria-label') ?? ''))
  await botTrigger.click()
  await search.fill('haiku')
  await search.press('Enter')
  await page.waitForFunction(async () => (await window.bravebot.readBots())[0]?.model?.includes('haiku'))
  await page.reload()
  await page.locator('[data-test="sidebar-tabs"]').getByText('Bots', { exact: true }).click()
  await botRow.locator(BOT_OPEN_BUTTON).click()
  await page.locator('[data-test="bot-page"]').waitFor()
  await page.waitForFunction(() => /haiku/.test(document.querySelector('.composer .model-trigger')?.getAttribute('aria-label') ?? ''))
  await page.locator(COMPOSER_INPUT).fill('Use the saved bot model')
  await page.locator(COMPOSER_SEND).click()
  await page.getByRole('button', { name: "Don't trust", exact: true }).click()
  await page.getByRole('dialog').waitFor({state:'hidden'})
  await page.waitForFunction(() => document.querySelector('.composer .model-trigger')?.shadowRoot?.querySelector('button')?.disabled)
  const botSent = await app.evaluate(() => globalThis.modelTest.botSent)
  assert.equal(botSent.model, 'openrouter/anthropic/claude-haiku-4.5')
  assert.equal(botSent.slug, stored.slug)
  await page.screenshot({ path: '/tmp/bravebot-bot-model-persistence.png' })
  console.log('PASS: bot creation, avatar editor on create and in details, saved model, composer model changes, reload persistence, and bot turn payload')
  assert.deepEqual(errors, [])
  console.log('PASS: default, picker placement, search, keyboard/focus, turn payload, running state, per-conversation persistence, empty/error/retry states')
} finally {
  await app.close()
  rmSync(profile, { recursive: true, force: true })
  rmSync('/tmp/bravebot-model-picker-project', { recursive: true, force: true })
}
