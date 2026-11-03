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
const t = load('src/renderer/transcript.ts')

// Every call reads the same file, so only the id can tell them apart.
const call = (id, note = null) => ({
  call: id, verb: 'Read', target: 'a.rs', shownTarget: 'a.rs', shownFile: 'a.rs', why: '', note, failed: false, untrusted: false, changes: [], waitedSeconds: null,
})
const notes = (entries) => entries.filter((entry) => entry.kind === 'tool').map((entry) => [entry.activity.call, entry.activity.note])

test('a finished call replaces the row of the call with its id, in whichever order overlapping calls finish', () => {
  for (const order of [[1, 2], [2, 1]]) {
    let entries = [t.started(call(1)), t.started(call(2))]
    const ids = entries.map((entry) => entry.id)
    entries = t.finish(entries, call(order[0], `first ${order[0]}`))
    entries = t.finish(entries, call(order[1], `second ${order[1]}`))
    assert.deepEqual(entries.map((entry) => entry.id), ids, 'each finish keeps its own row and position')
    const said = { [order[0]]: `first ${order[0]}`, [order[1]]: `second ${order[1]}` }
    assert.deepEqual(notes(entries), [[1, said[1]], [2, said[2]]], 'each row holds the finish of the call it started')
  }
})

test('a finish for no running call with its id is a row of its own, and leaves the running call alone', () => {
  let entries = [t.started(call(1))]
  entries = t.finish(entries, call(9, 'unknown'))
  entries = t.finish(entries, call(null, 'unnumbered'))
  assert.deepEqual(notes(entries), [[1, null], [9, 'unknown'], [null, 'unnumbered']])
  entries = t.finish(entries, call(1, 'done'))
  assert.deepEqual(notes(entries)[0], [1, 'done'])
  entries = t.finish(entries, call(1, 'again'))
  assert.equal(notes(entries).length, 4, 'a call already finished is not finished twice')
})
