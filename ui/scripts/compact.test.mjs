import test from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { buildSync } from 'esbuild'
const require = createRequire(import.meta.url)
function load(path) {
  const source = buildSync({ entryPoints: [path], bundle: true, write: false, platform: 'node', format: 'cjs' }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
}
const { apply } = load('src/renderer/apply.ts')
const { canCompact } = load('src/renderer/can-send.ts')
const t = load('src/renderer/transcript.ts')
const { NOTHING_OPEN, isEnabled, parseWindowState, command } = load('src/shared/commands.ts')
const { holding, compactedBot } = load('src/main/holding.ts')

const point = { steps: 1, turn: 1, prompt: 0, text: 'first', paths: [], gaps: [] }
const live = (more = {}) => ({
  handle: 's1', running: true, phase: 'compacting', archived: 0, turns: {}, queuePaused: false,
  rewind: [point], entries: [t.userSaid('first'), t.replied('one', 1)], bot: null, askingTrust: null, ...more,
})

/** The session after `event`, and whether the list was asked to refresh. */
function fold(before, event, data) {
  let after = before
  let refreshed = false
  apply({ event, session: 's1', data }, (update) => { after = update(after) }, () => {}, () => { refreshed = true })
  return { after, refreshed }
}

test('a compaction that summarised ends the wait, drops the rewind points and says what it did', () => {
  const { after, refreshed } = fold(live(), 'compact.done',
    { compacted: true, summarised: 14, kept: 6, tokens: 900, wonBack: 40000, archived: 1, rewind: [] })
  assert.equal(after.running, false)
  assert.equal(after.phase, null)
  assert.equal(after.archived, 1)
  assert.deepEqual(after.rewind, [])
  assert.equal(after.queuePaused, false, 'a compaction does not hold the queue')
  assert.equal(after.entries.at(-1).text, 'Summarised 14 earlier messages; 6 kept word for word.')
  assert.ok(refreshed, 'the record was rewritten, so the list is read again')
})

test('a bot whose briefing was summarised away is no longer grounded, and stays running to consolidate', () => {
  const bot = { slug: 'helper', grounded: true }
  const { after } = fold(live({ bot }), 'compact.done',
    { compacted: true, summarised: 4, kept: 2, tokens: 1, wonBack: 1, archived: 1, rewind: [], consolidating: true })
  assert.equal(after.bot.grounded, false)
  assert.equal(after.running, true)
})

test('nothing old enough leaves the session as it was and says so', () => {
  const { after } = fold(live(), 'compact.done', { compacted: false, summarised: 0, kept: 0, tokens: 0, wonBack: 0 })
  assert.equal(after.running, false)
  assert.deepEqual(after.rewind, [point], 'the points still describe the conversation')
  assert.equal(after.entries.at(-1).text, 'Nothing old enough to summarise yet.')
})

test('a failed or stopped compaction says the conversation is unchanged', () => {
  const failed = fold(live(), 'compact.error', { category: 'transport', stopped: false }).after
  assert.equal(failed.running, false)
  assert.equal(failed.entries.at(-1).text, 'Compaction failed: The model service could not be reached. The conversation is as it was.')
  assert.deepEqual(failed.rewind, [point])
  const stopped = fold(live(), 'compact.error', { category: 'cancelled', stopped: true }).after
  assert.equal(stopped.entries.at(-1).text, 'Compaction stopped. The conversation is as it was.')
})

test('Compact Conversation is offered only on an idle session somebody has spoken in', () => {
  const idle = live({ running: false })
  assert.equal(canCompact(idle), true)
  assert.equal(canCompact({ ...idle, running: true }), false)
  assert.equal(canCompact({ ...idle, askingTrust: '/project' }), false)
  assert.equal(canCompact({ ...idle, entries: [] }), false)
  assert.equal(canCompact(null), false)

  const item = command('turn.compact')
  assert.equal(isEnabled(item.requires, NOTHING_OPEN), false)
  const state = { ...NOTHING_OPEN, hasSession: true, canCompact: true }
  assert.equal(isEnabled(item.requires, state), true)
  assert.equal(parseWindowState({ ...state, canCompact: undefined }), null, 'a state that leaves it out enables nothing')
  assert.equal(parseWindowState(state).canCompact, true)
})

test('a compaction holds its session from the compacting phase until it ends, either way', () => {
  assert.deepEqual(holding({ event: 'phase', session: 's1', data: { phase: 'compacting' } }), { session: 's1', begins: true })
  assert.equal(holding({ event: 'phase', session: 's1', data: { phase: 'thinking' } }), null)
  assert.deepEqual(holding({ event: 'compact.done', session: 's1', data: { compacted: false } }), { session: 's1', begins: false })
  assert.deepEqual(holding({ event: 'compact.error', session: 's1', data: { category: 'cancelled', stopped: true } }), { session: 's1', begins: false })
  assert.deepEqual(holding({ event: 'turn.started', session: 's1', data: {} }), { session: 's1', begins: true })
  assert.equal(holding({ event: 'compact.done', data: { compacted: true } }), null, 'an event about no session holds nothing')
})

test('a bot whose archive a compaction raised is owed a consolidation, and only then', () => {
  const done = (data) => ({ event: 'compact.done', session: 's1', data: { summarised: 4, kept: 2, tokens: 1, wonBack: 1, ...data } })
  const helper = (archived) => (handle) => (handle === 's1' ? { slug: 'helper', archived } : null)

  assert.deepEqual(compactedBot(done({ compacted: true, archived: 2 }), helper(1)),
    { handle: 's1', slug: 'helper', archived: 2, consolidate: true })
  assert.deepEqual(compactedBot(done({ compacted: true, archived: 1 }), helper(1)),
    { handle: 's1', slug: 'helper', archived: 1, consolidate: false }, 'the archive is still noted, with nothing owed')
  assert.equal(compactedBot(done({ compacted: true, archived: 2 }), () => null), null, 'a session with no bot behind it')
  assert.equal(compactedBot(done({ compacted: false }), helper(0)), null, 'nothing was old enough')
  assert.equal(compactedBot({ event: 'turn.done', session: 's1', data: { archived: 2 } }, helper(1)), null, 'a turn is answered elsewhere')
})
