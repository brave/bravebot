// Shortening a chat's conversation from the desktop's Chat menu (CMD-34).
//
// The request and its endings are the bridge's (crates/ui-bridge/tests/compacting.rs). What the
// window owns is what each ending says and that the chat is drawn as running while it lasts, so
// these drive the plain functions with the answers the bridge sends. The faults they reject are a
// failure shown as a passing toast, the counts dropped from the notice, a chat left running after
// the answer, and a message typed meanwhile sent into a turn that does not exist.
import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const load = (contents) => {
  const source = buildSync({ stdin: { contents, resolveDir: process.cwd(), loader: 'ts' }, bundle: true, write: false, platform: 'node', format: 'cjs', external: ['electron'] }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)((id) => require(id), module, module.exports)
  return module.exports
}

const c = load("export * from './src/renderer/compacting'; export { route } from './src/renderer/interjecting'")

test('a compaction that summarised says how many messages it summarised and how many it kept', () => {
  const notice = c.compactNotice({ status: 'compacted', summarised: 4, kept: 2 })
  assert.equal(notice.toast.kind, 'done')
  assert.equal(notice.toast.title, 'Conversation compacted')
  assert.equal(notice.toast.body, 'Summarised 4 earlier messages, kept the last 2 as they are')
  assert.match(c.compactNotice({ status: 'compacted', summarised: 1, kept: 3 }).toast.body, /^Summarised 1 earlier message,/)
})

test('nothing to shorten and a stop are notes, and neither is a problem', () => {
  const nothing = c.compactNotice({ status: 'nothing' })
  assert.equal(nothing.toast.kind, 'note')
  assert.equal(nothing.toast.title, 'Nothing to compact')
  const stopped = c.compactNotice({ status: 'stopped' })
  assert.equal(stopped.toast.kind, 'note')
  assert.equal(stopped.toast.title, 'Compaction stopped')
  assert.equal(stopped.toast.body, 'The conversation is as it was')
})

test('a failure is a problem that says why, and never a toast', () => {
  const failed = c.compactNotice({ status: 'failed', message: 'the model service refused the request' })
  assert.equal(failed.toast, undefined)
  assert.equal(failed.problem, 'The conversation could not be summarised: the model service refused the request')
})

test('the chat is drawn as running while it compacts, and stops when the answer comes', () => {
  const idle = { running: false, phase: null, hook: null, compacting: false, tokens: 21 }
  const begun = c.compactionBegun(idle)
  assert.equal(begun.running, true, 'so the menu greys what a turn greys and Cancel Turn is offered')
  assert.equal(begun.tokens, 0, 'the row does not count the last turn')
  assert.equal(begun.phase, 'compacting')
  assert.equal(begun.compacting, true)
  const ended = c.compactionEnded({ ...begun, hook: { name: 'compacted' } })
  assert.equal(ended.running, false)
  assert.equal(ended.compacting, false)
  assert.equal(ended.phase, null)
  assert.equal(ended.hook, null)
})

test('a message sent while compacting waits for it instead of going into a turn', () => {
  assert.equal(c.route({ compacting: true }), 'queue')
  assert.equal(c.route({ compacting: false }), 'interject')
})
