import { ProtocolError } from './errors.js'
import type { Pending, Row, Status, ViewUpdate } from './wire.js'

export type ViewEndReason = 'detached' | 'sequence_gap' | 'protocol_error' | 'connection_lost'

export interface ViewEnd {
  reason: ViewEndReason
  detail: string
}

/**
 * What the client holds of one session's view: the rows it was sent and the last status fields.
 *
 * It is a copy of what Rust supplied. Busy state, approval state and row content are never
 * derived here; a view that has `ended` is the last state received and is not recovered.
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
  readonly ended: ViewEnd | null
}

/** The state a `session.view.initial` event starts from. Sequence 0 is the only valid start. */
export function startView(initial: ViewUpdate): ViewState {
  if (initial.sequence !== 0) throw new ProtocolError(`the initial view has sequence ${initial.sequence}, not 0`)
  const ids = new Set(initial.rows.map((row) => row.id))
  if (ids.size !== initial.rows.length) throw new ProtocolError('the initial view repeats a row id')
  const view: ViewState = { sequence: 0, turn: initial.turn, target: initial.target, status: initial.status, pending: initial.pending, rows: [...initial.rows], ended: null }
  return initial.status === 'detached' ? endView(view, 'detached', 'the session was closed') : view
}

/**
 * Apply the next update: replace each listed row at its existing position (or append a new one)
 * and replace every status field. A sequence that is not the next one ends the view.
 */
export function applyUpdate(view: ViewState, update: ViewUpdate): ViewState {
  if (view.ended !== null) return view
  if (update.sequence !== view.sequence + 1) {
    return endView(view, 'sequence_gap', `expected sequence ${view.sequence + 1}, received ${update.sequence}`)
  }
  const rows = [...view.rows]
  const positions = new Map<number, number>()
  rows.forEach((row, at) => positions.set(row.id, at))
  for (const row of update.rows) {
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
    rows,
    ended: null,
  }
  return update.status === 'detached' ? endView(next, 'detached', 'the session was closed') : next
}

export function endView(view: ViewState, reason: ViewEndReason, detail: string): ViewState {
  if (view.ended !== null) return view
  return { ...view, ended: { reason, detail } }
}
