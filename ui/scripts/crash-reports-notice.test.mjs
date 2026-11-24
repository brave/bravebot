// The one-time crash reports notice in the window: the notice says what a report holds and where to
// turn it off, the window asks the bridge whether it is due and records it as shown, and every
// method it uses is one the main process lets the window call and passes only what it reads.

import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { buildSync } from 'esbuild'
import { loadComponents, loadSync } from './render-support.mjs'

const require = createRequire(import.meta.url)
const React = require('react')
const { renderToStaticMarkup } = require('react-dom/server')

/** The main process's sanitiser, bundled with Electron left out as `sanitise.test.mjs` does. */
function loadSanitiser() {
  const source = buildSync({ entryPoints: ['src/main/sanitise.ts'], bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'] }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
}

test('the notice starts short, with a More toggle, settings and a dismiss', async () => {
  const { CrashReportsNotice } = await loadComponents('src/renderer/components/CrashReportsNotice.tsx')
  const markup = renderToStaticMarkup(React.createElement(CrashReportsNotice, { onSettings() {}, onDismiss() {} }))
  assert.ok(markup.includes('data-test="crash-reports-notice"'))
  assert.ok(markup.includes('sends Brave a report'), markup)
  assert.ok(!markup.includes('operating system'), 'the details wait for More')
  assert.ok(markup.includes('More…'), markup)
  assert.ok(markup.includes('data-test="crash-reports-notice-more"'))
  assert.ok(markup.includes('data-test="crash-reports-notice-settings"'))
  assert.ok(markup.includes('data-test="crash-reports-notice-dismiss"'))
})

test('expanded, the notice says what a report holds and both ways to turn reports off', async () => {
  const { CrashReportsNotice } = await loadComponents('src/renderer/components/CrashReportsNotice.tsx')
  const markup = renderToStaticMarkup(React.createElement(CrashReportsNotice, { onSettings() {}, onDismiss() {}, expanded: true }))
  assert.ok(markup.includes('operating system'), markup)
  assert.ok(markup.includes('never holds your prompts, files or keys'), markup)
  assert.ok(markup.includes('Settings, under General'), markup)
  assert.ok(markup.includes('BRAVEBOT_CRASH_REPORTS=0'), markup)
  assert.ok(markup.includes('Less'), markup)
})

test('the window shows the notice only when the bridge says it is due, and then records it', () => {
  const app = readFileSync('src/renderer/App.tsx', 'utf8')
  const asked = app.indexOf("'crash_reports.notice_due'")
  const marked = app.indexOf("'crash_reports.mark_notice_shown'")
  assert.ok(asked > 0 && marked > asked, 'the notice is asked about and then marked')
  const guard = app.indexOf('answer.due !== true) return', asked)
  const drawn = app.indexOf('setCrashReportsNoticeShown(true)', asked)
  assert.ok(guard > asked && guard < drawn && drawn < marked, 'the notice is drawn and marked only after the answer said it was due')
  assert.match(app, /<Transcript\s+notice=\{startupNotices\}/, 'the startup notices are drawn in the conversation panel')
  assert.match(app, /<>\{usagePingNotice\}\{crashReportsNotice\}<\/>/, 'beside the usage ping notice')
})

test('every method the window uses for crash reports is allowed, and a set passes only the choice', () => {
  const main = readFileSync('src/main/index.ts', 'utf8')
  for (const method of ['crash_reports.get', 'crash_reports.set', 'crash_reports.notice_due', 'crash_reports.mark_notice_shown']) {
    assert.ok(main.includes(`'${method}'`), `${method} is not allowed from the window`)
  }
  const { sanitised } = loadSanitiser()
  assert.deepEqual(
    sanitised('crash_reports.set', { enabled: false, path: '/etc/passwd', directory: '/', session: 's' }),
    { enabled: false },
  )
})

test('the notice and the switch are catalog messages the English window reads', async () => {
  const { t } = loadSync('src/renderer/i18n/index.ts')
  for (const name of [
    'uiCrashReportsNotice', 'uiCrashReportsNoticeShort', 'uiCrashReportsNoticeMore', 'uiCrashReportsNoticeLess',
    'uiCrashReportsNoticeSettings', 'uiCrashReportsNoticeDismiss', 'uiSettingsCrashReports',
    'uiSettingsCrashReportsExplained', 'uiSettingsCrashReportsLockedAdmin', 'uiSettingsCrashReportsLockedIncognito',
    'uiSettingsCrashReportsUnread', 'uiSettingsCrashReportsUnsaved',
  ]) {
    assert.equal(typeof t[name], 'function', `${name} is not a message`)
    assert.ok(t[name]().length > 0, name)
  }
  assert.match(t.uiSettingsCrashReportsLockedEnvironment({ variable: 'BRAVEBOT_CRASH_REPORTS' }), /BRAVEBOT_CRASH_REPORTS/)
})
