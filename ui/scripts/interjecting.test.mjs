// A line typed into a turn that is running (INPUT-10), as the desktop window handles it.
//
// The agent decides when the planner reads a line and says so with `turn.interjected`
// (crates/ui-bridge/tests/interjecting.rs). What the window owns is what the person sees while
// they wait and what becomes of a line the turn never read, so these drive the real event reducer
// with the events the bridge sends. The faults they reject are a line drawn in the transcript
// before the planner has it, a line lost when the turn ends first, and one that waits behind the
// pause a Stop puts on the queue although the person never asked to hold it.
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

const lines = load("export * from './src/renderer/interjecting'")
const events = load("export { apply } from './src/renderer/apply'")
const main = load("export * from './src/main/sanitise'")

const line = (id, prompt) => ({ id, prompt, attachments: [], staged: [] })
const live = (over = {}) => ({
  entries: [{ kind: 'user', id: 'u0', text: 'opening prompt' }],
  turns: {}, queued: [], interjected: [], running: true, queuePaused: false,
  summary: { id: null, title: 't', project: '/p', branch: null, directory: '/p' },
  archived: 0, awaitingOrdinal: null, staging: { made: 0, staged: [] },
  ...over,
})
/** Fold `message` into `before` the way the window does. */
const heard = (before, message) => {
  let after = before
  events.apply(message, (action) => { after = typeof action === 'function' ? action(after) : action }, () => {}, () => {})
  return after
}
const done = { event: 'turn.done', session: 's', data: { reply: 'ok', turn: 1, prompt: 0, archived: 0, contextTokens: 1 } }
const failed = (kind) => ({ event: 'turn.error', session: 's', data: { kind, message: 'm', turn: 1, prompt: 0, archived: 0, contextTokens: 1 } })
const said = (before) => before.entries.filter((entry) => entry.kind === 'user').map((entry) => entry.text)

test('a line sent into the turn waits, undrawn in the transcript, until the planner has it', () => {
  const sent = lines.sentInto(live(), line('a', 'no, the other one'))
  assert.deepEqual(sent.interjected.map((item) => item.prompt), ['no, the other one'])
  assert.deepEqual(said(sent), ['opening prompt'])
  const read = heard(sent, { event: 'turn.interjected', session: 's', data: { prompt: 'no, the other one' } })
  assert.deepEqual(read.interjected, [])
  assert.deepEqual(said(read), ['opening prompt', 'no, the other one'])
})

test('lines are read in the order they were sent, one event each', () => {
  let state = lines.sentInto(lines.sentInto(live(), line('a', 'first')), line('b', 'second'))
  state = heard(state, { event: 'turn.interjected', session: 's', data: { prompt: 'first' } })
  assert.deepEqual(state.interjected.map((item) => item.id), ['b'])
  state = heard(state, { event: 'turn.interjected', session: 's', data: { prompt: 'second' } })
  assert.deepEqual(said(state), ['opening prompt', 'first', 'second'])
})

test('a line the turn never read goes on as a turn of its own when the turn finishes, even under a paused queue', () => {
  const state = heard(lines.sentInto(live({ queuePaused: true, queued: [{ prompt: 'held on purpose', attachments: [], staged: [] }] }), line('a', 'unread')), done)
  assert.deepEqual(state.interjected, [])
  assert.deepEqual(state.queued.map((item) => item.prompt), ['unread', 'held on purpose'])
  assert.equal(lines.mayGo(state), true, 'the unread line waited behind a pause nobody asked for')
  assert.deepEqual(said(state), ['opening prompt'], 'an unread line was drawn as said')
})

test('what a person stopped goes on, and what a failed turn left waits for them', () => {
  const stopped = heard(lines.sentInto(live(), line('a', 'unread')), failed('cancelled'))
  assert.equal(stopped.queuePaused, true, 'a stop pauses the queue')
  assert.equal(lines.mayGo(stopped), true)
  const broken = heard(lines.sentInto(live(), line('a', 'unread')), failed('backend'))
  assert.deepEqual(broken.queued.map((item) => item.prompt), ['unread'])
  assert.equal(lines.mayGo(broken), false, 'a failed turn sent the line on without being asked')
})

test('a turn that would not take the line gives it back at the head of the queue', () => {
  const state = lines.declined(lines.sentInto(live({ queued: [{ prompt: 'later', attachments: [], staged: [] }] }), line('a', 'refused')), 'a')
  assert.deepEqual(state.interjected, [])
  assert.deepEqual(state.queued.map((item) => item.prompt), ['refused', 'later'])
  assert.equal(lines.declined(state, 'a'), state, 'declining a line twice changes nothing')
})

test('a paused queue holds what a person queued', () => {
  assert.equal(lines.mayGo({ queuePaused: true, queued: [{ prompt: 'x', attachments: [], staged: [] }] }), false)
  assert.equal(lines.mayGo({ queuePaused: false, queued: [{ prompt: 'x', attachments: [], staged: [] }] }), true)
  assert.equal(lines.mayGo({ queuePaused: false, queued: [] }), false)
})

test('the window names each marker and writes no words of its own', () => {
  const staged = [
    { marker: '[File #1]', via: 'drop', file: { id: 'a', name: 'notes.md', kind: 'text', noun: 'File' } },
    { marker: '[Image #2]', via: 'paste', picture: { id: 'p', noun: 'Image' } },
  ]
  assert.deepEqual(lines.markersOf(staged), { files: [{ marker: '[File #1]', name: 'notes.md' }], pictures: ['[Image #2]'] })
  assert.equal(lines.inWords, undefined, 'the transform belongs to the bridge (SHARE-001)')
})

test('a file chosen with the paperclip and the memory update queue the line instead of sending it', () => {
  assert.equal(lines.route({}), 'interject')
  assert.equal(lines.route({ attachments: [] }), 'interject')
  assert.equal(lines.route({ attachments: [{ path: '/p/a.png' }] }), 'queue')
  assert.equal(lines.route({ consolidating: true }), 'queue')
})

test('only the words of a line and the names of its markers reach the bridge, whatever else the window sent', () => {
  const forwarded = main.sanitised('turn.interject', {
    session: 's1', prompt: 'hello', target: 4,
    files: [{ marker: '[File #1]', name: 'a.md', path: '/etc/passwd' }], pictures: ['[Image #2]'],
    dropped: ['/etc/passwd'], images: ['AAAA'], definition: 'x', recall: false,
  })
  assert.deepEqual(forwarded, {
    session: 's1', target: 4, prompt: 'hello',
    files: [{ marker: '[File #1]', name: 'a.md' }], pictures: ['[Image #2]'],
  })
})
