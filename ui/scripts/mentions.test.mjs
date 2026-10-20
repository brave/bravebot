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
const { acceptMention } = module.exports

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
