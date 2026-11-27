// The row beside the composer that says a session's loops are live, and the loop rows of the
// session status (docs/specs/loop.md, LOOP-13).
//
// The row reads `loops.list` and counts down between reads. The faults these reject are a row drawn
// for a session with no loops, a countdown left out while a moment is ahead, a countdown that sits at
// `0s` once the moment has passed, several loops counted as one, a line `/after` holds counted as a
// loop, the row not mounted on the composer or not redrawn each second, and a status that leaves the
// loops out.
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { loadComponents, loadSync } from './render-support.mjs'

const require = createRequire(import.meta.url)
const React = require('react')
const { renderToStaticMarkup } = require('react-dom/server')

const shared = loadSync('src/shared/loop.ts')
const { statusNotice } = loadSync('src/renderer/reports.ts')

const row = (number, until, overrides = {}) => ({
  number, prompt: `line ${number}`, pace: 'every 5m', when: 'next in 5m', ticks: 1, quiet: 0, once: false, until, ...overrides,
})
const list = (...loops) => ({ loops, soonest: null })

test('the time left is spelled in its two largest units, as the terminal spells it', () => {
  assert.equal(shared.spell(0), '0s')
  assert.equal(shared.spell(9), '9s')
  assert.equal(shared.spell(252), '4m 12s')
  assert.equal(shared.spell(3_900), '1h 5m')
  assert.equal(shared.spell(90_061), '1d 1h')
})

test('a session with no loops says nothing', async () => {
  assert.equal(shared.loopStatusWords(list(), 0), null)
  const { LoopStatusRow } = await loadComponents('src/renderer/components/LoopStatus.tsx')
  assert.equal(renderToStaticMarkup(React.createElement(LoopStatusRow, { list: list(), elapsed: 0 })), '')
})

test('one loop counts down to its next tick from when the list was read', async () => {
  const read = list(row(1, 252))
  assert.equal(shared.loopStatusWords(read, 0), 'Loop live · next in 4m 12s')
  assert.equal(shared.loopStatusWords(read, 1), 'Loop live · next in 4m 11s')
  assert.equal(shared.loopStatusWords(read, 251), 'Loop live · next in 1s')
  const { LoopStatusRow } = await loadComponents('src/renderer/components/LoopStatus.tsx')
  const markup = renderToStaticMarkup(React.createElement(LoopStatusRow, { list: read, elapsed: 12 }))
  assert.match(markup, /data-test="loop-status"/)
  assert.match(markup, /Loop live · next in 4m/)
  assert.doesNotMatch(markup, /aria-live|role="status"/, 'a row that changes every second is not read out each time')
})

test('once the moment has passed, or none is set, the row says only that the loop is live', () => {
  assert.equal(shared.loopStatusWords(list(row(1, 3)), 3), 'Loop live')
  assert.equal(shared.loopStatusWords(list(row(1, 3)), 40), 'Loop live')
  // A tick in flight, or a self-paced loop not yet told when.
  assert.equal(shared.loopStatusWords(list(row(1, null)), 0), 'Loop live')
})

test('several loops are counted, with the soonest of their moments', () => {
  assert.equal(shared.loopStatusWords(list(row(1, 600), row(2, 12), row(3, null)), 0), '3 loops live · next in 12s')
  assert.equal(shared.loopStatusWords(list(row(1, 600), row(2, 12)), 5), '2 loops live · next in 7s')
  // The soonest moment is gone, so its tick waits for the session: no countdown, as in the terminal.
  assert.equal(shared.loopStatusWords(list(row(1, 600), row(2, 12)), 12), '2 loops live')
  assert.equal(shared.loopStatusWords(list(row(1, null), row(2, null)), 0), '2 loops live')
})

test('a line /after holds is named only where no loop is live, and is not counted as one', () => {
  const later = (number, until) => row(number, until, { once: true, pace: 'once' })
  assert.equal(shared.loopStatusWords(list(later(1, 90)), 0), 'Sending in 1m 30s')
  assert.equal(shared.loopStatusWords(list(later(1, 90)), 90), 'Sending later')
  assert.equal(shared.loopStatusWords(list(later(1, 5), row(2, 300)), 0), 'Loop live · next in 5m')
})

