// RETRIEVE-35 to RETRIEVE-37 in the window: the embeddings service picker under the recall switch.
// The steps are pure functions; the group is rendered through `react-dom/server`.

import test from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { loadComponents } from './render-support.mjs'

const require = createRequire(import.meta.url)
const React = require('react')
const { renderToStaticMarkup } = require('react-dom/server')

let bundle
const picker = () => (bundle ??= loadComponents('src/renderer/components/EmbeddingsPicker.tsx'))
let recallBundle
const recall = () => (recallBundle ??= loadComponents('src/renderer/components/RecallSetting.tsx'))

const option = (service, over = {}) => ({
  service, available: true, reason: null, url: null, host: null, model: null, dimensions: null,
  queryPrefix: null, passagePrefix: null, ...over,
})
const OPENROUTER = option('openrouter', { url: 'https://openrouter.ai/api/v1/embeddings', host: 'openrouter.ai' })
const choice = (over = {}) => ({
  selected: null, inForce: null, locked: null,
  options: [
    option('openai', { available: false, reason: 'OpenAI is not set up' }),
    option('brave', { available: false, reason: 'Brave is not set up' }),
    OPENROUTER,
    option('custom', { keyStored: false }),
  ],
  ...over,
})
const state = (over = {}) => ({
  enabled: false, path: '/Users/example/.bravebot/settings.json', from: null, locked: null, disclosed: false,
  endpoint: null, unconfigured: 'no embeddings service is selected', embeddings: choice(), ...over,
})
const render = async (props) => {
  const { RecallGroup } = await recall()
  return renderToStaticMarkup(React.createElement(RecallGroup, { onSaved() {}, ...props }))
}

test('only the services that can be chosen are listed, and a selected one that no longer can stays', async () => {
  const { shownOptions } = await picker()
  assert.deepEqual(shownOptions(choice()).map(o => o.service), ['openrouter', 'custom'])
  assert.deepEqual(shownOptions(choice({ selected: 'openai' })).map(o => o.service), ['openai', 'openrouter', 'custom'])
})

test('the chosen radio is the one picked here, else the one selected, else the one in force', async () => {
  const { currentService } = await picker()
  assert.equal(currentService(choice({ selected: 'custom', inForce: 'custom' }), 'openrouter'), 'openrouter')
  assert.equal(currentService(choice({ selected: 'custom', inForce: 'custom' }), null), 'custom')
  assert.equal(currentService(choice({ inForce: 'brave' }), null), 'brave')
  assert.equal(currentService(choice(), null), null)
})

test('choosing a service writes it at once, but while recall is on an incomplete one waits for its model', async () => {
  const { chooseStep } = await picker()
  assert.deepEqual(chooseStep(state(), OPENROUTER), { kind: 'send', params: { select: 'openrouter' } })
  assert.deepEqual(chooseStep(state({ enabled: true }), OPENROUTER), { kind: 'pending' })
  assert.deepEqual(chooseStep(state({ enabled: true }), { ...OPENROUTER, model: 'openai/text-embedding-3-small' }),
    { kind: 'send', params: { select: 'openrouter' } })
  assert.deepEqual(chooseStep(state({ enabled: true }), option('custom', { url: 'https://e.example/v1/embeddings' })), { kind: 'pending' })
  assert.deepEqual(chooseStep(state({ enabled: true }), option('openai')), { kind: 'send', params: { select: 'openai' } })
})

test('a model reads with its context length and its price per million tokens', async () => {
  const { modelText, priceText, sortedModels } = await picker()
  assert.equal(priceText('0.00000002'), '$0.02/M tokens')
  assert.equal(priceText('0.000000135'), '$0.14/M tokens')
  assert.equal(priceText('0'), 'free')
  assert.equal(priceText(null), null)
  assert.equal(priceText('n/a'), null)
  assert.equal(modelText({ id: 'openai/text-embedding-3-small', name: 'OpenAI: Text Embedding 3 Small', contextLength: 8192, pricing: { prompt: '0.00000002' } }),
    'OpenAI: Text Embedding 3 Small · 8k context · $0.02/M tokens')
  assert.equal(modelText({ id: 'x/y', name: '', contextLength: 512, pricing: { prompt: null } }), 'x/y · 512 tokens context')
  assert.deepEqual(sortedModels([{ id: 'b', name: 'Bee' }, { id: 'a', name: 'Ant' }]).map(m => m.id), ['a', 'b'])
})

