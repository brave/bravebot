/**
 * A request that failed in transit and waits to be sent again, as the window counts it down
 * (VIEW-43).
 *
 * The bridge sends how long each wait is from the moment of the event, because the two ends share
 * no clock, and the window turns that into times of its own when the event arrives. The figures are
 * the driver's own counts and timers: nothing a service wrote is in them.
 */
import type { RetryEvent } from '../shared/protocol'

/** When the next attempt goes and when the request is given up on, in milliseconds since the epoch. */
export interface RetryDue {
  attempt: number
  nextAt: number
  /** `null` where a count and not a window decides how long the request goes on, so no time is known. */
  givesUpAt: number | null
}

/** The times a `phase` event's `retry` names, counted from `receivedAt`. `null` where it names none. */
export function retryDue(retry: RetryEvent | null | undefined, receivedAt: number): RetryDue | null {
  if (!retry) return null
  const givesUp = retry.gives_up_in_ms ?? null
  return {
    attempt: retry.attempt,
    nextAt: receivedAt + retry.in_ms,
    givesUpAt: givesUp === null ? null : receivedAt + givesUp,
  }
}

/** A time left in whole seconds, rounded up so it reaches the second it names, as the terminal says it. */
export function timeLeft(milliseconds: number): string {
  const total = Math.ceil(Math.max(0, milliseconds) / 1000)
  return total < 60 ? `${total}s` : `${Math.floor(total / 60)}m ${String(total % 60).padStart(2, '0')}s`
}

/**
 * What the working row says while the request waits, or `null` once the pause is over, when the
 * attempt that follows it is out and the plain word is the true one.
 */
export function reconnectingWords(due: RetryDue, now: number): string | null {
  if (due.nextAt <= now) return null
  const next = `Reconnecting, next try in ${timeLeft(due.nextAt - now)}`
  return due.givesUpAt === null ? next : `${next}, giving up in ${timeLeft(due.givesUpAt - now)}`
}
