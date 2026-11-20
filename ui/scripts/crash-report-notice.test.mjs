// The one-time crash-report notice in the window: the notice says what a report holds, that it goes
// to Brave and the two ways to turn it off, the window asks the bridge whether it is due and records
// it as shown, and both methods are ones the main process lets the window call.

import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { loadComponents } from './render-support.mjs'

const require = createRequire(import.meta.url)
const React = require('react')
const { renderToStaticMarkup } = require('react-dom/server')

test('the notice starts short, with a More toggle and a dismiss, and no settings button', async () => {
  const { CrashReportNotice } = await loadComponents('src/renderer/components/CrashReportNotice.tsx')
  const markup = renderToStaticMarkup(React.createElement(CrashReportNotice, { onDismiss() {} }))
  assert.ok(markup.includes('data-test="crash-report-notice"'))
  assert.ok(markup.includes('Brave'), markup)
  assert.ok(!markup.includes('BRAVEBOT_CRASH_REPORTS=0'), 'the switches wait for More')
  assert.ok(markup.includes('More…'), markup)
  assert.ok(markup.includes('data-test="crash-report-notice-more"'))
  assert.ok(markup.includes('data-test="crash-report-notice-dismiss"'))
  assert.ok(!markup.includes('crash-report-notice-settings'), 'the desktop has no crash-report switch to open')
})

test('expanded, the notice says what a report holds, where it goes and both ways to turn it off', async () => {
  const { CrashReportNotice } = await loadComponents('src/renderer/components/CrashReportNotice.tsx')
  const markup = renderToStaticMarkup(React.createElement(CrashReportNotice, { onDismiss() {}, expanded: true }))
  for (const text of ['crashReports', 'BRAVEBOT_CRASH_REPORTS=0', 'Brave', 'prompts', 'Less']) {
    assert.ok(markup.includes(text), `${text} is missing from ${markup}`)
  }
})

test('the window shows the notice only when the bridge says it is due, and then records it', () => {
  const app = readFileSync('src/renderer/App.tsx', 'utf8')
  const asked = app.indexOf("'crash_reports.notice_due'")
  const marked = app.indexOf("'crash_reports.mark_notice_shown'")
  assert.ok(asked > 0 && marked > asked, 'the notice is asked about and then marked')
  assert.match(app.slice(asked, marked), /answer\.due !== true\) return/, 'marked only when it was due')
  assert.match(app, /const startupNotices = useMemo\([\s\S]*?\{crashReportNotice\}[\s\S]*?\[usagePingNotice, crashReportNotice\]/,
    'the crash-report notice shares the startup notice slot')
  assert.match(app, /<Transcript\s+notice=\{startupNotices\}/, 'and is drawn in the conversation panel')
  const main = readFileSync('src/main/index.ts', 'utf8')
  for (const method of ['crash_reports.notice_due', 'crash_reports.mark_notice_shown']) {
    assert.ok(main.includes(`'${method}'`), `${method} is not allowed from the window`)
  }
})
