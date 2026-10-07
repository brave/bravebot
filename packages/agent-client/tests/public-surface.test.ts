import assert from 'node:assert/strict'
import { test } from 'node:test'
import * as common from '@brave/agent-client'
import * as node from '@brave/agent-client/node'

// What a caller can reach through the package's entry points. Raw dispatch and the connection class
// would let a caller open a session in any directory or answer a question the client never offered,
// so the lists below name every export and a new one has to be added on purpose.
const COMMON = [
  'CapabilityError', 'ConnectionLostError', 'ProtocolError', 'ROW_KINDS', 'RpcAgentClient', 'RpcError',
  'SESSION_VIEW_START', 'SESSION_VIEW_VERSION', 'STATUSES', 'SUPPORTED_APPROVALS', 'UnsupportedError',
  'decodeIncoming', 'decodeUpdate', 'readSessionViewCapability',
]

test('the package entry points export only the intended names', () => {
  assert.deepEqual(Object.keys(common).sort(), [...COMMON].sort())
  assert.deepEqual(Object.keys(node).sort(), ['connectStdio'])
})

test('no instance reaches raw dispatch or the connection', () => {
  const client = new common.RpcAgentClient({ write: () => undefined })
  for (const name of ['raw', 'connection', '#connection', 'request']) {
    assert.equal(name in client, false, name)
    assert.equal(name in common.RpcAgentClient.prototype, false, name)
  }
  assert.deepEqual(Object.getOwnPropertyNames(client).filter((name) => /connection|raw/i.test(name)), [])
})

test('internal modules cannot be imported through the package name', async () => {
  for (const path of ['client', 'connection', 'framing', 'view']) {
    await assert.rejects(import(`@brave/agent-client/dist/src/common/${path}.js`), { code: 'ERR_PACKAGE_PATH_NOT_EXPORTED' })
  }
})
