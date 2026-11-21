/**
 * The desktop list of background sessions (BG-17), as data: the rows `background.list` sends,
 * grouped under the BG-16 headings in the BG-16 order. Nothing here draws.
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
  /** Seconds since the epoch. */
  updated: number
  directory: string
  prompt: string
}

/** One heading of the list and the rows under it. */
export interface BackgroundGroup {
  state: BackgroundState
  heading: string
  count: number
  rows: BackgroundRow[]
}

/** The BG-16 order of the headings, which is `listing::ROSTER_ORDER`. */
export const ORDER: readonly BackgroundState[] = ['needs-input', 'interrupted', 'working', 'idle', 'stopped']

/** The Sessions screen's word for a state. `idle` is drawn as Ready. */
const HEADINGS: Record<BackgroundState, string> = {
  'needs-input': 'Needs input',
  interrupted: 'Interrupted',
  working: 'Working',
  idle: 'Ready',
  stopped: 'Stopped',
}

/** The heading a state is listed under. */
export function heading(state: BackgroundState): string {
  return HEADINGS[state]
}

/** What a row says about its state, with the kind of prompt it holds where one is held. */
export function stateWords(row: Pick<BackgroundRow, 'state' | 'held'>): string {
  const words = heading(row.state)
  return row.state === 'needs-input' && row.held ? `${words} · ${row.held}` : words
}

/**
 * The rows under a heading for each state in BG-16 order, leaving out a state no row is in.
 * Rows keep the order they came in within a state, which is most recent first from the bridge.
 */
export function grouped(rows: readonly BackgroundRow[]): BackgroundGroup[] {
  return ORDER.flatMap((state) => {
    const under = rows.filter((row) => row.state === state)
    return under.length ? [{ state, heading: heading(state), count: under.length, rows: under }] : []
  })
}

/** The line a row copies, so the person can attach from a terminal (BG-9). */
export function attachCommand(row: Pick<BackgroundRow, 'short'>): string {
  return `bravebot attach ${row.short}`
}
