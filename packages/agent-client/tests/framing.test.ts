import assert from 'node:assert/strict'
import { test } from 'node:test'
import {ProtocolError } from '../src/common/index.js'
import { LineFramer } from '../src/common/framing.js'

test('a line split across chunks is delivered once, whole', () => {
  const framer = new LineFramer()
  assert.deepEqual(framer.push('{"a":'), [])
  assert.deepEqual(framer.push('1}\n'), ['{"a":1}'])
})

test('several lines in one chunk arrive in order, and a trailing fragment waits', () => {
  const framer = new LineFramer()
  assert.deepEqual(framer.push('one\ntwo\nthr'), ['one', 'two'])
  assert.deepEqual(framer.push('ee\n'), ['three'])
})

test('a multi-byte character is kept whole across the newline that follows it', () => {
  const framer = new LineFramer()
  assert.deepEqual(framer.push('héllo ✓\n\n  \nnext\n'), ['héllo ✓', 'next'])
})

test('a message that never ends is refused rather than buffered without bound', () => {
  const framer = new LineFramer(16)
  assert.throws(() => framer.push('x'.repeat(17)), ProtocolError)
  assert.deepEqual(framer.push('ok\n'), ['ok'])
})
