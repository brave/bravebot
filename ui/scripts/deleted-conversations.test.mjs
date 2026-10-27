// What the main process forgets after the agent deletes conversations.
//
// The app keeps a conversation's preferences, its fork lineage and its bot's link beside the agent's
// record. A delete or a purge that went through removes those too, for exactly the records the
// agent says it removed among the ones the window named. The faults rejected here are forgetting a
// record the agent kept, and forgetting one the window never named because the answer listed it.
import test from 'node:test'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { buildSync } from 'esbuild'
import { readFileSync } from 'node:fs'

const require = createRequire(import.meta.url)

function load(path) {
  const source = buildSync({ entryPoints: [path], bundle: true, write: false, platform: 'node', format: 'cjs' }).outputFiles[0].text
  const module = { exports: {} }
  new Function('require', 'module', 'exports', source)(require, module, module.exports)
  return module.exports
}

const { deletedConversations } = load('src/main/deleted.ts')
const DIRECTORY = '/work/project'

test('a purge forgets each record it deleted that the window named, and no other', () => {
  const params = { directory: DIRECTORY, ids: ['a', 'b', 'c', 'held'] }
  const ok = { deleted: ['a', 'c', 'unnamed', 'a', '../escape', 7], failed: ['b'], held: ['held'] }
  assert.deepEqual(deletedConversations('session.purge', params, ok), [
    { directory: DIRECTORY, id: 'a' },
    { directory: DIRECTORY, id: 'c' },
  ])
})

test('a purge that was refused, a dry run, or one with no names forgets nothing', () => {
  assert.deepEqual(deletedConversations('session.purge', { directory: DIRECTORY, ids: ['a'] }, null), [])
  assert.deepEqual(deletedConversations('session.purge', { directory: DIRECTORY, dryRun: true }, { delete: [{ id: 'a' }], held: [] }), [])
  assert.deepEqual(deletedConversations('session.purge', { directory: DIRECTORY }, { deleted: ['a'] }), [])
  assert.deepEqual(deletedConversations('session.purge', { ids: ['a'] }, { deleted: ['a'] }), [])
})

test('a delete forgets the one record the window named, once the agent confirms it', () => {
  assert.deepEqual(deletedConversations('session.delete', { directory: DIRECTORY, id: 'a' }, { deleted: true }), [{ directory: DIRECTORY, id: 'a' }])
  assert.deepEqual(deletedConversations('session.delete', { directory: DIRECTORY, id: 'a' }, { deleted: false }), [])
  assert.deepEqual(deletedConversations('session.delete', { directory: DIRECTORY, id: 'a/b' }, { deleted: true }), [])
  assert.deepEqual(deletedConversations('session.import', { directory: DIRECTORY, ids: ['a'] }, { copied: ['a'], deleted: ['a'] }), [])
})

test('the request handler forgets the preferences, the fork and the bot link of each, after the answer', () => {
  const index = readFileSync('src/main/index.ts', 'utf8')
  const loop = /for \(const \{ directory, id \} of deletedConversations\(method, params, ok\)\) \{\s*removeConversation\(conversationKey\(directory, id\)\)\s*forgetFork\(\{ directory, id \}\)\s*forgetBotConversation\(directory, id\)\s*\}/.exec(index)
  assert.ok(loop, 'each deleted conversation is forgotten in all three places')
  assert.ok(index.lastIndexOf('bridge.request(method', loop.index) > 0, 'and only once the agent has answered')
})
