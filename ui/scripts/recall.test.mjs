// RETRIEVE-24 and RETRIEVE-25 in the window: the Recall earlier messages switch in Agent settings.
// The steps are pure functions, and the group and its dialog are rendered through
// `react-dom/server` with `Modal` drawn in place of its portal.

import test from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { loadComponents } from './render-support.mjs'

const require = createRequire(import.meta.url)
const React = require('react')
const { renderToStaticMarkup } = require('react-dom/server')

let bundle
const recall = () => (bundle ??= loadComponents('src/renderer/components/RecallSetting.tsx'))

const URL = 'https://ai-chat-backend-dev.bsg.brave.software/v1/embeddings'
const HOST = 'ai-chat-backend-dev.bsg.brave.software'
const state = (over = {}) => ({
  enabled: false, path: '/Users/example/.bravebot/settings.json', from: null, locked: null, disclosed: false,
  endpoint: { service: 'brave', url: URL, host: HOST, model: 'embedding_gemma', dimensions: 768, development: true },
  ...over,
})

test('turning recall on asks first until the endpoint was shown, and the agreed request names it', async () => {
  const { recallChosen, recallAgreed } = await recall()
  assert.deepEqual(recallChosen(state(), true), { kind: 'ask' })
  assert.deepEqual(recallChosen(state({ disclosed: true }), true), { kind: 'write', params: { enabled: true } })
  assert.deepEqual(recallChosen(state({ enabled: true }), false), { kind: 'write', params: { enabled: false } })
  assert.deepEqual(recallAgreed(state()), { kind: 'write', params: { enabled: true, disclosed: URL } })
})

test('the caption names the service, its host, what is never sent and the file it is saved in', async () => {
  const { RecallGroup } = await recall()
  const markup = renderToStaticMarkup(React.createElement(RecallGroup, { state: state(), onSaved() {} }))
  assert.ok(markup.includes('Bots'))
  assert.ok(markup.includes('Recall earlier messages'))
  const caption = markup.slice(markup.indexOf('data-test="recall-caption"'))
  assert.ok(caption.includes('Brave’s embeddings service (development)'))
  assert.ok(caption.includes(HOST))
  assert.ok(caption.includes('untrusted content, images and PDFs, or anything in incognito'))
  assert.ok(caption.includes('/Users/example/.bravebot/settings.json'))
  assert.ok(!markup.includes('data-test="recall-locked"'))
})

test('with no endpoint the caption says why and turning on needs no dialog', async () => {
  const { RecallGroup, recallChosen } = await recall()
  const none = state({ endpoint: null, unconfigured: 'OpenRouter has no key' })
  assert.deepEqual(recallChosen(none, true), { kind: 'write', params: { enabled: true } })
  const markup = renderToStaticMarkup(React.createElement(RecallGroup, { state: none, onSaved() {} }))
  const caption = markup.slice(markup.indexOf('data-test="recall-caption"'))
  assert.ok(caption.includes('No embeddings service is set up: OpenRouter has no key'))
  assert.ok(!caption.includes(HOST))
})

test('the caption names the service the settings choose', async () => {
  const { serviceName } = await recall()
  const at = service => state({ endpoint: { service, url: 'https://e.example/v1/embeddings', host: 'e.example', model: 'm', dimensions: 8, development: false } })
  assert.equal(serviceName(at('openai')), 'OpenAI’s embeddings service')
  assert.equal(serviceName(at('openrouter')), 'OpenRouter’s embeddings service')
  assert.equal(serviceName(at('custom')), 'your embeddings endpoint')
  assert.equal(serviceName(at('brave')), 'Brave’s embeddings service')
})

test('a locked switch says what set it', async () => {
  const { RecallGroup, lockedText } = await recall()
  const managed = state({ locked: { by: 'managed', path: '/Library/managed.json' } })
  const run = state({ enabled: true, locked: { by: 'run', path: '/work/team.json' } })
  assert.equal(lockedText(state()), null)
  assert.equal(lockedText(managed), 'Set by your administrator')
  assert.equal(lockedText(run), 'Set by /work/team.json for this run')
  const markup = renderToStaticMarkup(React.createElement(RecallGroup, { state: managed, onSaved() {} }))
  assert.ok(markup.includes('Set by your administrator'))
})

test('the dialog says where it sends and what never goes, with Cancel just before Turn on recall', async () => {
  const { RecallDisclosure } = await recall()
  const markup = renderToStaticMarkup(React.createElement(RecallDisclosure, { state: state(), onCancel() {}, onConfirm() {} }))
  assert.ok(markup.includes('data-title="Recall earlier messages"'))
  assert.ok(markup.includes(HOST))
  assert.ok(markup.includes('images and PDFs, and anything in incognito'))
  assert.ok(markup.includes('Project conversations are not affected.'))
  assert.ok(markup.indexOf('data-test="recall-cancel"') < markup.indexOf('data-test="recall-confirm"'))
  assert.ok(markup.includes('Turn on recall'))
  assert.ok(!markup.includes('modal-leading'))
})
