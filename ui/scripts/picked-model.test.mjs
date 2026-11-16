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
const { modelKey, pickedModel, rememberModel, rememberPickedModel, reopenedModel, pickModel } = load('src/renderer/models.ts')

const memory = () => {
  const held = new Map()
  globalThis.localStorage = {
    getItem: (key) => held.get(key) ?? null,
    setItem: (key, value) => { held.set(key, String(value)) },
    removeItem: (key) => { held.delete(key) },
  }
  return held
}

test('a model the person picked is read back for that conversation and for no other', () => {
  memory()
  rememberModel('/work', 'a', 'picked/model')
  assert.equal(pickedModel('/work', 'a'), 'picked/model')
  assert.equal(pickedModel('/work', 'b'), null)
  assert.equal(pickedModel('/elsewhere', 'a'), null)
})

test('clearing the pick leaves a conversation with none', () => {
  memory()
  rememberModel('/work', 'a', 'picked/model')
  rememberModel('/work', 'a', null)
  assert.equal(pickedModel('/work', 'a'), null)
})

// BACKEND-33: before a checkout's default model was refused, the window recorded every model a
// conversation showed, so an entry under the old key may be a default a checkout chose. Reading it
// back would go on sending that model after the checkout stopped deciding it.
test('an entry written under the key that recorded defaults too is not read as a pick', () => {
  const held = memory()
  held.set(`bravebot.conversation-model:${JSON.stringify(['/work', 'a'])}`, 'checkout/model')
  assert.equal(pickedModel('/work', 'a'), null)
  assert.ok(!modelKey('/work', 'a').startsWith('bravebot.conversation-model:'))
})

test('an unavailable store reads as no pick and a write does not throw', () => {
  globalThis.localStorage = {
    getItem: () => { throw new Error('unavailable') },
    setItem: () => { throw new Error('full') },
    removeItem: () => { throw new Error('unavailable') },
  }
  assert.equal(pickedModel('/work', 'a'), null)
  assert.doesNotThrow(() => rememberModel('/work', 'a', 'm'))
})

// BACKEND-33: a conversation opened with the default the backend supplied shows that model, and
// storing it would go on requesting it after the settings that named it stopped applying. Only a
// pick is stored: this is the decision App.tsx makes when it opens, reopens and picks.
test('a default the backend supplied is shown and not stored', () => {
  const held = memory()
  const shown = reopenedModel(undefined, null, 'backend/default')
  assert.equal(shown.model, 'backend/default')
  assert.equal(shown.modelPicked, false)
  rememberPickedModel('/work', 'a', shown)
  assert.equal(held.size, 0)
  assert.equal(pickedModel('/work', 'a'), null)
})

test('the model a bot names is shown and not stored as a pick', () => {
  const held = memory()
  const shown = reopenedModel('bot/model', null, 'backend/default')
  assert.equal(shown.model, 'bot/model')
  rememberPickedModel('/work', 'a', shown)
  assert.equal(held.size, 0)
})

test('a model the person picked is stored, and a reopened conversation shows it over the default', () => {
  memory()
  const picked = pickModel(reopenedModel(undefined, null, 'backend/default'), 'picked/model')
  assert.equal(picked.modelPicked, true)
  rememberPickedModel('/work', 'a', picked)
  assert.equal(pickedModel('/work', 'a'), 'picked/model')
  const reopened = reopenedModel(undefined, pickedModel('/work', 'a'), 'backend/changed')
  assert.equal(reopened.model, 'picked/model')
  assert.equal(reopened.modelPicked, true)
})
