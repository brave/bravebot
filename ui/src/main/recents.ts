/**
 * Where the recently-opened projects are kept.
 *
 * The same arrangement as everything else this app remembers, for the same measured reason: the
 * renderer runs on a `file://` origin whose storage Chromium discards between launches, so
 * anything meant to outlive a run lives in the file the main process owns. `state.ts` holds it
 * under its own key.
 *
 * The main process records these itself rather than taking them from the renderer. That keeps the
 * promise `chooseDirectory` already makes — the renderer is never handed a path it invented, and
 * it cannot forge one into the list either. It can read the list and it can ask to open something
 * on it; there is no channel by which it can write to it.
 */

import { RECENTS_MAX, withMostRecent } from '../shared/recents'
import { putRecents, readState } from './state'

/** The projects opened before, newest first. Never throws; an unreadable file is empty. */
export function recents(): string[] {
  return readState().recents
}

/**
 * Record that a project was opened. Says whether the list actually changed.
 *
 * The caller uses that to decide whether to rebuild the menu — the recents submenu is the one part
 * of it whose *structure* varies, and rebuilding is expensive enough to be worth not doing when
 * reopening the project that was already at the front.
 */
export function noteProject(directory: string): boolean {
  const before = recents()
  const after = withMostRecent(before, directory)
  if (before.length === after.length && before.every((old, index) => old === after[index])) {
    return false
  }
  putRecents(after)
  return after.length <= RECENTS_MAX
}
