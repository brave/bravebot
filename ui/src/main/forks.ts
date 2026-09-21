/**
 * Where the record of which session came out of which is kept.
 *
 * The same arrangement as the recents list next door, and for the same two reasons. The
 * renderer's storage does not survive a launch, so anything meant to outlive one lives in a file
 * the main process owns — `state.ts`, which holds this under its own key alongside the other four
 * things this app remembers. And the main process writes this itself, from what the *agent*
 * answered rather than from what the renderer asked for: the window can read the list and it can
 * ask for a fork, but there is no channel by which it can write a line into it.
 *
 * The agent's own record cannot hold this; `src/shared/forks.ts` says why.
 */

import { type Fork, withFork } from '../shared/forks'
import { putForks, readState } from './state'

/** Every fork taken before, newest first. Never throws; an unreadable file is empty. */
export function forks(): Fork[] {
  return readState().forks
}

/**
 * Record that one session was cut out of another.
 *
 * A failure is swallowed inside `putForks`. The fork itself has already happened — it is a live
 * session with a conversation in it — and losing the line that says where it came from costs a
 * banner, which is not worth an error on screen over.
 */
export function noteFork(fork: Fork): void {
  putForks(withFork(forks(), fork))
}