test('a line /after holds says what it waits for, and several lead with their count', () => {
  const later = (number, until) => row(number, until, { once: true, pace: 'once' })
  // A moment still ahead is counted down, busy or not.
  assert.equal(shared.loopStatusWords(list(later(1, 90)), 0, true), 'Sending in 1m 30s')
  // Past its moment while a turn runs or lines are queued, it waits for that.
  assert.equal(shared.loopStatusWords(list(later(1, 5)), 10, true), 'Sending after this turn')
  assert.equal(shared.loopStatusWords(list(later(1, 5), later(2, 8)), 10, true), '2 lines · sending after this turn')
  assert.equal(shared.loopStatusWords(list(later(1, 50), later(2, 80)), 10, false), '2 lines · sending in 40s')
})

test('the row is mounted on the composer and polls the list and redraws every second', () => {
  const component = readFileSync('src/renderer/components/LoopStatus.tsx', 'utf8')
  assert.match(component, /request<LoopList>\('loops\.list', \{ session \}\)/)
  assert.match(component, /setInterval\(\(\) => setNow\(Date\.now\(\)\), 1000\)/)
  // Read again at once on every loop event and on every answer to /loop or /after.
  assert.match(component, /message\.session === session && message\.event\.startsWith\('loop\.'\)\) void ask\(\)/)
  assert.match(component, /window\.addEventListener\(LOOPS_CHANGED, answered\)/)
  // Every figure in the row is tabular, the text's own font shorthand included.
  assert.match(readFileSync('src/renderer/styles/composer.css', 'utf8'), /\.loop-status \.notice-text \{[^}]*font-variant-numeric: tabular-nums/)
  const transcript = readFileSync('src/renderer/components/Transcript.tsx', 'utf8')
  assert.match(transcript, /<LoopStatus session=\{live\.handle\} busy=\{loopsBusy\} \/>/)
  assert.match(transcript, /const loopsBusy = Boolean\(live\?\.running\) \|\| queued\.length > 0/)
  assert.match(transcript, /loops=\{loopRow\}/)
  assert.match(readFileSync('src/renderer/components/Composer.tsx', 'utf8'), /\{notice\}\s*\{loops\}/)
})

const report = {
  project: '/work/app', folders: ['/work/app'], model: null, definition: null, permissionMode: 'default',
  sandboxMode: 'workspace-write', sessionId: 'abc', title: 't', turns: 1, tokens: 10,
  rulesHeldByTurn: false, rules: { paths: [], commands: [] },
}

test('the session status lists each loop under its number, and nothing about loops where there are none', () => {
  assert.doesNotMatch(statusNotice(report).body, /Loop/)
  assert.doesNotMatch(statusNotice(report, []).body, /Loop/)
  const notice = statusNotice(report, [row(1, 252, { prompt: 'check the deploy', when: 'next in 4m 12s' }), row(3, null, { prompt: 'tidy', pace: 'each turn sets the pace', when: 'when this turn ends' }), row(4, 60, { prompt: 'say hi', once: true, pace: 'once', when: 'in 1m' })])
  const body = notice.body
  // The times first and the line last, so a long line is what the row cuts.
  assert.match(body, /\nLoop 1 +every 5m · next in 4m 12s · check the deploy\n/)
  assert.match(body, /\nLoop 3 +each turn sets the pace · when this turn ends · tidy\n/)
  assert.match(body, /\nLoop 4 +sent once · in 1m · say hi\n/)
  // Drawn as label and value pairs, the line apart from the times.
  assert.deepEqual(notice.rows.find((pair) => pair.label === 'Loop 1'), { label: 'Loop 1', value: 'every 5m · next in 4m 12s', rest: 'check the deploy' })
  assert.deepEqual(notice.rows.find((pair) => pair.label === 'Session'), { label: 'Session', value: 't' })
  // Mid-turn as well, where the rules are held.
  assert.match(statusNotice({ ...report, rulesHeldByTurn: true, rules: null }, [row(2, 5)]).body, /\nLoop 2 +every 5m · next in 5m · line 2\n/)
  assert.match(readFileSync('src/renderer/App.tsx', 'utf8'), /statusNotice\(report, loops\.loops\)/)
})
