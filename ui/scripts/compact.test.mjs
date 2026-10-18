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
