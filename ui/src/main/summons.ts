/**
 * The record of which bot answered which summoned turn (MEMORY-13), kept the way the forks are.
 *
 * Written here and nowhere else, from the bot a person picked and from what the agent answered when
 * the turn ended. The window reads it through `bravebot:summons:read` and has no channel that
 * writes it.
 */

import { readState, putSummons } from './state'
import { type HomeTurn } from '../shared/bots'
import { type SessionRef } from '../shared/forks'
import {
  REPLY_MAX,
  withSummon,
  withoutSession,
  withoutSummonAt,
  type Summon,
  type SummonHost,
} from '../shared/summons'

/** Every summon remembered, newest first. */
export function summons(): Summon[] {
  return readState().summons
}

/** What a summoned send left waiting for its turn to end, keyed by the session's handle. */
export interface Pending {
  slug: string
  host: SummonHost
  after: HomeTurn | null
}

/**
 * Note how a turn ended at `prompt` of the conversation `session`.
 *
 * With `pending`, that prompt was summoned and the record says so. Without, it was not, and
 * anything recorded at that prompt before (a turn rewound and sent again) is removed.
 */
export function noteTurnEnded(session: SessionRef, prompt: number, pending: Pending | null, reply: string): void {
  const held = summons()
  if (!pending) {
    const next = withoutSummonAt(held, session, prompt)
    if (next.length !== held.length) putSummons(next)
    return
  }
  putSummons(withSummon(held, {
    session,
    prompt,
    slug: pending.slug,
    host: pending.host,
    after: pending.after,
    reply: reply.slice(0, REPLY_MAX),
    at: Date.now(),
  }))
}

/** Forget everything recorded in a conversation the agent has deleted. */
export function forgetSummons(session: SessionRef): void {
  const held = summons()
  const next = withoutSession(held, session)
  if (next.length !== held.length) putSummons(next)
}
