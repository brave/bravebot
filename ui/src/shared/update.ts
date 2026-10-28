/**
 * What the window says about a newer release, decided once (UPDATE-12).
 *
 * The bridge's `update.status` answers with the check's state, this build's version and whether a
 * newer release is recorded, and never the recorded version (UPDATE-5). So a dismissal cannot name
 * the release it dismissed: it names the version that was running, and lasts until the app runs
 * another. A banner that came back for each newer release would need the recorded version, which
 * the window is not told.
 *
 * Nothing here imports `electron` or `react`, so the main process, the renderer and the node tests
 * read the same rules.
 */

export interface UpdateStatus {
  check: 'on' | 'off'
  /** This build's version, or `null` where the bridge would not send it. */
  running: string | null
  /** `null` where nothing is recorded, nothing that could be read, or a record the bridge found stale. */
  newer: boolean | null
  /** When a check last learned the newest release, as a UTC timestamp, or `null` where none did. */
  checked: string | null
}

/** Where the desktop app's installers are. The app has no shell command to update itself with. */
export const RELEASES_LATEST = 'https://github.com/brave/bravebot/releases/latest'

/** Where an issue is opened. Nothing is put in the query: a URL is logged on its way. */
export const NEW_ISSUE = 'https://github.com/brave/bravebot/issues/new'

/** The two pages the window may ask the main process to open, by name. */
export const PAGES = { releases: RELEASES_LATEST, 'new-issue': NEW_ISSUE } as const
export type PageName = keyof typeof PAGES

/** The URL a name stands for, or nothing for a name that is not one of [`PAGES`]. */
export function pageUrl(name: unknown): string | null {
  return typeof name === 'string' && Object.hasOwn(PAGES, name) ? PAGES[name as PageName] : null
}

/**
 * An `update.status` answer, or nothing where it is not one.
 *
 * A translation of the wire shape and nothing more: the bridge decides what a version and a time
 * look like and whether a record is stale, and sends null for what it would not vouch for, so the
 * window checks none of it again (SHARE-001).
 */
export function parseUpdateStatus(value: unknown): UpdateStatus | null {
  if (typeof value !== 'object' || value === null) return null
  const { check, running, newer, checked } = value as Record<string, unknown>
  if (check !== 'on' && check !== 'off') return null
  if (newer !== null && typeof newer !== 'boolean') return null
  const off = check === 'off'
  return {
    check,
    running: typeof running === 'string' ? running : null,
    newer: off ? null : newer,
    checked: !off && typeof checked === 'string' ? checked : null,
  }
}

/**
 * Whether the sidebar shows the banner: a newer release is recorded in a check the bridge found
 * fresh, and this build was not dismissed. A stale record says nothing here; About says when the
 * last check was.
 */
export function bannerShown(status: UpdateStatus | null, dismissed: string | null): boolean {
  return status !== null && status.check === 'on' && status.newer === true && status.running !== null && dismissed !== status.running
}

export type UpdateState = 'off' | 'unchecked' | 'current' | 'newer' | 'stale'

/**
 * Which of the five things About says. An answer that could not be read is the same as none, and a
 * record the bridge found stale is one with a time and no comparison.
 */
export function updateState(status: UpdateStatus | null): UpdateState {
  if (status === null) return 'unchecked'
  if (status.check === 'off') return 'off'
  if (status.newer === null) return status.checked === null ? 'unchecked' : 'stale'
  return status.newer ? 'newer' : 'current'
}

/**
 * About's line for each state. Only the running version is named, since only it is known, and the
 * time of the last check is put as `date` writes it, which the renderer gives as its own date
 * format.
 */
export function updateLine(status: UpdateStatus | null, date: (iso: string) => string): string {
  const checked = status?.checked ?? null
  switch (updateState(status)) {
    case 'off':
      return 'Update checks are off'
    case 'unchecked':
      return 'Not checked yet'
    case 'current':
      return checked === null ? 'Up to date as of the last check' : `Up to date as of ${date(checked)}`
    case 'stale':
      return `Couldn't check recently (last checked ${date(checked ?? '')})`
    case 'newer':
      return 'A newer version is available'
  }
}
