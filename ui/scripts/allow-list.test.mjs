// What the window may ask the agent for.
//
// `src/main/index.ts` lists the methods in one `const ALLOWED = new Set([...])` literal. These tests
// hold the names in it to the rules `drive-menu.mjs` also asserts, on every pull request and with no
// Electron. The rule on `approve` and `decide` is a naming check: it stops a method whose name says
// it approves or decides from being added without a person noticing. The `*.reply` methods are on the
// list because they carry the person's answers to the agent's questions through the window, and
// `session-directory.test.mjs` classifies each of them. How many names there are is not asserted:
// `session-directory.test.mjs` fails for a name added to the literal and not classified there, and
// for one removed, so a person who changes the list is told by that test.
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { allowedMethods } from './allow-list.mjs'

function allowed() {
  const names = allowedMethods(readFileSync('src/main/index.ts', 'utf8'))
  assert.ok(names, 'the allow-list is a literal set the tests can read')
  return names
}

test('no method the window may send approves or decides, whatever its capitalisation', () => {
  assert.deepEqual(allowed().filter((name) => /approve|decide/i.test(name)), [])
})

test('model discovery is on the allow-list', () => {
  assert.ok(allowed().includes('models.list'), 'the model picker asks for models.list')
})

test('no method is listed twice', () => {
  const names = allowed()
  assert.deepEqual(names.filter((name, at) => names.indexOf(name) !== at), [], 'a set keeps one of two equal names, so a repeat is a line that does nothing')
})

test('a comment inside the literal does not change which names are read', () => {
  const source = [
    'const ALLOWED = new Set([',
    "  'a.one', // not 'decide.me', a, b, c",
    "  // the window's reach, 'b.two' and 'c.three' included",
    "  'd.four',",
    '])',
  ].join('\n')
  assert.deepEqual(allowedMethods(source), ['a.one', 'd.four'])
})
