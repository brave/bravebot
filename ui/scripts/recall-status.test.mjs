// What a bot's Recall block says at each state of its index (RETRIEVE-29).
import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'
const require = createRequire(import.meta.url)
const source = buildSync({ entryPoints: ['src/renderer/recall-status.ts'], bundle: true, write: false, platform: 'node', format: 'cjs' }).outputFiles[0].text
const module = { exports: {} }
new Function('require', 'module', 'exports', source)(require, module, module.exports)
const { recallLine, building } = module.exports

test('each state of the index has its own line, and off has none', () => {
  assert.equal(recallLine({ state: 'off', indexed: 0, eligible: 0 }, null), null)
  assert.equal(recallLine(null, null), null)
  const indexing = recallLine({ state: 'indexing', indexed: 30, eligible: 120 }, null)
  assert.equal(indexing.text, 'Indexing 30 of 120 earlier messages')
  assert.equal(indexing.progress, 0.25)
  assert.ok(indexing.until.includes('summarises'))
  assert.ok(recallLine({ state: 'rebuilding', indexed: 0, eligible: 0 }, null).text.startsWith('Rebuilding'))
  assert.equal(recallLine({ state: 'ready', indexed: 1, eligible: 1 }, null).text, 'Ready · 1 earlier message can be recalled')
  assert.ok(recallLine({ state: 'untrusted', indexed: 0, eligible: 4 }, null).text.startsWith('Not available'))
  assert.equal(building({ state: 'indexing' }), true)
  assert.equal(building({ state: 'ready' }), false)
})

test('a last turn that could not reach the service says so over the index, and no other fallback does', () => {
  const ready = { state: 'ready', indexed: 9, eligible: 9 }
  assert.equal(recallLine(ready, 'key').text, 'Unavailable · key refused')
  assert.ok(recallLine(ready, 'unreachable').text.includes('could not be reached'))
  assert.ok(recallLine(ready, 'failed').text.includes('returned an error'))
  assert.ok(recallLine(ready, 'not ready').text.startsWith('Ready'))
  assert.equal(recallLine({ state: 'indexing', indexed: 3, eligible: 9 }, 'key').progress, undefined)
  assert.ok(recallLine({ state: 'untrusted', indexed: 0, eligible: 9 }, 'key').text.startsWith('Not available'))
})

test('an indexing pass the service gave nothing says why, stops asking, and a refused key says where to fix it', () => {
  const refused = recallLine({ state: 'indexing', indexed: 0, eligible: 9, failure: 'key' }, null)
  assert.equal(refused.text, 'Unavailable · key refused')
  assert.equal(refused.progress, undefined)
  assert.ok(refused.until.includes('embeddings key in Settings'))
  assert.ok(recallLine({ state: 'rebuilding', indexed: 0, eligible: 9, failure: 'unreachable' }, null).text.includes('could not be reached'))
  assert.equal(recallLine({ state: 'indexing', indexed: 0, eligible: 9, failure: 'failed' }, null).until, undefined)
  assert.ok(recallLine({ state: 'indexing', indexed: 0, eligible: 9, failure: 'failed' }, 'not ready').text.includes('returned an error'))
  assert.ok(recallLine({ state: 'indexing', indexed: 0, eligible: 9, failure: 'failed' }, 'key').text.includes('key refused'))
  assert.equal(building({ state: 'indexing', failure: 'key' }), false)
  assert.equal(building({ state: 'rebuilding', failure: 'out of time' }), false)
})

test('no service set up, and a service that did not answer in time, each say so rather than ready', () => {
  const unconfigured = recallLine({ state: 'unconfigured', indexed: 0, eligible: 0 }, null)
  assert.equal(unconfigured.text, 'Not set up · choose an embeddings service in Settings')
  assert.equal(unconfigured.progress, undefined)
  assert.equal(recallLine({ state: 'ready', indexed: 4, eligible: 4 }, 'unconfigured').text, unconfigured.text)
  assert.equal(building({ state: 'unconfigured' }), false)
  assert.equal(
    recallLine({ state: 'ready', indexed: 9, eligible: 9 }, 'out of time').text,
    'Unavailable · the service did not answer in time',
  )
})