test('each test outcome says what to check', async () => {
  const { testText } = await picker()
  const at = { ...OPENROUTER, model: 'm/x' }
  assert.equal(testText({ ok: true, dimensions: 1536, millis: 80 }, at, true), 'Saved and tested. openrouter.ai answered in 80 ms; each message becomes 1536 numbers.')
  assert.equal(testText({ ok: true, dimensions: 1536, millis: 80 }, at, false), 'Tested. openrouter.ai answered in 80 ms; each message becomes 1536 numbers.')
  assert.equal(testText({ ok: false, kind: 'key', message: '' }, at, true), 'Saved, but the test failed: openrouter.ai refused the key. Check it and try again.')
  assert.equal(testText({ ok: false, kind: 'key', message: '' }, at, false), 'openrouter.ai refused the key. Check it and try again.')
  assert.equal(testText({ ok: false, kind: 'not-found', message: '' }, at, false),
    'Nothing answered at https://openrouter.ai/api/v1/embeddings for m/x (404). Check the URL and model.')
  assert.equal(testText({ ok: false, kind: 'shape', message: '' }, at, false), 'openrouter.ai answered, but not with embeddings. Check the URL ends in /embeddings.')
  assert.equal(testText({ ok: false, kind: 'network', message: 'connection refused' }, at, false), 'Couldn’t reach openrouter.ai: connection refused.')
  assert.equal(testText({ ok: false, kind: 'other', message: 'boom' }, at, false), 'The test failed: boom.')
})

test('the custom endpoint saves only a usable URL and model, and sends a key only where one was typed', async () => {
  const { draftReady, customParams } = await picker()
  assert.equal(draftReady({ url: 'e.example', model: 'm' }), false)
  assert.equal(draftReady({ url: 'https://e.example/v1/embeddings', model: ' ' }), false)
  assert.equal(draftReady({ url: 'https://e.example/v1/embeddings', model: 'm' }), true)
  const draft = { url: ' https://e.example/v1/embeddings ', model: 'm', queryPrefix: '', passagePrefix: 'passage: ', key: '', replacing: false }
  assert.deepEqual(customParams(draft), {
    select: 'custom', service: 'custom',
    values: { url: 'https://e.example/v1/embeddings', model: 'm', queryPrefix: null, passagePrefix: 'passage: ' },
  })
  assert.equal(customParams({ ...draft, key: 'sk-1' }).values.key, 'sk-1')
})

test('with no service resolved the switch cannot be turned on and the caption says to choose one', async () => {
  const { switchBlocked, recallCaption } = await recall()
  assert.equal(switchBlocked(state()), true)
  assert.equal(switchBlocked(state({ enabled: true })), false)
  assert.equal(switchBlocked(state({ embeddings: undefined })), false)
  assert.ok(recallCaption(state()).startsWith('Choose an embeddings service below to turn on recall.'))
  const markup = await render({ state: state() })
  assert.ok(markup.includes('data-test="recall-embeddings"'))
  assert.ok(markup.includes('data-test="recall-option-openrouter"'))
  assert.ok(markup.includes('data-test="recall-option-custom"'))
  assert.ok(!markup.includes('recall-option-openai'))
  assert.ok(!markup.includes('recall-model-openrouter'))
})

test('OpenRouter selected shows its models loading first', async () => {
  const markup = await render({ state: state({ embeddings: choice({ selected: 'openrouter', inForce: 'openrouter' }) }) })
  assert.ok(markup.includes('data-test="recall-models-loading"'))
  assert.ok(markup.includes('Loading models…'))
})

test('a selected service that is no longer set up says so and why', async () => {
  const markup = await render({ state: state({ embeddings: choice({ selected: 'openai' }) }) })
  assert.ok(markup.includes('data-test="recall-option-openai-unavailable"'))
  assert.ok(markup.includes('OpenAI can’t be used: OpenAI is not set up. Long conversations are summarised until you choose another service.'))
})

