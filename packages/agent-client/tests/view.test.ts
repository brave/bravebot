import assert from 'node:assert/strict'
import { test } from 'node:test'
import { ProtocolError } from '../src/common/index.js'
import { applySnapshot, applyUpdate, awaitSnapshot, startView } from '../src/common/view.js'
import type { Row, ViewSnapshot, ViewUpdate } from '../src/common/wire.js'

const row = (id: number, text = ''): Row => ({ id, turn: 0, kind: 'narration', event: null, data: text, resolved: false })
const update = (sequence: number, rows: Row[], status: ViewUpdate['status'] = 'idle'): ViewUpdate =>
  ({ sequence, turn: 0, target: 0, status, pending: null, rows })

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

const snapshot = (sequence: number, rows: Row[], omitted: number): ViewSnapshot => ({ ...update(sequence, rows), omitted })

test('a snapshot replaces the whole view and the next update continues from its sequence', () => {
  let view = startView(update(0, [row(1, 'a')]))
  view = applySnapshot(view, snapshot(7, [row(3, 'c'), row(4, 'd')], 2))
  assert.deepEqual([view.sequence, view.omitted, view.rows.map((item) => item.id)], [7, 2, [3, 4]])
  view = applyUpdate(view, update(8, [row(4, 'd2'), row(5, 'e')]))
  assert.deepEqual(view.rows.map((item) => [item.id, item.data]), [[3, 'c'], [4, 'd2'], [5, 'e']])
  assert.equal(view.ended, null)
})

test('an update to a row the snapshot left out is not appended out of place', () => {
  let view = applySnapshot(startView(update(0, [])), snapshot(4, [row(3, 'c')], 2))
  view = applyUpdate(view, update(5, [row(1, 'a finished'), row(4, 'd')]))
  assert.deepEqual(view.rows.map((item) => item.id), [3, 4])
})

test('a view keeps only the most recent rows it is bounded to and counts the rest as omitted', () => {
  let view = startView(update(0, []))
  for (let id = 1; id <= 10; id++) view = applyUpdate(view, update(id, [row(id)]), 4)
  assert.deepEqual([view.omitted, view.rows.map((item) => item.id)], [6, [7, 8, 9, 10]])
})

test('a view leaves out the rows an update says the runtime no longer keeps, and a lower count brings none back', () => {
  let view = startView(update(0, [row(1), row(2), row(3)]))
  view = applyUpdate(view, { ...update(1, [row(4)]), omitted: 2 })
  assert.deepEqual([view.omitted, view.rows.map((item) => item.id)], [2, [3, 4]])
  view = applyUpdate(view, { ...update(2, [row(2, 'late'), row(5)]), omitted: 1 })
  assert.deepEqual([view.omitted, view.rows.map((item) => item.id)], [2, [3, 4, 5]])
  view = applyUpdate(view, { ...update(3, [row(6)]), omitted: 5 })
  assert.deepEqual([view.omitted, view.rows.map((item) => item.id)], [5, [6]])
})

test('a resyncing view ignores updates until a snapshot, and a snapshot older than the view is refused', () => {
  let view = awaitSnapshot(applyUpdate(startView(update(0, [row(1)])), update(1, [row(2)])))
  assert.equal(applyUpdate(view, update(5, [row(9)])), view)
  assert.throws(() => applySnapshot(view, snapshot(0, [], 0)), ProtocolError)
  view = applySnapshot(view, snapshot(5, [row(1), row(2), row(9)], 0))
  assert.deepEqual([view.resyncing, view.sequence, view.rows.length], [false, 5, 3])
})

test('a snapshot that repeats a row id is refused', () => {
  assert.throws(() => applySnapshot(startView(update(0, [])), snapshot(1, [row(1), row(1)], 0)), ProtocolError)
})
