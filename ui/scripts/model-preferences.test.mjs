import test from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { buildSync } from 'esbuild'

const require = createRequire(import.meta.url)
const load = (entry) => {
  const source = buildSync({ entryPoints: [entry], bundle: true, write: false, platform: 'node', format: 'cjs' }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
}
const preferences = load('src/shared/model-preferences.ts')
const { arrange, setProviderShown, setModelShown, setOrder, resetProvider, hasChanges, parseProviderPreferences } = preferences
const { parseExperience } = load('src/shared/experience.ts')
const { accountProviderKey, providerLogo } = load('src/shared/accounts.ts')

const row = (id, providerKey) => ({ id, providerKey })
const catalogue = [
  row('bedrock-a', 'bedrock'), row('bedrock-b', 'bedrock'), row('bedrock-c', 'bedrock'),
  row('openrouter/x', 'gateway:openrouter'), row('openrouter/y', 'gateway:openrouter'),
  row('xai/grok', 'xai'),
]
const ids = (arranged) => arranged.models.map((model) => model.id)

test('with no choices made the picker lists every model, grouped by service in catalogue order', () => {
  const arranged = arrange(catalogue, {}, null)
  assert.deepEqual(arranged.providers, ['bedrock', 'gateway:openrouter', 'xai'])
  assert.deepEqual(ids(arranged), catalogue.map((model) => model.id))
})

test('a hidden service and a hidden model leave the picker, and other services keep theirs', () => {
  let chosen = setProviderShown({}, 'gateway:openrouter', false)
  chosen = setModelShown(chosen, 'bedrock', 'bedrock-b', false)
  const arranged = arrange(catalogue, chosen, null)
  assert.deepEqual(arranged.providers, ['bedrock', 'xai'])
  assert.deepEqual(ids(arranged), ['bedrock-a', 'bedrock-c', 'xai/grok'])
  // Hiding a model by id in one service hides nothing of the same name in another.
  const elsewhere = arrange([...catalogue, row('bedrock-b', 'xai')], setModelShown({}, 'bedrock', 'bedrock-b', false), null)
  assert.ok(elsewhere.models.some((model) => model.id === 'bedrock-b' && model.providerKey === 'xai'))
})

test('the model in use stays listed even when its service or its row was hidden', () => {
  const hidden = setProviderShown(setModelShown({}, 'xai', 'xai/grok', false), 'xai', false)
  assert.deepEqual(ids(arrange(catalogue, hidden, 'xai/grok')).slice(-1), ['xai/grok'])
  assert.ok(!ids(arrange(catalogue, hidden, 'bedrock-a')).includes('xai/grok'))
})

test('a chosen order puts the named models first and the rest after, in catalogue order', () => {
  const chosen = setOrder({}, 'bedrock', ['bedrock-c', 'bedrock-a'])
  assert.deepEqual(ids(arrange(catalogue, chosen, null)).slice(0, 3), ['bedrock-c', 'bedrock-a', 'bedrock-b'])
  // A model the service no longer lists is ignored, and a new one is appended.
  const stale = setOrder({}, 'gateway:openrouter', ['gone', 'openrouter/y'])
  assert.deepEqual(ids(arrange(catalogue, stale, null)).slice(3, 5), ['openrouter/y', 'openrouter/x'])
})

test('reset forgets one service and leaves the others as chosen', () => {
  let chosen = setOrder({}, 'bedrock', ['bedrock-b'])
  chosen = setProviderShown(chosen, 'bedrock', false)
  chosen = setModelShown(chosen, 'xai', 'xai/grok', false)
  assert.ok(hasChanges(chosen, 'bedrock'))
  const reset = resetProvider(chosen, 'bedrock')
  assert.ok(!hasChanges(reset, 'bedrock'))
  assert.ok(hasChanges(reset, 'xai'))
  assert.deepEqual(ids(arrange(catalogue, reset, null)), ['bedrock-a', 'bedrock-b', 'bedrock-c', 'openrouter/x', 'openrouter/y'])
})

test('showing again what was hidden leaves nothing to reset', () => {
  const back = setModelShown(setModelShown(setProviderShown(setProviderShown({}, 'xai', false), 'xai', true), 'xai', 'm', false), 'xai', 'm', true)
  assert.ok(!hasChanges(back, 'xai'))
  assert.deepEqual(back, {})
})

test('the choices survive the experience file, and a malformed entry is dropped', () => {
  const chosen = setOrder(setModelShown(setProviderShown({}, 'xai', false), 'bedrock', 'bedrock-a', false), 'bedrock', ['bedrock-c'])
  const saved = parseExperience(JSON.parse(JSON.stringify({ providers: chosen }))).providers
  assert.deepEqual({ ...saved }, chosen)
  const parsed = parseProviderPreferences({ xai: { hidden: 'yes', hiddenModels: [3, '', 'ok', 'ok'] }, '': { hidden: true }, bad: 7, list: [] })
  assert.deepEqual({ ...parsed }, { xai: { hiddenModels: ['ok'] } })
  assert.deepEqual({ ...parseExperience(null).providers }, {})
})

test('an account row names the service key its models carry', () => {
  assert.equal(accountProviderKey({ way: 'gateway', id: 'openrouter' }), 'gateway:openrouter')
  assert.equal(accountProviderKey({ way: 'gateway', id: null }), null)
  assert.equal(accountProviderKey({ way: 'bedrock', id: 'work-profile' }), 'bedrock')
  assert.equal(accountProviderKey({ way: 'chatgpt', id: null }), 'chatgpt')
  assert.equal(providerLogo('gateway:openrouter'), 'openrouter')
  assert.equal(providerLogo('bedrock'), 'amazon-color')
  assert.equal(providerLogo('default'), 'key')
})
