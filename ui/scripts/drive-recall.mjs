// RETRIEVE-24 and RETRIEVE-25 in the window: the Recall earlier messages switch, its dialog, its
// caption and its locked state, against a stand-in for the agent's `settings.inspect` and
// `retrieval.set`. The agent's own rules have Rust tests; here the page's side is driven.
import assert from 'node:assert/strict'
import { mkdtempSync, mkdirSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { _electron as electron } from 'playwright-core'
const profile = mkdtempSync(join(tmpdir(), 'bravebot-recall-profile-'))
const output = process.env.UX_OUTPUT || join(tmpdir(), 'bravebot-recall')
mkdirSync(output, { recursive: true })
const app = await electron.launch({ args: ['.', ...(process.platform === 'linux' ? ['--ozone-platform=x11'] : []), `--user-data-dir=${profile}`], cwd: process.cwd(), timeout: 40000 })
let page
try {
  page = await app.firstWindow()
  await page.setViewportSize({ width: 640, height: 900 })
  page.setDefaultTimeout(10000)
  await page.emulateMedia({ reducedMotion: 'reduce' })
  const errors = []
  page.on('pageerror', e => errors.push(e.message))
  await app.evaluate(({ ipcMain }) => {
    const url = 'https://ai-chat-backend-dev.bsg.brave.software/v1/embeddings'
    const retrieval = { enabled: false, path: '/Users/example/.bravebot/settings.json', from: null, locked: null, disclosed: false, endpoint: { url, host: 'ai-chat-backend-dev.bsg.brave.software', model: 'embedding_gemma', dimensions: 768, development: true } }
    const report = { build: 'bravebot fixture', configured: true, problem: null, model: 'local/test', brave: false, bedrock: false, providers: [], selected: null, layers: [], overrides: [], managed: { path: null, keys: [] }, network: { roots: [], problem: null, trustsNothing: false, proxy: null, authenticated: false, unusableProxy: null, noProxy: null }, retrieval }
    const state = { report, sets: [] }
    globalThis.recallFixture = state
    const replace = (name, fn) => { ipcMain.removeHandler(name); ipcMain.handle(name, fn) }
    replace('bravebot:request', (_, method, params = {}) => {
      if (method === 'agent.info') return { ok: { configured: true, build: report.build } }
      if (method === 'session.list') return { ok: { sessions: [] } }
      if (method === 'models.list') return { ok: { models: [], defaultModel: null, warnings: [] } }
      if (method === 'settings.inspect') return { ok: state.report }
      if (method === 'hooks.inspect') return { ok: { path: '/test/.bravebot/hooks.json', text: '{"hooks":[]}', entire: true, timeoutSeconds: { min: 1, max: 600, defaultFor: {} }, hooks: [] } }
      if (method === 'retrieval.set') {
        state.sets.push(params)
        const now = state.report.retrieval
        if (now.locked) return { error: { code: 'config', message: `retrieval is set by ${now.locked.path} for this run` } }
        if (params.enabled && !now.disclosed && params.disclosed !== url) return { error: { code: 'bad_request', message: 'not shown' } }
        state.report.retrieval = { ...now, enabled: params.enabled, from: now.path, disclosed: now.disclosed || params.enabled }
        return { ok: state.report.retrieval }
      }
      return { ok: {} }
    })
  })
  const fixture = () => app.evaluate(() => globalThis.recallFixture)
  const snap = async name => { await page.waitForTimeout(500); await page.screenshot({ path: join(output, name + '.png'), scale: 'css' }) }
  const checked = () => page.locator('[data-test="recall-switch"]').evaluate(el => el.shadowRoot?.querySelector('[role="switch"]')?.getAttribute('aria-checked'))
  const open = async () => {
    await page.reload()
    await page.getByRole('button', { name: 'Settings', exact: true }).click()
    const view = page.locator('[data-test="settings-view"]')
    await view.waitFor()
    await view.locator('[data-test="settings-page-agent"]').click()
    await view.getByRole('heading', { name: 'Bots', exact: true, level: 2 }).waitFor()
    // Bots is the first group, above Connection.
    assert.deepEqual((await view.getByRole('heading', { level: 2 }).allTextContents()).slice(0, 2), ['Bots', 'Connection'])
  }

  await open()
  const caption = page.locator('[data-test="recall-caption"]')
  for (const words of ['Brave’s embeddings service (development)', 'ai-chat-backend-dev.bsg.brave.software', 'untrusted content, images and PDFs, or anything in incognito', '/Users/example/.bravebot/settings.json']) {
    assert.ok((await caption.textContent()).includes(words), words)
  }
  assert.equal(await checked(), 'false')
  await snap('01-off')

  // Turning it on asks first, and Cancel leaves it off with nothing sent.
  await page.locator('[data-test="recall-switch"]').click()
  const dialog = page.locator('.recall-disclosure')
  await dialog.getByText('Turn on recall', { exact: true }).waitFor()
  await snap('02-dialog')
  await page.locator('[data-test="recall-cancel"]').click()
  await dialog.waitFor({ state: 'detached' })
  assert.equal(await checked(), 'false')
  assert.deepEqual((await fixture()).sets, [])

  // Turn on recall sends the endpoint the dialog showed, and the switch reads back on.
  await page.locator('[data-test="recall-switch"]').click()
  await page.locator('[data-test="recall-confirm"]').click()
  await page.waitForFunction(() => document.querySelector('[data-test="recall-switch"]')?.shadowRoot?.querySelector('[role="switch"]')?.getAttribute('aria-checked') === 'true')
  assert.deepEqual((await fixture()).sets, [{ enabled: true, disclosed: 'https://ai-chat-backend-dev.bsg.brave.software/v1/embeddings' }])
  await snap('03-on')

  // Once shown, off and on again ask nothing.
  await page.locator('[data-test="recall-switch"]').click()
  await page.waitForFunction(() => document.querySelector('[data-test="recall-switch"]')?.shadowRoot?.querySelector('[role="switch"]')?.getAttribute('aria-checked') === 'false')
  await page.locator('[data-test="recall-switch"]').click()
  await page.waitForFunction(() => document.querySelector('[data-test="recall-switch"]')?.shadowRoot?.querySelector('[role="switch"]')?.getAttribute('aria-checked') === 'true')
  assert.equal(await dialog.count(), 0)
  assert.deepEqual((await fixture()).sets.slice(1), [{ enabled: false }, { enabled: true }])

  // An administrator's file locks it: the row says so and the switch cannot be moved.
  await app.evaluate(() => { const r = globalThis.recallFixture.report; r.retrieval = { ...r.retrieval, enabled: false, locked: { by: 'managed', path: '/Library/Application Support/Bravebot/managed-settings.json' } } })
  await open()
  await page.locator('[data-test="recall-locked"]').getByText('Set by your administrator', { exact: true }).waitFor()
  const before = (await fixture()).sets.length
  await page.locator('[data-test="recall-switch"]').click({ force: true })
  await page.waitForTimeout(300)
  assert.equal(await checked(), 'false')
  assert.equal((await fixture()).sets.length, before, 'a locked switch sent a request')
  await snap('04-locked-managed')

  await app.evaluate(() => { const r = globalThis.recallFixture.report; r.retrieval = { ...r.retrieval, enabled: true, locked: { by: 'run', path: '/Users/example/work/team-settings.json' } } })
  await open()
  await page.locator('[data-test="recall-locked"]').getByText('Set by /Users/example/work/team-settings.json for this run', { exact: true }).waitFor()
  await snap('05-locked-run')

  assert.deepEqual(errors, [])
  console.log(`recall: ok, screenshots in ${output}`)
} finally {
  await app.close()
}
