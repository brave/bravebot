// RETRIEVE-35 to RETRIEVE-37 in the window: choosing the embeddings service under the recall switch,
// OpenRouter's models, the test call's outcome, the custom endpoint and the dialog for a new host,
// against a stand-in for the agent's `settings.inspect`, `retrieval.set` and `retrieval.embeddings.*`.
// The agent's own rules have Rust tests; here the page's side is driven.
import assert from 'node:assert/strict'
import { mkdtempSync, mkdirSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { _electron as electron } from 'playwright-core'
const profile = mkdtempSync(join(tmpdir(), 'bravebot-embeddings-profile-'))
const output = process.env.UX_OUTPUT || join(tmpdir(), 'bravebot-embeddings')
mkdirSync(output, { recursive: true })
const app = await electron.launch({ args: ['.', ...(process.platform === 'linux' ? ['--ozone-platform=x11'] : []), `--user-data-dir=${profile}`], cwd: process.cwd(), timeout: 40000 })
let page
try {
  page = await app.firstWindow()
  await page.setViewportSize({ width: 640, height: 1000 })
  page.setDefaultTimeout(10000)
  await page.emulateMedia({ reducedMotion: 'reduce' })
  const errors = []
  page.on('pageerror', e => errors.push(e.message))
  await app.evaluate(({ ipcMain }) => {
    const NAMES = { openai: 'OpenAI', brave: 'Brave', openrouter: 'OpenRouter', custom: 'the custom endpoint' }
    const MODELS = [
      { id: 'openai/text-embedding-3-small', name: 'OpenAI: Text Embedding 3 Small', contextLength: 8192, pricing: { prompt: '0.00000002' } },
      { id: 'openai/text-embedding-3-large', name: 'OpenAI: Text Embedding 3 Large', contextLength: 8192, pricing: { prompt: '0.00000013' } },
      { id: 'qwen/qwen3-embedding-8b', name: 'Qwen: Qwen3 Embedding 8B', contextLength: 32000, pricing: { prompt: '0.00000001' } },
      { id: 'mistralai/mistral-embed-2312', name: 'Mistral: Mistral Embed 2312', contextLength: 8192, pricing: { prompt: '0.0000001' } },
      { id: 'baai/bge-m3', name: 'BAAI: bge-m3', contextLength: 8192, pricing: { prompt: '0' } },
    ]
    const DIMS = { 'openai/text-embedding-3-small': 1536, 'openai/text-embedding-3-large': 3072, 'qwen/qwen3-embedding-8b': 4096, 'mistralai/mistral-embed-2312': 1024, 'baai/bge-m3': 1024 }
    const fx = {
      enabled: false, disclosed: null, selected: null, locked: null,
      available: { openai: true, brave: false, openrouter: true, custom: true },
      kept: {
        openai: { model: null }, brave: {}, openrouter: { model: null, dimensions: null },
        custom: { url: 'https://embed.example.com/v1/embeddings', model: 'nomic-embed-text', dimensions: 768, queryPrefix: 'search_query: ', passagePrefix: 'search_document: ', keyStored: true },
      },
      modelsMode: 'ok', modelsDelay: 0, testMode: 'ok', setDelay: 400, sets: [], listings: 0,
    }
    globalThis.embeddingsFixture = fx
    const urlOf = service => ({ openai: 'https://api.openai.com/v1/embeddings', brave: 'https://ai-chat-backend-dev.bsg.brave.software/v1/embeddings', openrouter: 'https://openrouter.ai/api/v1/embeddings', custom: fx.kept.custom.url })[service] ?? null
    const hostOf = url => url ? url.replace(/^[a-z]+:\/\//, '').split(/[/?#]/)[0] : null
    const complete = service => service === 'openrouter' ? !!fx.kept.openrouter.model : service === 'custom' ? !!fx.kept.custom.url && !!fx.kept.custom.model : true
    const dimsOf = service => service === 'openai' ? ((fx.kept.openai.model ?? 'text-embedding-3-small') === 'text-embedding-3-large' ? 3072 : 1536) : service === 'brave' ? 768 : fx.kept[service].dimensions ?? null
    const reason = service => {
      if (!fx.available[service]) return `${NAMES[service]} is not set up`
      if (!complete(service)) return service === 'custom' ? 'the custom embeddings endpoint has no http(s) URL' : `no ${NAMES[service]} model is chosen`
      if (!dimsOf(service)) return `the ${NAMES[service]} model has not been tested, so the length of its vectors is not known`
      return null
    }
    const option = service => ({
      service, available: fx.available[service], reason: reason(service), url: urlOf(service), host: hostOf(urlOf(service)),
      model: service === 'brave' ? 'embedding_gemma' : fx.kept[service].model ?? null, dimensions: dimsOf(service),
      queryPrefix: fx.kept[service].queryPrefix ?? null, passagePrefix: fx.kept[service].passagePrefix ?? null,
      ...(service === 'custom' ? { keyStored: fx.kept.custom.keyStored } : {}),
      ...(service === 'openai' ? { models: ['text-embedding-3-small', 'text-embedding-3-large'] } : {}),
    })
    const retrieval = () => {
      const s = fx.selected
      const endpoint = s && !reason(s) ? { service: s, url: urlOf(s), host: hostOf(urlOf(s)), model: option(s).model, dimensions: dimsOf(s), development: s === 'brave' } : null
      return {
        enabled: fx.enabled, path: '/Users/example/.bravebot/settings.json', from: null, locked: null,
        disclosed: !!endpoint && fx.disclosed === endpoint.url, endpoint,
        unconfigured: endpoint ? null : s ? reason(s) : 'no embeddings service is selected',
        embeddings: { selected: s, inForce: s, locked: fx.locked, options: ['openai', 'brave', 'openrouter', 'custom'].map(option) },
      }
    }
    const report = () => ({ build: 'bravebot fixture', configured: true, problem: null, model: 'local/test', brave: false, bedrock: false, providers: [], selected: null, layers: [], overrides: [], managed: { path: null, keys: [] }, network: { roots: [], problem: null, trustsNothing: false, proxy: null, authenticated: false, unusableProxy: null, noProxy: null }, retrieval: retrieval() })
    const wait = ms => new Promise(resolve => setTimeout(resolve, ms))
    const set = async params => {
      fx.sets.push(params)
      await wait(fx.setDelay)
      if (fx.locked) return { error: { code: 'config', message: `the embeddings service is set by ${fx.locked.path} for this run` } }
      const service = params.service ?? params.select
      const before = JSON.stringify(fx.kept)
      const selectedBefore = fx.selected
      const values = params.values ?? {}
      const kept = fx.kept[service]
      for (const [name, value] of Object.entries(values)) {
        if (name === 'key') kept.keyStored = value !== null
        else kept[name] = value
      }
      if ('model' in values || 'url' in values) kept.dimensions = null
      if (params.select) fx.selected = params.select
      const touched = !!params.select || service === fx.selected
      const target = touched && fx.selected && complete(fx.selected) ? urlOf(fx.selected) : null
      const undo = () => { fx.kept = JSON.parse(before); fx.selected = selectedBefore }
      if (fx.enabled && touched) {
        if (!target) { undo(); return { error: { code: 'bad_request', message: `retrieval is on, so the embeddings service was left as it was: ${reason(params.select ?? service)}` } } }
        if (fx.disclosed !== target) {
          if (params.disclosed !== target) { undo(); return { ok: { state: retrieval(), needsDisclosure: target } } }
          fx.disclosed = target
        }
      }
      let test = null
      if (target) {
        if (fx.testMode === 'ok') {
          const dimensions = fx.selected === 'openrouter' ? DIMS[fx.kept.openrouter.model] ?? 1024 : fx.selected === 'custom' ? 768 : dimsOf(fx.selected)
          if (fx.selected === 'openrouter' || fx.selected === 'custom') fx.kept[fx.selected].dimensions = dimensions
          test = { ok: true, dimensions, millis: 140 }
        } else {
          const message = fx.testMode === 'key' ? 'HTTP 401' : 'connection refused'
          if (fx.enabled) { undo(); return { error: { code: 'bad_request', message: `retrieval is on, so the embeddings service was left as it was: its test call failed: ${message}` } } }
          test = { ok: false, kind: fx.testMode, message }
        }
      }
      return { ok: { state: retrieval(), test } }
    }
    const replace = (name, fn) => { ipcMain.removeHandler(name); ipcMain.handle(name, fn) }
    replace('bravebot:request', async (_, method, params = {}) => {
      if (method === 'agent.info') return { ok: { configured: true, build: 'bravebot fixture' } }
      if (method === 'session.list') return { ok: { sessions: [] } }
      if (method === 'models.list') return { ok: { models: [], defaultModel: null, warnings: [] } }
      if (method === 'settings.inspect') return { ok: report() }
      if (method === 'hooks.inspect') return { ok: { path: '/test/.bravebot/hooks.json', text: '{"hooks":[]}', entire: true, timeoutSeconds: { min: 1, max: 600, defaultFor: {} }, hooks: [] } }
      if (method === 'retrieval.embeddings.models') {
        fx.listings++
        await wait(fx.modelsDelay)
        if (fx.modelsMode === 'error') return { error: { code: 'config', message: 'OpenRouter answered HTTP 503' } }
        if (fx.modelsMode === 'empty') return { ok: { models: [] } }
        return { ok: { models: MODELS } }
      }
      if (method === 'retrieval.embeddings.set') return set(params)
      if (method === 'retrieval.set') {
        const now = retrieval()
        if (params.enabled && !now.endpoint) return { error: { code: 'bad_request', message: 'retrieval was not turned on: no embeddings service is selected' } }
        if (params.enabled && !now.disclosed && params.disclosed !== now.endpoint.url) return { error: { code: 'bad_request', message: 'not shown' } }
        if (params.enabled && params.disclosed) fx.disclosed = params.disclosed
        fx.enabled = params.enabled
        return { ok: retrieval() }
      }
      return { ok: {} }
    })
  })
  const fixture = () => app.evaluate(() => globalThis.embeddingsFixture)
  const tweak = fn => app.evaluate(fn)
  const snap = async name => {
    await page.waitForTimeout(400)
    await page.locator('[data-test="recall-row"]').screenshot({ path: join(output, name + '.png'), scale: 'css' })
  }
  const snapPage = async name => { await page.waitForTimeout(400); await page.screenshot({ path: join(output, name + '.png'), scale: 'css' }) }
  const checked = () => page.locator('[data-test="recall-switch"]').evaluate(el => el.shadowRoot?.querySelector('[role="switch"]')?.getAttribute('aria-checked'))
  const switchDisabled = () => page.locator('[data-test="recall-switch"]').evaluate(el => !!el.shadowRoot?.querySelector('[role="switch"]')?.disabled)
  const radio = service => page.locator(`[data-test="recall-option-${service}"] leo-radiobutton`)
  const open = async () => {
    await page.reload()
    await page.getByRole('button', { name: 'Settings', exact: true }).click()
    const view = page.locator('[data-test="settings-view"]')
    await view.waitFor()
    await view.locator('[data-test="settings-page-agent"]').click()
    await page.locator('[data-test="recall-embeddings"]').waitFor()
  }

  // Nothing chosen: OpenAI, OpenRouter and the custom endpoint are offered, Brave is not, and the switch waits.
  await open()
  for (const service of ['openai', 'openrouter', 'custom']) await radio(service).waitFor()
  assert.equal(await page.locator('[data-test="recall-option-brave"]').count(), 0)
  assert.ok((await page.locator('[data-test="recall-caption"]').textContent()).includes('Choose an embeddings service below to turn on recall.'))
  assert.equal(await switchDisabled(), true)
  await snap('01-nothing-chosen')

  // Choosing OpenRouter writes it and lists its models, slowly enough to see them load.
  await tweak(() => { globalThis.embeddingsFixture.modelsDelay = 2500 })
  await radio('openrouter').click()
  await page.locator('[data-test="recall-models-loading"]').waitFor()
  await page.waitForTimeout(600)
  await snap('02-models-loading')
  assert.deepEqual((await fixture()).sets, [{ select: 'openrouter' }])
  const dropdown = page.locator('[data-test="recall-model-openrouter"]')
  await dropdown.waitFor({ timeout: 8000 })
  await tweak(() => { globalThis.embeddingsFixture.modelsDelay = 0 })

  // The menu names each model with its context length and price.
  await dropdown.click()
  const small = page.locator('leo-option[value="openai/text-embedding-3-small"]')
  await small.waitFor()
  assert.equal((await small.textContent()).trim(), 'OpenAI: Text Embedding 3 Small · 8k context · $0.02/M tokens')
  assert.equal((await page.locator('leo-option[value="baai/bge-m3"]').textContent()).trim(), 'BAAI: bge-m3 · 8k context · free')
  await snapPage('03-models-listed')

  // Choosing a model tests it: the length is learned and the switch can be turned on.
  await tweak(() => { globalThis.embeddingsFixture.setDelay = 3000 })
  await small.click()
  await page.locator('[data-test="recall-test"]').waitFor()
  assert.equal(await dropdown.evaluate(el => el.value), 'openai/text-embedding-3-small')
  await page.locator('[data-test="recall-row"]').screenshot({ path: join(output, '04-testing.png'), scale: 'css' })
  await tweak(() => { globalThis.embeddingsFixture.setDelay = 400 })
  await page.locator('[data-test="recall-test-ok"]').getByText('Saved and tested. openrouter.ai answered in 140 ms; each message becomes 1536 numbers.').waitFor()
  assert.deepEqual((await fixture()).sets.at(-1), { select: 'openrouter', service: 'openrouter', values: { model: 'openai/text-embedding-3-small' } })
  assert.equal(await switchDisabled(), false)
  assert.ok((await page.locator('[data-test="recall-caption"]').textContent()).includes('openrouter.ai'))
  await snap('05-connected')

  // Turning recall on shows where it sends: openrouter.ai.
  await page.locator('[data-test="recall-switch"]').click()
  const dialog = page.locator('.recall-disclosure')
  await dialog.getByText('openrouter.ai', { exact: true }).waitFor()
  await snapPage('06-disclosure-turn-on')
  await page.locator('[data-test="recall-confirm"]').click()
  await page.waitForFunction(() => document.querySelector('[data-test="recall-switch"]')?.shadowRoot?.querySelector('[role="switch"]')?.getAttribute('aria-checked') === 'true')
  assert.equal((await fixture()).enabled, true)

  // While recall is on, a model whose test fails is refused: the old model stays, and the reason shows under it.
  await tweak(() => { globalThis.embeddingsFixture.testMode = 'key' })
  await page.locator('[data-test="recall-model-openrouter"]').click()
  await page.locator('leo-option[value="qwen/qwen3-embedding-8b"]').click()
  await page.locator('[data-test="recall-test-refused"]').waitFor()
  assert.equal((await page.locator('[data-test="recall-test-refused"]').textContent()).replace(/\s+/g, ' ').trim(), 'Not changed, the test failed: HTTP 401. OpenRouter is still in use as before.')
  assert.equal((await fixture()).kept.openrouter.model, 'openai/text-embedding-3-small')
  assert.equal(await page.locator('[data-test="recall-model-openrouter"]').evaluate(el => el.value), 'openai/text-embedding-3-small')
  await snap('07-test-refused-on')
  await tweak(() => { globalThis.embeddingsFixture.testMode = 'ok' })
  await page.locator('[data-test="recall-model-openrouter"]').click()
  await page.locator('leo-option[value="qwen/qwen3-embedding-8b"]').click()
  await page.locator('[data-test="recall-test-ok"]').getByText('Saved and tested. openrouter.ai answered in 140 ms; each message becomes 4096 numbers.').waitFor()

  // Moving to the custom endpoint while recall is on asks first, naming its host.
  const writes = (await fixture()).sets.length
  await radio('custom').click()
  await dialog.getByText('embed.example.com', { exact: true }).waitFor()
  await page.locator('[data-test="recall-confirm"]').getByText('Use embed.example.com').waitFor()
  await snapPage('08-disclosure-new-host')
  // Cancel puts the radio back and writes nothing.
  await page.locator('[data-test="recall-cancel"]').click()
  await dialog.waitFor({ state: 'detached' })
  assert.equal((await fixture()).selected, 'openrouter')
  assert.equal(await radio('openrouter').evaluate(el => !!el.shadowRoot?.querySelector('input')?.checked), true)
  assert.equal(await radio('custom').evaluate(el => !!el.shadowRoot?.querySelector('input')?.checked), false)
  await radio('custom').click()
  await page.locator('[data-test="recall-confirm"]').click()
  await page.locator('[data-test="recall-custom-key-stored"]').waitFor()
  assert.equal((await fixture()).selected, 'custom')
  assert.deepEqual((await fixture()).sets.slice(writes).map(set => set.disclosed ?? null), [null, null, 'https://embed.example.com/v1/embeddings'])
  await page.locator('[data-test="recall-reindex"]').waitFor()
  await page.locator('[data-test="recall-test-ok"]').waitFor()
  await page.locator('[data-test="recall-embeddings"]').screenshot({ path: join(output, '09-custom-key-stored.png'), scale: 'css' })
  // The stored key is never shown, and Replace offers a field for a new one.
  await page.locator('[data-test="recall-custom-key-replace"]').click()
  await page.locator('[data-test="recall-custom-key"]').waitFor()
  // A typed change reads as unsaved and clears the last result; Save and test says plainly that it passed.
  await page.locator('[data-test="recall-custom-key"]').locator('input').fill('sk-new')
  await page.locator('[data-test="recall-custom-unsaved"]').waitFor()
  assert.equal(await page.locator('[data-test="recall-test-ok"]').count(), 0)
  await page.locator('[data-test="recall-custom-save"]').click()
  await page.locator('[data-test="recall-test-ok"]').getByText('Saved and tested. embed.example.com answered in 140 ms', { exact: false }).waitFor()
  assert.equal(await page.locator('[data-test="recall-custom-unsaved"]').count(), 0)
  await page.locator('[data-test="recall-embeddings"]').screenshot({ path: join(output, '09b-custom-saved-and-tested.png'), scale: 'css' })

  // OpenRouter's list failing says why and can be tried again.
  await tweak(() => { const f = globalThis.embeddingsFixture; f.modelsMode = 'error'; f.selected = 'openrouter'; f.enabled = false })
  await open()
  await page.locator('[data-test="recall-models-error"]').getByText('Couldn’t load OpenRouter’s models: OpenRouter answered HTTP 503.').waitFor()
  await snap('10-models-error')
  await tweak(() => { globalThis.embeddingsFixture.modelsMode = 'ok' })
  const listings = (await fixture()).listings
  await page.locator('[data-test="recall-models-retry"]').click()
  await page.locator('[data-test="recall-model-openrouter"]').waitFor()
  assert.equal((await fixture()).listings, listings + 1)

  // With recall off, a failed test is saved and reads as what to check, with Test again.
  await tweak(() => { globalThis.embeddingsFixture.testMode = 'key' })
  await page.locator('[data-test="recall-model-openrouter"]').click()
  await page.locator('leo-option[value="openai/text-embedding-3-large"]').click()
  await page.locator('[data-test="recall-test-error"]').getByText('Saved, but the test failed: openrouter.ai refused the key. Check it and try again.').waitFor()
  await page.locator('[data-test="recall-test-again"]').waitFor()
  await snap('07b-test-failed-key-off')
  await tweak(() => { globalThis.embeddingsFixture.testMode = 'ok' })
  await page.locator('[data-test="recall-test-again"]').click()
  await page.locator('[data-test="recall-test-ok"]').getByText('Tested. openrouter.ai answered in 140 ms; each message becomes 3072 numbers.').waitFor()

  // A selected service that is no longer set up says so, and recall falls back.
  await tweak(() => { const f = globalThis.embeddingsFixture; f.selected = 'openai'; f.available.openai = false })
  await open()
  await page.locator('[data-test="recall-option-openai-unavailable"]').getByText('OpenAI can’t be used: OpenAI is not set up.', { exact: false }).waitFor()
  await snap('11-selected-unavailable')

  // A run file fixing the choice disables it and says what set it.
  await tweak(() => { const f = globalThis.embeddingsFixture; f.available.openai = true; f.selected = 'openrouter'; f.locked = { by: 'run', path: '/Users/example/work/team-settings.json' } })
  await open()
  await page.locator('[data-test="recall-embeddings-locked"]').getByText('Set by /Users/example/work/team-settings.json for this run').waitFor()
  const before = (await fixture()).sets.length
  await radio('custom').click({ force: true })
  await page.waitForTimeout(300)
  assert.equal((await fixture()).sets.length, before, 'a locked choice sent a request')
  assert.equal(await radio('custom').evaluate(el => !!el.shadowRoot?.querySelector('input')?.checked), false)
  await snap('12-locked')

  assert.deepEqual(errors, [])
  console.log(`embeddings: ok, screenshots in ${output}`)
} finally {
  await app.close()
}
