import assert from 'node:assert/strict'
import { test } from 'node:test'
import * as sdk from '../src/index.js'

// What a caller can reach through the package's entry point. A new export has to be added here on
// purpose. There is no permission handler, no approval option, no trust-map or path-vouching
// function, and no function that sends a line of the caller's own.
const RUNTIME = ['ArgumentError', 'ProcessError', 'ProtocolError', 'Session', 'run', 'startSession', 'stream']

test('the package entry point exports only the intended names', () => {
  assert.deepEqual(Object.keys(sdk).sort(), [...RUNTIME].sort())
})

test('nothing in the public surface approves an effect, answers a permission or vouches for a path', () => {
  const names = [...Object.keys(sdk), ...Object.getOwnPropertyNames(sdk.Session.prototype)]
  assert.equal(names.some((name) => /permission|approve|decide|trust(?!InputLines)|vouch|allow|grant|accept/i.test(name)), false, names.join(','))
})

test('a session offers turns, steers, stops and position answers, and no method that sends a line of the caller', () => {
  const methods = Object.getOwnPropertyNames(sdk.Session.prototype).filter((name) => name !== 'constructor').sort()
  assert.deepEqual(methods, ['answer', 'close', 'kill', 'nextTurn', 'prompt', 'send', 'steer', 'stop'])
})

test('internal modules cannot be imported through the package name', async () => {
  for (const path of ['args', 'process', 'lines', 'session']) {
    await assert.rejects(import(`@brave/bravebot-sdk/dist/src/${path}.js`), { code: 'ERR_PACKAGE_PATH_NOT_EXPORTED' })
  }
})
