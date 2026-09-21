/**
 * How the session list is arranged, and the one thing that decides whether a file on disk is
 * that.
 *
 * A separate file from the layout next door rather than a field in it, for the reason
 * [`parseRecents`] gives about its own: each validator's job is to be the single judgement
 * about *one* shape. Folding a view preference into the layout means a hand-edited grouping
 * flag can cost somebody their column widths, which is a coupling neither feature asked for.
 */

import { isProjectPath } from './recents'

export interface StoredView {
  /** Whether the sessions are grouped under the checkout they were started in. */
  grouped: boolean
  /**
   * The checkouts whose group is shut, by absolute path.
   *
   * The shut ones rather than the open ones, so that a project started since the last launch
   * arrives open — the list's own default — instead of hidden behind a heading nobody has
   * ever collapsed.
   */
  collapsed: string[]
  /**
   * Which of the column's two lists is on screen.
   *
   * A sessions-and-bots pair rather than a boolean, because a third list is a thing a column like
   * this grows and a `showingBots` flag would have to be replaced rather than extended.
   */
  tab: Tab
}

export const TABS = ['sessions', 'bots'] as const

export type Tab = (typeof TABS)[number]

/** What a file that has never been written means: the flat list the column always showed. */
const FLAT: StoredView = { grouped: false, collapsed: [], tab: 'sessions' }

/**
 * A view preference, always.
 *
 * Unlike [`parseLayout`] this never returns null, because there is nothing for null to mean:
 * no file, a truncated file, and a file with nothing usable in it all describe the same
 * column, and distinguishing them would only give the caller a decision it does not have to
 * make.
 *
 * Nothing is coerced. `grouped: 1` is not a preference, it is a file nobody wrote on purpose,
 * and reading it as one is how a bad value survives a launch.
 */
export function parseView(value: unknown): StoredView {
  if (typeof value !== 'object' || value === null) return flat()
  const { grouped, collapsed, tab } = value as {
    grouped?: unknown
    collapsed?: unknown
    tab?: unknown
  }
  if (typeof grouped !== 'boolean') return flat()
  return { grouped, collapsed: shut(collapsed), tab: shown(tab) }
}

/**
 * Which list to show.
 *
 * Repaired rather than refused, unlike the `grouped` flag above and like the paths below. A tab
 * this build does not have — one written by a later version, or renamed since — should cost the
 * person the tab they were on and not their grouping or their folds. The sessions list is the
 * answer to every doubt because it is the one every launch before this had.
 */
function shown(value: unknown): Tab {
  return typeof value === 'string' && (TABS as readonly string[]).includes(value)
    ? (value as Tab)
    : 'sessions'
}

/**
 * The paths whose group is shut.
 *
 * Filtered rather than the whole file refused, the way [`parseRecents`] treats its list and
 * unlike the `grouped` flag above: that flag is the shape's reason for existing, while one
 * unusable path here should not cost somebody the arrangement or the seven good paths beside
 * it. A folded group is a convenience, and throwing the lot away over one bad line is the
 * tail wagging the dog.
 *
 * Not checked against the filesystem. A project on an unmounted volume is still one you
 * folded shut, and it should still be shut when the volume comes back. Nothing is coerced,
 * and duplicates collapse — a path is shut or it is not, and saying so twice means nothing.
 */
function shut(value: unknown): string[] {
  if (!Array.isArray(value)) return []
  return [...new Set(value.filter(isProjectPath))]
}

/** A fresh one every time: `FLAT`'s own array must not end up shared between callers. */
function flat(): StoredView {
  return { ...FLAT, collapsed: [] }
}
