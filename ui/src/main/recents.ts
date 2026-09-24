/**
 * Where the recently-opened projects are kept.
 *
 * The same arrangement as everything else this app remembers, for the same measured reason: the
 * renderer runs on a `file://` origin whose storage Chromium discards between launches, so
 * anything meant to outlive a run lives in the file the main process owns. `state.ts` holds it
 * under its own key.
 *
 * The main process records these itself: every write below is made where a folder was just
 * chosen, opened or forked, and there is no channel that takes a list from a window. What a
 * window can do is ask to open a session in a folder, and that is an opening like any other, so
 * the entry it leaves is real. It is also the reason membership here is a record of what has been
 * opened and not an authority over what may be: a window that names a folder decides what goes on
 * this list, so a check against the list is one it can answer for itself. `opened.ts` is the
 * authority, and it holds only what came back from the picker.
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