test('the custom endpoint shows a stored key without the key, with Replace and Remove', async () => {
  const custom = option('custom', { url: 'https://e.example/v1/embeddings', host: 'e.example', model: 'm', dimensions: 384, keyStored: true, queryPrefix: 'query: ' })
  const embeddings = choice({ selected: 'custom', inForce: 'custom', options: [OPENROUTER, custom] })
  const markup = await render({ state: state({ embeddings }) })
  for (const id of ['recall-custom-url', 'recall-custom-model', 'recall-custom-key-stored', 'recall-custom-key-replace', 'recall-custom-key-remove', 'recall-custom-query-prefix', 'recall-custom-save']) {
    assert.ok(markup.includes(`data-test="${id}"`), id)
  }
  assert.ok(!markup.includes('data-test="recall-custom-key"'))
  assert.ok(markup.includes('Each message becomes 384 numbers.'))
})

test('a set-up service that is not ready yet says why', async () => {
  const embeddings = choice({ selected: 'openrouter', options: [{ ...OPENROUTER, model: 'm/x', reason: 'the OpenRouter model has not been tested' }] })
  const { EmbeddingsPicker } = await picker()
  const markup = renderToStaticMarkup(React.createElement(EmbeddingsPicker, { state: state({ embeddings }), choice: embeddings, onSaved() {} }))
  assert.ok(markup.includes('Not ready yet: the OpenRouter model has not been tested.'))
})

test('a locked choice says what set it and offers nothing to change', async () => {
  const { choiceLockedText } = await picker()
  assert.equal(choiceLockedText(choice()), null)
  assert.equal(choiceLockedText(choice({ locked: { by: 'managed', path: '/m.json' } })), 'Set by your administrator')
  assert.equal(choiceLockedText(choice({ locked: { by: 'run', path: '/w/team.json' } })), 'Set by /w/team.json for this run')
  const markup = await render({ state: state({ embeddings: choice({ locked: { by: 'run', path: '/w/team.json' } }) }) })
  assert.ok(markup.includes('data-test="recall-embeddings-locked"'))
})

test('the dialog for a move names the new host and offers to use it', async () => {
  const { RecallDisclosure } = await recall()
  const markup = renderToStaticMarkup(React.createElement(RecallDisclosure, {
    state: state({ enabled: true }), target: { service: 'openrouter', host: 'openrouter.ai', development: false }, confirm: 'Use openrouter.ai',
    onCancel() {}, onConfirm() {},
  }))
  assert.ok(markup.includes('OpenRouter’s embeddings service'))
  assert.ok(markup.includes('openrouter.ai'))
  assert.ok(markup.includes('Use openrouter.ai'))
  assert.ok(!markup.includes('Turn on recall</'))
})

test('while recall is on, a change whose test fails is refused and reads under the option kept as it was', async () => {
  const { refusalStep, TestOutcome } = await picker()
  const message = 'retrieval is on, so the embeddings service was left as it was: its test call failed: HTTP 401'
  assert.deepEqual(refusalStep('openrouter', 'openrouter', message), { kind: 'inline', message: 'the test failed: HTTP 401' })
  assert.deepEqual(refusalStep('openrouter', 'openrouter', 'boom'), { kind: 'inline', message: 'boom' })
  assert.deepEqual(refusalStep('custom', 'openrouter', message), { kind: 'problem', text: `The embeddings service was not changed: ${message}.` })
  const at = { ...OPENROUTER, model: 'openai/text-embedding-3-small', dimensions: 1536 }
  const markup = renderToStaticMarkup(React.createElement(TestOutcome, { option: at, testing: false, result: null, refused: 'the test failed: HTTP 401', stale: false, disabled: false, onAgain() {} }))
  assert.ok(markup.includes('data-test="recall-test-refused"'))
  assert.ok(markup.includes('Not changed, the test failed: HTTP 401. OpenRouter is still in use as before.'))
  assert.ok(!markup.includes('Each message becomes'))
})

test('an OpenRouter option with no model leaves the not-ready line to the model menu', async () => {
  const { TestOutcome } = await picker()
  const markup = renderToStaticMarkup(React.createElement(TestOutcome, { option: { ...OPENROUTER, reason: 'no OpenRouter model is chosen' }, testing: false, result: null, refused: null, stale: false, disabled: false, onAgain() {} }))
  assert.equal(markup, '')
})
