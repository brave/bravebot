import assert from 'node:assert/strict'
import { test } from 'node:test'
import { ProtocolError } from '../src/common/index.js'
import { applyUpdate, startView } from '../src/common/view.js'
import type { Row, ViewUpdate } from '../src/common/wire.js'

const row = (id: number, text = ''): Row => ({ id, turn: 0, kind: 'narration', event: null, data: text, resolved: false })
const update = (sequence: number, rows: Row[], status: ViewUpdate['status'] = 'idle'): ViewUpdate =>
  ({ sequence, turn: 0, status, pending: null, rows })

test('an initial view that repeats a row id is refused', () => {
  assert.throws(() => startView(update(0, [row(1, 'a'), row(1, 'b')])), ProtocolError)
})

test('an initial view that is already detached is ended', () => {
  const view = startView(update(0, [row(1)], 'detached'))
  assert.equal(view.ended?.reason, 'detached')
  assert.equal(view.rows.length, 1)
})

test('a replaced row keeps its position and an appended row goes last, over many updates', () => {
  let view = startView(update(0, [row(1, 'a'), row(2, 'b')]))
  view = applyUpdate(view, update(1, [row(2, 'b2'), row(3, 'c')]))
  view = applyUpdate(view, update(2, [row(1, 'a2'), row(4, 'd')]))
  assert.deepEqual(view.rows.map((item) => [item.id, item.data]), [[1, 'a2'], [2, 'b2'], [3, 'c'], [4, 'd']])
})

test('an update with no rows keeps the rows and replaces the status', () => {
  const first = startView(update(0, [row(1, 'a')]))
  const next = applyUpdate(first, update(1, [], 'running'))
  assert.deepEqual(next.rows.map((item) => item.id), [1])
  assert.equal(next.status, 'running')
})
