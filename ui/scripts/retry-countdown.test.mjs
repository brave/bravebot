// A request waiting to be sent again says when, as the desktop window draws it (VIEW-43).
//
// The bridge sends spans from the moment of each `phase` event (crates/ui-bridge/tests/reporting.rs),
// because the two ends share no clock. What the window owns is turning them into times of its own,
// counting them down, and taking the countdown away when the wait is over, so these drive the real
// event reducer and the real word function. The faults they reject are a countdown that is never
// counted down, one that reads `0s` while an attempt is still to come, one that outlives its wait,
// and one that takes the place of a hook or a check the turn is held up on.
import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const load = (contents) => {
  const source = buildSync({ stdin: { contents, resolveDir: process.cwd(), loader: 'ts' }, bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'] }).outputFiles[0].text
  const module = { exports: {} }
  const electron = { app: { getPath: () => '/nonexistent', getAppPath: () => process.cwd(), isPackaged: false } }
  new Function('require', 'module', 'exports', source)((id) => (id === 'electron' ? electron : require(id)), module, module.exports)
  return module.exports
}

const { apply } = load("export { apply } from './src/renderer/apply'")
const { workingWord } = load("export { workingWord } from './src/renderer/components/Transcript'")
const { retryDue, reconnectingWords, timeLeft } = load("export * from './src/renderer/retry'")

const session = () => {
  const held = {
    live: { handle: 's-1', summary: { title: '', project: '', branch: null, directory: '/', id: 'a' }, entries: [], turns: {}, todos: [], quarantine: [], phase: null, checking: null, hook: null, composing: null, thinking: '', tokens: 0, running: false, archived: 0, awaitingOrdinal: null, bot: null, agents: [] },
  }
  held.send = (event, data) => apply({ event, data }, (update) => { held.live = update(held.live) }, () => {}, () => {})
  held.word = (now) => workingWord(held.live.phase, held.live.checking, held.live.composing, held.live.hook, held.live.retry ?? null, now)
  return held
}

test('a wait names the time to the attempt and to giving up, counted down from the event', () => {
  const due = retryDue({ attempt: 4, in_ms: 38_000, gives_up_in_ms: 3_130_000 }, 1_000_000)
  const say = (after) => reconnectingWords(due, 1_000_000 + after)

  assert.equal(say(0), 'Reconnecting, next try in 38s, giving up in 52m 10s')
  assert.equal(say(8_000), 'Reconnecting, next try in 30s, giving up in 52m 02s')
  // Rounded up, so a time reaches the second it names rather than reading a second early.
  assert.equal(say(500), 'Reconnecting, next try in 38s, giving up in 52m 10s')
  assert.equal(say(37_500), 'Reconnecting, next try in 1s, giving up in 51m 33s')
})

test('a wait that a count decides names no time to give up, and a spent pause names nothing', () => {
  const due = retryDue({ attempt: 2, in_ms: 2_000, gives_up_in_ms: null }, 0)

  assert.equal(reconnectingWords(due, 0), 'Reconnecting, next try in 2s')
  assert.equal(reconnectingWords(due, 2_000), null, 'the attempt is out and the plain word is the true one')
  assert.equal(reconnectingWords(due, 60_000), null)
  assert.equal(retryDue(null, 0), null)
  assert.equal(retryDue(undefined, 0), null)
  assert.equal(timeLeft(0), '0s')
  assert.equal(timeLeft(59_001), '1m 00s')
})

test('the reducer counts a wait from the moment the event arrives, and the next phase takes it away', () => {
  const held = session()
  const before = Date.now()
  held.send('phase', { phase: 'reconnecting', retry: { attempt: 4, in_ms: 38_000, gives_up_in_ms: 3_130_000 } })
  const after = Date.now()

  const due = held.live.retry
  assert.ok(due.nextAt >= before + 38_000 && due.nextAt <= after + 38_000, `counted from another moment: ${due.nextAt}`)
  assert.ok(due.givesUpAt >= before + 3_130_000 && due.givesUpAt <= after + 3_130_000)
  assert.equal(held.word(due.nextAt - 38_000), 'Reconnecting, next try in 38s, giving up in 52m 10s')

  // The attempt is out: the same phase with nothing to count down to reads as the bare word.
  held.send('phase', { phase: 'reconnecting', retry: null })
  assert.equal(held.live.retry, null)
  assert.equal(held.word(due.nextAt), 'Reconnecting')

  // And a later phase never inherits the earlier wait.
  held.send('phase', { phase: 'reconnecting', retry: { attempt: 2, in_ms: 1_000, gives_up_in_ms: null } })
  held.send('phase', { phase: 'thinking' })
  assert.equal(held.live.retry, null)
  assert.equal(held.word(Date.now()), 'Thinking')
})

test('a hook or a check the turn is held up on keeps the word, whatever the request is waiting on', () => {
  const held = session()
  held.send('phase', { phase: 'reconnecting', retry: { attempt: 2, in_ms: 30_000, gives_up_in_ms: null } })
  const now = held.live.retry.nextAt - 30_000

  held.send('hook.started', { moment: 'tool-finished', program: 'cargo' })
  assert.equal(held.word(now), 'Running hook: cargo (tool-finished)')
  held.send('hook.finished', {})
  held.send('check.started', { lines: 3 })
  assert.equal(held.word(now), 'Checking 3 lines')
  held.send('check.finished', {})
  assert.equal(held.word(now), 'Reconnecting, next try in 30s')
})
