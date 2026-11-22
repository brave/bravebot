/**
 * The desktop list of background sessions (BG-17), as data: the groups `background.list` sends,
 * which the bridge builds under the BG-16 headings in the BG-16 order with the terminal list's
 * words. Nothing here groups, orders or names a state, and nothing here draws.
 */

/** A BG-6 state word, as `bravebot sessions --json` writes it. */
export type BackgroundState = 'needs-input' | 'interrupted' | 'working' | 'idle' | 'stopped'

/** One row of `background.list`. Every text field has its control characters pictured by the bridge. */
export interface BackgroundRow {
  id: string
  /** The first 8 characters of `id`, which `bravebot attach` takes. */
  short: string
  name: string
  state: BackgroundState
  /** The word for the kind of prompt held, or `null` unless `state` is `needs-input`. */
  held: string | null
  /** What the terminal's row says about the state, with the kind of prompt held where one is. */
  words: string
  /** Seconds since the epoch. */
  updated: number
  directory: string
  prompt: string
}

/** One heading of the list and the sessions under it, most recent first. */
export interface BackgroundGroup {
  state: BackgroundState
  heading: string
  count: number
  sessions: BackgroundRow[]
}

/** The answer of `background.list`. */
export interface BackgroundList {
  groups: BackgroundGroup[]
}

/** Every session of the groups, in the order they are listed. */
export function sessionsOf(groups: readonly BackgroundGroup[]): BackgroundRow[] {
  return groups.flatMap((group) => group.sessions)
}

/** The line a row copies, so the person can attach from a terminal (BG-9). */
export function attachCommand(row: Pick<BackgroundRow, 'short'>): string {
  return `bravebot attach ${row.short}`
}
