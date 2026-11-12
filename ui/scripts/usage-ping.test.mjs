import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'
const require = createRequire(import.meta.url)
function load(path) {
  const source = buildSync({ entryPoints: [path], bundle: true, write: false, platform: 'node', format: 'cjs' }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
}
const { changeUsagePing, lockedLine, reportIn } = load('src/renderer/usage-ping.ts')

const on = { enabled: true, changeable: true, reason: null }

test('a locked switch names what holds it, and a free one says nothing', () => {
  assert.equal(lockedLine(null), null)
  assert.match(lockedLine({ code: 'environment', variable: 'DO_NOT_TRACK' }), /DO_NOT_TRACK/)
  assert.match(lockedLine({ code: 'setting' }), /usagePing/)
  assert.match(lockedLine({ code: 'admin' }), /administrator/)
  assert.match(lockedLine({ code: 'incognito' }), /incognito/)
})

test('an answer that is not a report is not read as one', () => {
  assert.deepEqual(reportIn({ ok: on }), on)
  assert.equal(reportIn({ error: { code: 'bad_request', message: 'no' } }), null)
  assert.equal(reportIn({ ok: { enabled: 'yes', changeable: true } }), null)
})

test('the switch shows what the bridge kept, and goes back when nothing was kept', async () => {
  const off = { enabled: false, changeable: true, reason: null }
  const sent = []
  const kept = await changeUsagePing(async (enabled) => { sent.push(enabled); return { ok: { ...off, saved: true } } }, on, false)
  assert.deepEqual(sent, [false])
  assert.deepEqual(kept, { report: off, unsaved: false })

  const unwritten = await changeUsagePing(async () => ({ ok: { ...on, saved: false } }), on, false)
  assert.equal(unwritten.unsaved, true)
  assert.equal(unwritten.report.enabled, true, 'the switch shows a choice nothing kept')

  const refused = await changeUsagePing(async () => ({ error: { code: 'bad_request', message: 'locked' } }), on, false)
  assert.deepEqual(refused, { report: on, unsaved: true })

  const thrown = await changeUsagePing(async () => { throw new Error('the agent is not running') }, on, false)
  assert.deepEqual(thrown, { report: on, unsaved: true })
})
