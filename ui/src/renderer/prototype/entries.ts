/**
 * The transcript a shared session draws, made from the rows of the Rust view.
 *
 * Each row carries the bridge's own event name and payload, so the desktop's own constructors make
 * its entry from it, the ones it makes the same entry with from a live event. The rows come from
 * the view and are never made here, so a prompt another client sent appears from the view alone.
 * Nothing here reads a row's text to decide anything: a row's kind and event choose the entry, and
 * a payload that is not an object is drawn as a line saying it could not be.
 */

import type { AskAnswer } from '../../shared/protocol'
import type { Row } from '../../shared/prototype'
import * as t from '../transcript'

/** What a question that was settled without this window's say is called where it is drawn. */
export const SETTLED = 'No longer waiting: answered elsewhere, refused or cancelled'

/** What a question says where the view has ended and nothing can answer it now. */
export const ENDED = 'The view ended, so this can no longer be answered here'

const isObject = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value)

const cannot = (row: Row): t.Entry => t.narrated(`A ${row.kind} row could not be drawn`)

/** Which approval events this window can draw, and the entry each makes. */
const ASKED: Record<string, (data: never) => t.Entry> = {
  'confirm.request': t.asked,
  'run.request': t.askedRun,
  'fetch.request': t.askedFetch,
  'ask.request': t.askedQuestions,
}

/** What this window answered to a question and the host took. */
export type AnsweredHere = 'approve' | 'reject' | readonly AskAnswer[]

export interface Settled {
  /** What this window itself answered, by question number, so its own cards say so. */
  here: ReadonlyMap<number, AnsweredHere>
  /** The view has ended: no question in it can be answered. */
  ended?: boolean
  /** The kinds this client may answer. A card of another kind is drawn as one to answer elsewhere. */
  answers: readonly string[]
}

function entryOf(row: Row): t.Entry {
  const data: unknown = row.data
  if (!isObject(data)) return cannot(row)
  switch (row.kind) {
    case 'prompt':
      return typeof data.text === 'string' ? t.userSaid(data.text) : cannot(row)
    case 'narration':
      return typeof data.text === 'string' ? t.narrated(data.text, typeof data.command === 'string' ? data.command : null) : cannot(row)
    case 'quarantined':
      return t.quarantined(data as never)
    case 'activity':
      return t.started(data as never)
    case 'reply':
      return typeof data.reply === 'string' ? t.replied(data.reply, typeof data.turn === 'number' ? data.turn : undefined) : cannot(row)
    case 'error':
      return {
        ...t.errored(`${String(data.kind)}: ${String(data.message)}`),
        category: data.kind === 'cancelled' ? 'cancelled' : (data.category as string | null | undefined),
        accountMessage: data.accountMessage as string | null | undefined,
        accountCommand: data.accountCommand as string | null | undefined,
        failureMessage: data.failureMessage as string | null | undefined,
        failureCodes: data.failureCodes as string[] | null | undefined,
        failureCause: data.failureCause as string | null | undefined,
        attempts: data.attempts as number | null | undefined,
        status: data.status as number | null | undefined,
        cutOff: data.cutOff as never,
        turn: typeof data.turn === 'number' ? data.turn : undefined,
      } as t.Entry
    case 'approval': {
      const make = row.event === null ? undefined : ASKED[row.event]
      if (!make) return t.narrated('A question of a kind this window cannot draw. Answer it on another device.')
      return make(data as never)
    }
  }
}

/**
 * The entries of a list of rows. A card for a question that is no longer waiting says what this
 * window itself answered where it answered, and otherwise that it was settled. A question that is
 * waiting and that this client may not answer says so, and offers no button.
 */
export function entriesFromRows(rows: readonly Row[], settled: Settled): t.Entry[] {
  return rows.map((row) => {
    const entry = { ...entryOf(row), id: `row-${row.id}` } as t.Entry
    if (row.kind !== 'approval' || !(entry.kind === 'confirm' || entry.kind === 'run' || entry.kind === 'fetch' || entry.kind === 'ask')) return entry
    const request = entry.request.request
    if (row.resolved) {
      const here = settled.here.get(request)
      if (here === 'approve' || here === 'reject') return entry.kind === 'ask' ? { ...entry, interrupted: true } : { ...entry, decision: here }
      if (Array.isArray(here) && entry.kind === 'ask') return { ...entry, answers: [...here] }
      return { ...entry, interrupted: true }
    }
    const answerable = settled.ended !== true && row.event !== null && settled.answers.includes(row.event.replace('.request', ''))
    return answerable ? entry : { ...entry, interrupted: true }
  })
}
