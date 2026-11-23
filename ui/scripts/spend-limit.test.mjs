// The session spend limit in the composer (TURN-8): what it shows against the limit, the dialog that
// sets or removes it through `session.limit`, and that the window reads it again when a turn ends.

import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { loadComponents } from './render-support.mjs'

const require = createRequire(import.meta.url)
const React = require('react')
const { renderToStaticMarkup } = require('react-dom/server')

test('the control shows spent against the limit, and names credits', async () => {
  const { spentAgainst, spendSentence } = await loadComponents('src/renderer/components/SpendLimit.tsx')
  assert.equal(spentAgainst({ limit: null, spent: 1200 }), null, 'no limit, no figures')
  assert.match(spentAgainst({ limit: { unit: 'tokens', count: 50000 }, spent: 1200 }), /^1\.2K \/ 50K$/)
  assert.match(spentAgainst({ limit: { unit: 'credits', count: 20 }, spent: 3 }), /^3 \/ 20 credits$/)
  assert.equal(spendSentence({ limit: null, spent: 0 }), 'No spend limit. 0 tokens spent')
  assert.match(spendSentence({ limit: { unit: 'tokens', count: 100 }, spent: 150 }), /^Spend limit: 150 of 100 tokens spent$/)
})

test('the dialog offers Remove only while a limit is set', async () => {
  const { SpendLimitDialog } = await loadComponents('src/renderer/components/SpendLimit.tsx')
  const set = renderToStaticMarkup(React.createElement(SpendLimitDialog, {
    session: 's', figures: { limit: { unit: 'tokens', count: 100 }, spent: 40 }, onDone() {},
  }))
  assert.ok(set.includes('data-test="spend-limit-remove"'), set)
  assert.ok(set.includes('data-test="spend-limit-set"'), set)
  assert.ok(set.includes('data-test="spend-limit-field"'), set)
  const none = renderToStaticMarkup(React.createElement(SpendLimitDialog, {
    session: 's', figures: { limit: null, spent: 40 }, onDone() {},
  }))
  assert.ok(!none.includes('data-test="spend-limit-remove"'), none)
})

test('the control reads and sets the limit through session.limit, and reads it again when a turn ends', () => {
  const source = readFileSync('src/renderer/components/SpendLimit.tsx', 'utf8')
  assert.match(source, /call<SessionLimit>\('session\.limit', \{ session \}\)/, 'read with no limit named')
  assert.match(source, /call<SessionLimit>\('session\.limit', \{ session, limit \}\)/, 'set with a figure, or null to remove')
  assert.match(source, /\}, \[session, running, pending\]\)/, 'read again when the turn ends or a question is answered')
  const composer = readFileSync('src/renderer/components/Composer.tsx', 'utf8')
  assert.match(composer, /<SpendLimit [^>]*session=\{session\} running=\{running\} pending=\{pending\}/)
  const main = readFileSync('src/main/index.ts', 'utf8')
  assert.ok(main.includes("'session.limit',"), 'session.limit is allowed from the window')
})
