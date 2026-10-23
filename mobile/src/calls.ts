import type { Row, Status } from '@brave/agent-client'
import { readActivity } from './payloads'

export type CallState = 'running' | 'unfinished' | 'done' | 'failed'

export interface Call {
  id: number
  verb: string
  target: string
  state: CallState
  note: string | null
  untrusted: boolean
}

const TOOL_EVENTS = new Set(['tool.started', 'tool.finished'])

/** Whether the view's status says a turn is still working or waiting. A view that ended has no turn running, whatever its last status was. */
export function turnIsRunning(status: Status, ended = false): boolean {
  return !ended && (status === 'running' || status === 'waiting')
}

/** The turn of the last row, which is the only turn that can still be running. A turn number can be reused after a rewind, so the largest is not the newest. */
export function latestTurn(rows: readonly Row[]): number {
  return rows[rows.length - 1]?.turn ?? 0
}

/** How a call's state is worded, for a screen reader and the Calls list. */
export const CALL_STATE_WORD = { running: 'running', unfinished: 'did not finish', done: 'done', failed: 'failed' } as const satisfies Record<CallState, string>

/** A call whose row is still its start is running only while its turn is. After a turn ends, cancelled or failed, it did not finish. */
export function callState(row: Pick<Row, 'event'>, activity: { failed: boolean }, live: boolean): CallState {
  if (row.event !== 'tool.finished') return live ? 'running' : 'unfinished'
  return activity.failed ? 'failed' : 'done'
}

/** One call per tool row, in row order. Only the row's kind, event and structured fields decide; the target and note are carried, never read. A call's target is the spelled one, since the list only draws it. */
export function callsFrom(rows: readonly Row[], running: boolean): Call[] {
  const latest = latestTurn(rows)
  const calls: Call[] = []
  for (const row of rows) {
    if (row.kind !== 'activity' || row.event === null || !TOOL_EVENTS.has(row.event)) continue
    const activity = readActivity(row.data)
    if (activity === null) continue
    calls.push({
      id: row.id,
      verb: activity.verb,
      target: activity.shownTarget,
      state: callState(row, activity, running && row.turn === latest),
      note: activity.note,
      untrusted: activity.untrusted,
    })
  }
  return calls
}

/** `Verb(target)`, as the desktop's Calls made list writes it. */
export function callLabel(call: Call): string {
  return `${call.verb.charAt(0).toUpperCase()}${call.verb.slice(1)}(${call.target})`
}
