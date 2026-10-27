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
  /** This build's version. */
  running: string
  /** `null` where nothing is recorded, or nothing that could be read. */
  newer: boolean | null
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

/** Whether `text` is a version's shape, the rule the bridge holds the interface version to. */
export function isVersion(text: unknown): text is string {
  return typeof text === 'string' && /^[A-Za-z0-9.+-]{1,64}$/.test(text)
}

/** An `update.status` answer, or nothing where it is not one. */
export function parseUpdateStatus(value: unknown): UpdateStatus | null {
  if (typeof value !== 'object' || value === null) return null
  const { check, running, newer } = value as Record<string, unknown>
  if (check !== 'on' && check !== 'off') return null
  if (!isVersion(running)) return null
  if (newer !== null && typeof newer !== 'boolean') return null
  return { check, running, newer: check === 'off' ? null : newer }
}

/** Whether the sidebar shows the banner: a newer release is recorded and this build was not dismissed. */
export function bannerShown(status: UpdateStatus | null, dismissed: string | null): boolean {
  return status !== null && status.check === 'on' && status.newer === true && dismissed !== status.running
}

export type UpdateState = 'off' | 'unchecked' | 'current' | 'newer'

/** Which of the four things About says. An answer that could not be read is the same as none. */
export function updateState(status: UpdateStatus | null): UpdateState {
  if (status === null) return 'unchecked'
  if (status.check === 'off') return 'off'
  if (status.newer === null) return 'unchecked'
  return status.newer ? 'newer' : 'current'
}

/** About's line for each state. Only the running version is named, since only it is known. */
export function updateLine(status: UpdateStatus | null): string {
  switch (updateState(status)) {
    case 'off':
      return 'Update checks are off'
    case 'unchecked':
      return 'Not checked yet'
    case 'current':
      return 'Up to date as of the last check'
    case 'newer':
      return 'A newer version is available'
  }
}
