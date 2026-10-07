import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { test } from 'node:test'
import {
  ProtocolError,
  ROW_KINDS,
  STATUSES,
  SUPPORTED_APPROVALS,
  decodeIncoming,
  decodeUpdate,
  readSessionViewCapability,
} from '../src/common/index.js'
import { FIXTURES } from './support/scenario.js'

const contract = JSON.parse(readFileSync(join(FIXTURES, 'wire-contract.json'), 'utf8')) as {
  capability: { approvals: string[] } & Record<string, unknown>
  statuses: string[]
  rowKinds: string[]
  update: unknown
}

/** The Rust test `the_client_wire_contract_matches_the_rust_types` writes this file from the Rust types. */
test('the client enumerations equal the Rust ones', () => {
  assert.deepEqual([...STATUSES], contract.statuses)
  assert.deepEqual([...ROW_KINDS], contract.rowKinds)
  assert.deepEqual([...SUPPORTED_APPROVALS], contract.capability.approvals)
})

test('the capability the Rust runtime advertises is accepted as version 1', () => {
  const read = readSessionViewCapability({ capabilities: { sessionView: contract.capability } })
  assert.deepEqual(read, contract.capability)
})

test('an update the Rust types wrote decodes with every field intact', () => {
  assert.deepEqual(decodeUpdate(contract.update), contract.update)
})

test('an update with a status or kind the contract lacks is refused', () => {
  const update = structuredClone(contract.update) as { status: string; rows: { kind: string }[] }
  update.status = 'paused'
  assert.throws(() => decodeUpdate(update), ProtocolError)
  const other = structuredClone(contract.update) as { rows: { kind: string }[] }
  other.rows[0]!.kind = 'banner'
  assert.throws(() => decodeUpdate(other), ProtocolError)
})

test('a view update missing a required field is refused rather than defaulted', () => {
  for (const field of ['sequence', 'turn', 'status', 'pending', 'rows']) {
    const update = structuredClone(contract.update) as Record<string, unknown>
    delete update[field]
    assert.throws(() => decodeUpdate(update), ProtocolError, field)
  }
})

test('an absent capability, or one that is not version 1, is not read as supported', () => {
  assert.equal(readSessionViewCapability({}), null)
  assert.equal(readSessionViewCapability({ capabilities: {} }), null)
  assert.equal(readSessionViewCapability(null), null)
  assert.equal(readSessionViewCapability({ capabilities: { sessionView: { ...contract.capability, version: 2 } } }), null)
  assert.equal(readSessionViewCapability({ capabilities: { sessionView: { ...contract.capability, version: '1' } } }), null)
})

test('messages are told apart as events and responses', () => {
  assert.deepEqual(decodeIncoming({ event: 'agent.ready', data: {} }), {
    type: 'event',
    event: { event: 'agent.ready', session: null, data: {} },
  })
  assert.deepEqual(decodeIncoming({ id: 4, ok: { a: 1 } }), { type: 'response', id: 4, result: { ok: { a: 1 } } })
  assert.deepEqual(decodeIncoming({ id: 4, error: { code: 'bad_request', message: 'm' } }), {
    type: 'response',
    id: 4,
    result: { error: { code: 'bad_request', message: 'm' } },
  })
  for (const bad of [null, [], { id: 'x', ok: 1 }, { id: 1 }, { id: 1, error: {} }, { event: 'e', session: 3 }]) {
    assert.throws(() => decodeIncoming(bad), ProtocolError, JSON.stringify(bad))
  }
})
