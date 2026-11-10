import { ProtocolError } from './errors.js'
import type { Pending, Row, Status, ViewSnapshot, ViewUpdate } from './wire.js'

export type ViewEndReason = 'detached' | 'sequence_gap' | 'protocol_error' | 'connection_lost'

export interface ViewEnd {
  reason: ViewEndReason
  detail: string
}

/**
 * What the client holds of one session's view: the rows it was sent and the last status fields.
 *
 * It is a copy of what Rust supplied. Busy state, approval state and row content are never
 * derived here; a view that has `ended` is the last state received and is not recovered. A view
 * that is `resyncing` is the last state received before a missed update, until a snapshot replaces
 * it.
 */
export interface ViewState {
  readonly sequence: number
  readonly turn: number
  /** What a cancel names to stop the turn on screen. */
  readonly target: number
  readonly status: Status
  readonly pending: Pending | null
  /** In the order each row first appeared. */
  readonly rows: readonly Row[]
  /** How many rows before the first in `rows` are not held. */
  readonly omitted: number
  /** Whether an update was missed and a snapshot has been asked for. */
  readonly resyncing: boolean
  readonly ended: ViewEnd | null
}

/** Keep at most `retained` rows, the most recent, and count the rest as omitted. */
function bound(rows: Row[], omitted: number, retained: number): { rows: Row[]; omitted: number } {
  const over = rows.length - retained
  return over > 0 ? { rows: rows.slice(over), omitted: omitted + over } : { rows, omitted }
}

/** The state a `session.view.initial` event starts from. Sequence 0 is the only valid start. */
export function startView(initial: ViewUpdate): ViewState {
  if (initial.sequence !== 0) throw new ProtocolError(`the initial view has sequence ${initial.sequence}, not 0`)
  const ids = new Set(initial.rows.map((row) => row.id))
  if (ids.size !== initial.rows.length) throw new ProtocolError('the initial view repeats a row id')
  const view: ViewState = { sequence: 0, turn: initial.turn, target: initial.target, status: initial.status, pending: initial.pending, rows: [...initial.rows], omitted: 0, resyncing: false, ended: null }
  return initial.status === 'detached' ? endView(view, 'detached', 'the session was closed') : view
}

/**
 * Apply the next update: replace each listed row at its existing position (or append a new one)
 * and replace every status field. A sequence that is not the next one ends the view. Row ids
 * count from 1, so a row whose id is within `omitted` is one the view does not hold and is left
 * out. At most `retained` rows are kept. A view that is resyncing ignores updates: the snapshot it
 * waits for includes them.
 */
export function applyUpdate(view: ViewState, update: ViewUpdate, retained = Infinity): ViewState {
  if (view.ended !== null || view.resyncing) return view
  if (update.sequence !== view.sequence + 1) {
    return endView(view, 'sequence_gap', `expected sequence ${view.sequence + 1}, received ${update.sequence}`)
  }
  const rows = [...view.rows]
  const positions = new Map<number, number>()
  rows.forEach((row, at) => positions.set(row.id, at))
  for (const row of update.rows) {
    if (row.id <= view.omitted) continue
    const at = positions.get(row.id)
    if (at === undefined) {
      positions.set(row.id, rows.length)
      rows.push(row)
    } else {
      rows[at] = row
    }
  }
  const next: ViewState = {
    sequence: update.sequence,
    turn: update.turn,
    target: update.target,
    status: update.status,
    pending: update.pending,
    ...bound(rows, view.omitted, retained),
    resyncing: false,
    ended: null,
  }
  return update.status === 'detached' ? endView(next, 'detached', 'the session was closed') : next
}

/** Hold the view as it is, ignoring updates, until a snapshot replaces it. */
export function awaitSnapshot(view: ViewState): ViewState {
  return view.ended !== null || view.resyncing ? view : { ...view, resyncing: true }
}

/**
 * Replace the whole view with a snapshot. Updates that follow it continue from its sequence. A
 * snapshot older than the view is refused, since it would undo state already shown.
 */
export function applySnapshot(view: ViewState, snapshot: ViewSnapshot): ViewState {
  if (view.ended !== null) return view
  if (snapshot.sequence < view.sequence) {
    throw new ProtocolError(`a snapshot at sequence ${snapshot.sequence} arrived after sequence ${view.sequence}`)
  }
  if (new Set(snapshot.rows.map((row) => row.id)).size !== snapshot.rows.length) throw new ProtocolError('a snapshot repeats a row id')
  const next: ViewState = {
    sequence: snapshot.sequence,
    turn: snapshot.turn,
    target: snapshot.target,
    status: snapshot.status,
    pending: snapshot.pending,
    rows: [...snapshot.rows],
    omitted: snapshot.omitted,
    resyncing: false,
    ended: null,
  }
  return snapshot.status === 'detached' ? endView(next, 'detached', 'the session was closed') : next
}

export function endView(view: ViewState, reason: ViewEndReason, detail: string): ViewState {
  if (view.ended !== null) return view
  return { ...view, ended: { reason, detail } }
}
