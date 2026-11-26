// The one edit the composer makes to its draft when a name is chosen (NAME-9, NAME-8).
//
// The word being replaced and the way a name is written come from the bridge, which takes them from
// the code the terminal uses. The faults these reject are the window working the word out itself
// (a `\S+$` stops at the space inside `@My\ Doc` and leaves the line unchanged) and writing the
// path unescaped (`read @My Documents/` names `My`, a different path).
import test from 'node:test'
import assert from 'node:assert/strict'
import { buildSync } from 'esbuild'

const source = buildSync({ entryPoints: ['src/shared/mentions.ts'], bundle: true, write: false, platform: 'node', format: 'cjs' }).outputFiles[0].text
const module = { exports: {} }
new Function('module', 'exports', source)(module, module.exports)
const { acceptMention, definitionOf, offerOf } = module.exports

const folder = { path: 'My Documents/', written: 'My\\ Documents/', directory: true }

test('a directory with a space is written escaped, so the word stays one word', () => {
  assert.equal(acceptMention('read @My', folder, '@My'), 'read @My\\ Documents/')
})

test('a half-typed name that already holds an escaped space is completed over the whole word', () => {
  assert.equal(acceptMention('read @My\\ Doc', folder, '@My\\ Doc'), 'read @My\\ Documents/')
})

test('a file gets a trailing space and a directory does not', () => {
  const file = { path: 'My Documents/notes.md', written: 'My\\ Documents/notes.md', directory: false }
  assert.equal(acceptMention('read @My\\ Documents/no', file, '@My\\ Documents/no'), 'read @My\\ Documents/notes.md ')
})

test('a draft whose last word is no name is left as it is', () => {
  assert.equal(acceptMention('read My', folder, 'My'), 'read My')
  assert.equal(acceptMention('read @My', folder, null), 'read @My')
  // An offer that is out of date for the draft changes nothing rather than splicing at the wrong place.
  assert.equal(acceptMention('read @Other', folder, '@My'), 'read @Other')
})

// ADDRESS-2: the composer asks for definition rows, and a definition row is staged to answer the
// message instead of being written as an `@` name. The faults these reject are the offer dropping
// a row's kind or description on its way to the list, and a definition row read as a file.
const definitionRow = { path: 'lantern', shownPath: 'lantern', written: 'lantern', directory: false, kind: 'definition', description: 'Keeps the lights.' }
const fileRow = { path: 'lantern.md', shownPath: 'lantern.md', written: 'lantern.md', directory: false, kind: 'file', description: null }

test('a definition row reaches the composer with its kind and description, ahead of the files', () => {
  const offer = offerOf({ typed: 'lan', word: '@lan', entries: [definitionRow, fileRow], completes: true })
  assert.deepEqual(offer.entries.map((row) => [row.path, row.kind, row.description]), [
    ['lantern', 'definition', 'Keeps the lights.'],
    ['lantern.md', 'file', null],
  ])
  assert.equal(offer.completes, true)
})

test('an offer out of shape offers nothing', () => {
  assert.deepEqual(offerOf(undefined), { typed: null, word: null, entries: [], completes: false })
  assert.deepEqual(offerOf({ typed: 'lan' }), { typed: null, word: null, entries: [], completes: false })
})

test('a definition row is addressed when picked, and a file row is written as a name', () => {
  assert.deepEqual(definitionOf(definitionRow), { name: 'lantern', description: 'Keeps the lights.' })
  assert.equal(definitionOf(fileRow), null)
  assert.equal(definitionOf({ ...fileRow, kind: 'directory', directory: true }), null)
})
