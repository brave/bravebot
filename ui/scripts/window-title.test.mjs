// The desktop window's title text, case for case with the tests in crates/tui/src/title.rs.

import test from 'node:test'
import assert from 'node:assert/strict'
import { fileURLToPath } from 'node:url'
import { loadSync } from './render-support.mjs'

const source = fileURLToPath(new URL('../src/shared/window-title.ts', import.meta.url))
const { windowTitle, PREFIX, MOST_COLUMNS } = loadSync(source)

const points = (text) => Array.from(text).length

test('the title names the session behind the prefix', () => {
  assert.equal(PREFIX, 'Bravebot')
  assert.equal(windowTitle('fix', 'ready'), 'Bravebot · fix')
  assert.equal(windowTitle('  fix  ', 'ready'), 'Bravebot · fix')
})

test('each state puts its marker in front', () => {
  assert.equal(windowTitle('fix', 'working'), '✦ Bravebot · fix')
  assert.equal(windowTitle('fix', 'waiting'), '[!] Bravebot · fix')
})

test('a session with no name gets the bare prefix and its marker', () => {
  assert.equal(windowTitle('', 'ready'), 'Bravebot')
  assert.equal(windowTitle('   ', 'ready'), 'Bravebot')
  assert.equal(windowTitle('', 'working'), '✦ Bravebot')
  assert.equal(windowTitle('  ', 'waiting'), '[!] Bravebot')
})

test('control characters in a name are replaced by visible ones', () => {
  const title = windowTitle('fix\x07\x1b]0;owned\x1b\\\u009c\x1b[2J\nnext\x7f', 'ready')
  assert.doesNotMatch(title, /[\u0000-\u001f\u007f-\u009f]/)
  assert.ok(title.startsWith('Bravebot · fix␇␛]0;owned'), title)
  assert.ok(title.includes('‹U+009C›'), title)
  assert.ok(title.includes('␊next␡'), title)
})

test('a long name is cut to sixty code points ending in …', () => {
  const long = 'a'.repeat(200)
  const cut = windowTitle(long, 'ready')
  assert.equal(MOST_COLUMNS, 60)
  assert.equal(points(cut), MOST_COLUMNS)
  assert.ok(cut.startsWith('Bravebot · aaa'))
  assert.ok(cut.endsWith('…'))
  assert.equal(windowTitle('short', 'ready'), 'Bravebot · short')
})

test('the marker leads the title and counts toward the sixty', () => {
  const long = 'a'.repeat(200)
  for (const [state, marker] of [['working', '✦'], ['waiting', '[!]']]) {
    const cut = windowTitle(long, state)
    assert.ok(cut.startsWith(`${marker} Bravebot · `), cut)
    assert.equal(points(cut), MOST_COLUMNS)
    assert.ok(cut.endsWith('…'))
  }
})

test('a name made of characters outside the basic plane is cut by code point', () => {
  const cut = windowTitle('😀'.repeat(100), 'ready')
  assert.equal(points(cut), MOST_COLUMNS)
  assert.ok(cut.endsWith('😀…'), cut)
})
