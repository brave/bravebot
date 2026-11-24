// The crash reports switch in Settings, General, Privacy: what holds it off is named, and the
// shared switch helpers read and keep it the way they do for the usage ping.

import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'
const require = createRequire(import.meta.url)
function load(path) {
  const source = buildSync({ entryPoints: [path], bundle: true, write: false, platform: 'node', format: 'cjs' }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
}
const { crashReportsLockedLine } = load('src/renderer/crash-reports.ts')
const { changeUsagePing } = load('src/renderer/usage-ping.ts')

const on = { enabled: true, changeable: true, reason: null }

test('a locked switch names what holds it, and a free one says nothing', () => {
  assert.equal(crashReportsLockedLine(null), null)
  assert.match(crashReportsLockedLine({ code: 'environment', variable: 'BRAVEBOT_CRASH_REPORTS' }), /BRAVEBOT_CRASH_REPORTS/)
  assert.match(crashReportsLockedLine({ code: 'admin' }), /administrator/)
  assert.match(crashReportsLockedLine({ code: 'incognito' }), /incognito/)
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
})

test('Privacy has the crash reports row beside the usage ping row, each kept by its own methods', () => {
  const settings = readFileSync('src/renderer/components/GeneralSettings.tsx', 'utf8')
  assert.match(settings, /id="usage-ping" method="usage_ping"/)
  assert.match(settings, /id="crash-reports" method="crash_reports"/)
  assert.match(settings, /`\$\{method\}\.get`/, 'the row reads with its own get')
  assert.match(settings, /`\$\{method\}\.set`/, 'and keeps with its own set')
  assert.match(settings, /data-test=\{`\$\{id\}-row`\}/, 'the rows keep the data-test ids drivers use')
  assert.match(settings, /data-test=\{`\$\{id\}-locked`\}/)
  assert.match(settings, /disabled=\{!report \|\| !report\.changeable \|\| busy\}/, 'a locked switch cannot be changed')
})
