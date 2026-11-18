// The one-time usage-ping notice in the window: the notice says what is sent and where to turn it
// off, the window asks the bridge whether it is due and records it as shown, and both methods are
// ones the main process lets the window call.

import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { loadComponents } from './render-support.mjs'

const require = createRequire(import.meta.url)
const React = require('react')
const { renderToStaticMarkup } = require('react-dom/server')

test('the notice starts short, with a More toggle, settings and a dismiss', async () => {
  const { UsagePingNotice } = await loadComponents('src/renderer/components/UsagePingNotice.tsx')
  const markup = renderToStaticMarkup(React.createElement(UsagePingNotice, { onSettings() {}, onDismiss() {} }))
  assert.ok(markup.includes('data-test="usage-ping-notice"'))
  assert.ok(markup.includes('anonymous usage ping'), markup)
  assert.ok(!markup.includes('premium'), 'the details wait for More')
  assert.ok(markup.includes('More…'), markup)
  assert.ok(markup.includes('data-test="usage-ping-notice-more"'))
  assert.ok(markup.includes('data-test="usage-ping-notice-settings"'))
  assert.ok(markup.includes('data-test="usage-ping-notice-dismiss"'))
})

test('expanded, the notice says everything that is sent and where to turn it off', async () => {
  const { UsagePingNotice } = await loadComponents('src/renderer/components/UsagePingNotice.tsx')
  const markup = renderToStaticMarkup(React.createElement(UsagePingNotice, { onSettings() {}, onDismiss() {}, expanded: true }))
  assert.ok(markup.includes('premium'), markup)
  assert.ok(markup.includes('Settings, under General'), markup)
  assert.ok(markup.includes('Less'), markup)
})

test('the window shows the notice only when the bridge says it is due, and then records it', () => {
  const app = readFileSync('src/renderer/App.tsx', 'utf8')
  const asked = app.indexOf("'usage_ping.notice_due'")
  const marked = app.indexOf("'usage_ping.mark_notice_shown'")
  assert.ok(asked > 0 && marked > asked, 'the notice is asked about and then marked')
  assert.match(app.slice(asked, marked), /answer\.due !== true\) return/, 'marked only when it was due')
  assert.match(app, /openSettingsPage\('general'\)/, 'the settings button opens General')
  assert.match(app, /<Transcript\s+notice=\{usagePingNotice\}/, 'the notice is drawn in the conversation panel')
  assert.match(app, /<Sidebar[\s\S]*?notice=\{updateNotice\}/, 'and the sidebar keeps only the update banner')
  const transcript = readFileSync('src/renderer/components/Transcript.tsx', 'utf8')
  assert.match(transcript, /\{notice\}\s*\{backendReady === false && <BackendTray/, 'on the welcome, under its actions')
  assert.match(transcript, /<Composer[\s\S]*?notice=\{notice\}/, 'and docked on the composer in a conversation')
  const main = readFileSync('src/main/index.ts', 'utf8')
  for (const method of ['usage_ping.notice_due', 'usage_ping.mark_notice_shown']) {
    assert.ok(main.includes(`'${method}'`), `${method} is not allowed from the window`)
  }
})
