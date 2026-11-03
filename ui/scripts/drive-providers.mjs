// Settings › Providers and what it does to the model picker: isolated profile, mocked bridge, no paid inference.
import assert from 'node:assert/strict'
import { mkdirSync, mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { _electron as electron } from 'playwright-core'

const MINIMUM = { width: 900, height: 560 }
const profile = mkdtempSync(join(tmpdir(), 'bravebot-providers-profile-'))
const directory = mkdtempSync(join(tmpdir(), 'bravebot-providers-project-'))
const output = join(tmpdir(), 'bravebot-providers')
mkdirSync(output, { recursive: true })
const app = await electron.launch({ args: ['.', ...(process.platform === 'linux' ? ['--ozone-platform=x11'] : []), `--user-data-dir=${profile}`], cwd: process.cwd(), timeout: 40000 })
try {
  const page = await app.firstWindow()
  await page.setViewportSize({ width: 1350, height: 900 })
  page.setDefaultTimeout(10000)
  await page.emulateMedia({ reducedMotion: 'reduce' })
  const errors = []
  page.on('pageerror', (error) => errors.push(error.message))
  await app.evaluate(({ ipcMain, dialog }, directory) => {
    const report = { build: 'bravebot fixture', configured: true, problem: null, model: 'bedrock-a', brave: false, bedrock: true,
      providers: [{ name: 'OpenRouter', key: 'gateway:openrouter', credential: 'missing' }, { name: 'OpenRouter', key: 'gateway:openrouter-eu', credential: 'missing' }, { name: 'Local', key: 'gateway:local', credential: 'not required' }],
      selected: null, layers: [], overrides: [], managed: { path: null, keys: [] },
      network: { roots: [], problem: null, trustsNothing: false, proxy: null, authenticated: false, unusableProxy: null, noProxy: null } }
    const accounts = [
      { way: 'brave', id: null, name: 'Brave Premium', standing: 'notSignedIn', keyFrom: null, detail: null, command: 'bravebot auth login brave', canSignOut: false, host: null, canEnterKey: true },
      { way: 'bedrock', id: 'work', name: 'AWS Bedrock (work)', standing: 'signedIn', keyFrom: null, detail: null, command: null, canSignOut: true, host: null, canEnterKey: false },
      { way: 'gateway', id: 'openrouter', name: 'OpenRouter', standing: 'notSignedIn', keyFrom: null, detail: null, command: null, canSignOut: false, host: 'openrouter.ai', canEnterKey: true },
      { way: 'xai', id: null, name: 'xAI', standing: 'signedIn', keyFrom: 'stored', detail: null, command: null, canSignOut: true, host: null, canEnterKey: false },
      { way: 'gateway', id: 'openrouter-eu', name: 'OpenRouter', standing: 'notSignedIn', keyFrom: null, detail: null, command: null, canSignOut: false, host: 'eu.openrouter.ai', canEnterKey: true },
    ]
    const model = (id, name, providerKey, provider) => ({ id, name, provider, providerKey, readsEffort: true, premium: false, contextWindow: 200000, capabilities: ['text', 'tools'] })
    // One listing failed, and its warning names the command that signs in again.
    const catalogue = { defaultModel: 'bedrock-a', warnings: ['Could not list the ChatGPT account\'s models. Sign in again.'], warningCommands: ['bravebot auth login chatgpt'], models: [
      model('bedrock-a', 'Bedrock A', 'bedrock', 'AWS Bedrock'),
      model('bedrock-b', 'Bedrock B', 'bedrock', 'AWS Bedrock'),
      model('bedrock-c', 'Bedrock C', 'bedrock', 'AWS Bedrock'),
      model('xai/grok-1', 'Grok one', 'xai', 'xAI'),
      model('xai/grok-2', 'Grok two', 'xai', 'xAI'),
    ] }
    const rows = [{ id: 'a', title: 'Providers fixture', directory, project: 'Fixture', branch: null, updated: 1, bytes: 1 }]
    globalThis.providersFixture = { signOuts: [], listings: 0 }
    dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [directory] })
    ipcMain.removeHandler('bravebot:request')
    ipcMain.handle('bravebot:request', (_, method, params = {}) => {
      if (method === 'agent.info') return { ok: { configured: true, build: report.build } }
      if (method === 'session.list') return { ok: { sessions: rows } }
      if (method === 'session.new') return { ok: { session: 's-new', directory, shownDirectory: directory, branch: null, model: 'bedrock-a' } }
      if (method === 'session.open') return { ok: { session: `s-${params.id}`, model: 'bedrock-a', record: { ...rows[0], started: 1, turns: 0, tokens: 0, build: 'fixture' }, said: [], todos: {}, context: '', trust: { known: true, rules: [] }, archived: 0 } }
      if (method === 'models.list') { globalThis.providersFixture.listings += 1; return { ok: catalogue } }
      if (method === 'effort.get') return { ok: { recorded: null, inForce: null, outranked: false } }
      if (method === 'settings.inspect') return { ok: report }
      if (method === 'accounts.list') return { ok: { accounts } }
      if (method === 'accounts.signOut') { globalThis.providersFixture.signOuts.push(params); return { ok: { accounts } } }
      return { ok: {} }
    })
  }, directory)
  const snap = async (name) => {
    await page.waitForTimeout(300)
    await page.screenshot({ path: join(output, `${name}.png`), scale: 'css' })
  }
  await page.reload()

  // Providers is the second page, and holds the model service and the accounts.
  await page.getByRole('button', { name: 'Settings', exact: true }).click()
  const settings = page.locator('[data-test="settings-view"]')
  assert.deepEqual(await settings.locator('[data-test^="settings-page-"]').evaluateAll((items) => items.map((item) => item.dataset.test)),
    ['settings-page-general', 'settings-page-providers', 'settings-page-connectors', 'settings-page-agent'])
  await settings.locator('[data-test="settings-page-providers"]').click()
  await settings.getByRole('heading', { name: 'Model service configured', exact: true, level: 2 }).waitFor()
  await settings.getByRole('heading', { name: 'Accounts', exact: true, level: 2 }).waitFor()
  await settings.getByText('Credentials missing', { exact: true }).first().waitFor()
  console.log('PASS: Providers is second in the list and holds the model service and the accounts')

  // The first signed-in account is chosen, and a signed-in account shows its models.
  const accounts = settings.locator('[data-test="accounts"]')
  const tab = (key) => accounts.locator(`[data-test="provider-${key}"]`)
  await accounts.locator('[data-test="account-bedrock:work"]').waitFor()
  assert.equal(await tab('bedrock:work').getAttribute('aria-selected'), 'true', 'the first signed-in account is chosen')
  const mark = await tab('brave:').locator('.providers-item-logo').boundingBox()
  assert.deepEqual([Math.round(mark.width), Math.round(mark.height)], [24, 24], 'an account\'s mark in the list is 24px')
  assert.equal(await tab('brave:').locator('.providers-item-logo').getAttribute('name'), 'social-brave-release-favicon-fullheight-color')
  const models = accounts.locator('[data-test="provider-model-list"]')
  const order = () => models.locator('li').evaluateAll((rows) => rows.map((row) => row.dataset.modelId))
  await models.waitFor()
  assert.deepEqual(await order(), ['bedrock-a', 'bedrock-b', 'bedrock-c'])
  await snap('01-bedrock')

  // A row not signed in shows its actions and no models; "Fix" on the status table chooses its row.
  // Two gateways share a display name, so Fix names the service and not the name.
  await settings.locator('[data-test="settings-provider-fix"]').nth(1).click()
  await accounts.locator('[data-test="account-gateway:openrouter-eu"]').waitFor()
  assert.equal(await tab('gateway:openrouter-eu').getAttribute('aria-selected'), 'true', 'Fix on the second gateway chooses the second gateway')
  await settings.locator('[data-test="settings-provider-fix"]').first().click()
  await accounts.locator('[data-test="account-gateway:openrouter"]').waitFor()
  assert.equal(await tab('gateway:openrouter').getAttribute('aria-selected'), 'true', 'Fix chooses the account it names')
  await accounts.locator('[data-test="account-gateway:openrouter"] [data-test="account-enter-key"]').waitFor()
  assert.equal(await accounts.locator('[data-test="provider-models"]').count(), 0, 'an account not signed in shows no models')
  // Arrow keys move through the list.
  await tab('gateway:openrouter').press('ArrowDown')
  await accounts.locator('[data-test="account-xai:"]').waitFor()
  assert.equal(await page.evaluate(() => document.activeElement?.dataset.test), 'provider-xai:', 'focus follows the chosen account')
  console.log('PASS: Fix and the arrow keys choose an account, and one not signed in shows its actions and no models')

  // Reading the rows again, here after signing out of the account chosen since, leaves that account
  // chosen: the Fix pressed earlier is not applied a second time.
  await accounts.locator('[data-test="account-xai:"] [data-test="account-sign-out"]').click()
  await page.locator('[data-test="sign-out-confirm"]').click()
  await accounts.locator('[data-test="accounts-said"]').waitFor()
  await page.waitForFunction(() => globalThis.document.querySelector('[data-test="sign-out-confirm"]') === null)
  await page.waitForTimeout(200)
  assert.equal(await tab('xai:').getAttribute('aria-selected'), 'true', 'reading the rows again keeps the account chosen after Fix')
  assert.equal(await tab('gateway:openrouter').getAttribute('aria-selected'), 'false', 'the row Fix chose is not chosen again')
  const signedOut = (await app.evaluate(() => globalThis.providersFixture.signOuts)).length
  console.log('PASS: reading the rows again after Fix keeps the account chosen since')

  // A listing that failed is said on the page, with the sign-in its warning names, and Try again
  // asks for the list again.
  const warnings = settings.locator('[data-test="providers-warnings"]')
  await warnings.locator('[data-test="model-warning"]').waitFor()
  assert.match(await warnings.locator('[data-test="model-warning"]').innerText(), /Could not list the ChatGPT account/)
  assert.equal(await warnings.locator('[data-test="model-warning"] [slot="actions"] leo-button').count(), 1, 'the warning offers its sign-in')
  const listedBefore = await app.evaluate(() => globalThis.providersFixture.listings)
  await warnings.locator('[data-test="providers-warnings-retry"]').click()
  await page.waitForFunction((before) => globalThis.document.querySelector('[data-test="providers-warnings"]') !== null && before >= 0, listedBefore)
  await page.waitForTimeout(300)
  assert.equal(await app.evaluate(() => globalThis.providersFixture.listings), listedBefore + 1, 'Try again asks for the models again')
  console.log('PASS: a failed listing is said on the Providers page with its sign-in, and Try again lists again')

  // Hide one model of Bedrock, move one, and hide xAI altogether.
  await tab('bedrock:work').click()
  await models.waitFor()
  const reset = accounts.locator('[data-test="provider-reset"]')
  assert.equal(await reset.evaluate((host) => host.shadowRoot?.querySelector('button')?.disabled), true, 'nothing to reset before a change')
  await accounts.locator('[data-test="provider-model-bedrock-b"] [data-test="provider-model-shown"]').click()
  await page.waitForFunction(() => document.querySelector('[data-test="provider-model-bedrock-b"] leo-toggle')?.shadowRoot?.querySelector('[role="switch"]')?.getAttribute('aria-checked') === 'false')
  // Dragging the last row's handle up by two rows puts it first.
  const handle = accounts.locator('[data-test="provider-model-bedrock-c"] .provider-model-handle')
  const box = await handle.boundingBox()
  const rowHeight = (await accounts.locator('[data-test="provider-model-bedrock-a"]').boundingBox()).height
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2)
  await page.mouse.down()
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2 - rowHeight, { steps: 4 })
  await snap('02-dragging')
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2 - 2 * rowHeight, { steps: 4 })
  await page.mouse.up()
  await page.waitForFunction(() => document.querySelector('[data-test="provider-model-list"] li')?.dataset.modelId === 'bedrock-c')
  assert.deepEqual(await order(), ['bedrock-c', 'bedrock-a', 'bedrock-b'], 'a drag moves the row to where it was dropped')
  assert.match(await accounts.locator('[data-test="provider-models-said"]').innerText(), /Bedrock C moved to position 1 of 3/)
  // Alt and an arrow on a handle moves its row by one.
  await accounts.locator('[data-test="provider-model-bedrock-a"] [data-test="provider-model-move"]').focus()
  await page.keyboard.press('Alt+ArrowDown')
  await page.waitForFunction(() => document.querySelectorAll('[data-test="provider-model-list"] li')[2]?.dataset.modelId === 'bedrock-a')
  assert.deepEqual(await order(), ['bedrock-c', 'bedrock-b', 'bedrock-a'], 'Alt with an arrow moves the row by one')
  await snap('03-changed')
  await tab('xai:').click()
  await accounts.locator('[data-test="provider-model-list"]').waitFor()
  await accounts.locator('[data-test="provider-shown"]').click()
  await page.waitForFunction(() => document.querySelector('[data-test="provider-shown"]')?.shadowRoot?.querySelector('[role="switch"]')?.getAttribute('aria-checked') === 'false')
  console.log('PASS: a model is hidden, rows move by drag and by keyboard, and a service is hidden')

  // The picker follows: xAI is gone from the rail, Bedrock B is gone, and Bedrock lists C then A.
  await page.locator('[data-test="settings-back"]').click()
  await page.locator('[data-test="new-project"]').click()
  const trust = page.getByRole('button', { name: "Don't trust", exact: true })
  if (await trust.isVisible().catch(() => false)) await trust.click()
  const trigger = page.locator('.model-trigger')
  await trigger.click()
  await page.locator('[data-test="model-rail"]').waitFor()
  await page.getByRole('option', { name: /Bedrock A/ }).waitFor()
  assert.deepEqual(await page.locator('[data-test="model-rail"] leo-button').evaluateAll((buttons) => buttons.map((button) => button.dataset.test)),
    ['model-rail-all', 'model-rail-bedrock'], 'a hidden service leaves the rail')
  assert.deepEqual(await page.locator('.model-menu').getByRole('option').evaluateAll((rows) => rows.map((row) => row.getAttribute('value'))),
    ['bedrock-c', 'bedrock-a'], 'a hidden model is left out and the rest are in the chosen order')
  await page.getByRole('combobox', { name: 'Search models' }).fill('grok')
  await page.locator('[data-test="model-empty"]').waitFor()
  assert.equal(await page.locator('.model-menu').getByRole('option').count(), 0, 'a search does not bring a hidden service back')
  await snap('04-picker')
  await page.getByRole('combobox', { name: 'Search models' }).press('Escape')
  console.log('PASS: the model picker leaves out what was hidden and lists the rest in the chosen order')

  // Reset to defaults puts Bedrock back as it was and leaves the account signed in.
  await page.getByRole('button', { name: 'Settings', exact: true }).click()
  await settings.locator('[data-test="settings-page-providers"]').click()
  await tab('bedrock:work').click()
  await models.waitFor()
  await reset.click()
  await page.waitForFunction(() => document.querySelector('[data-test="provider-model-list"] li')?.dataset.modelId === 'bedrock-a')
  assert.deepEqual(await order(), ['bedrock-a', 'bedrock-b', 'bedrock-c'], 'reset restores the order')
  assert.equal(await accounts.locator('[data-test="provider-model-bedrock-b"] leo-toggle').evaluate((host) => host.shadowRoot?.querySelector('[role="switch"]')?.getAttribute('aria-checked')), 'true', 'reset shows the hidden model again')
  assert.equal(await accounts.locator('[data-test="account-bedrock:work"] [data-test="account-sign-out"]').count(), 1, 'the account is still signed in')
  assert.equal((await app.evaluate(() => globalThis.providersFixture.signOuts)).length, signedOut, 'reset signs nothing out')
  await tab('xai:').click()
  await accounts.locator('[data-test="provider-models"].provider-hidden').waitFor()
  console.log('PASS: Reset to defaults restores one service and keeps its sign-in and the other service\'s choices')

  for (const [name, size] of [['05-normal', { width: 1350, height: 900 }], ['06-minimum', MINIMUM]]) {
    await page.setViewportSize(size)
    await tab('bedrock:work').click()
    await models.waitFor()
    await accounts.scrollIntoViewIfNeeded()
    await snap(name)
  }
  await page.emulateMedia({ colorScheme: 'dark', reducedMotion: 'reduce' })
  await page.setViewportSize({ width: 1350, height: 900 })
  await snap('07-dark')
  assert.deepEqual(errors, [])
  console.log(`PASS: screenshots in ${output}`)
} finally {
  await app.close()
}
