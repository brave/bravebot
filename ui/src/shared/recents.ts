/**
 * The projects opened before, and the one thing that decides whether a file on disk is that
 * list.
 *
 * A separate file from the layout rather than a field in it, for the reason `parseLayout`'s
 * own comment gives: that validator's job is to be the single judgement about *that* shape.
 * Folding two shapes into one file means a hand-edited recents entry can cost somebody their
 * column widths, which is a coupling neither feature asked for.
 */

export interface StoredRecents {
  /** Newest first. */
  directories: string[]
}

/** Enough to be useful in a submenu, few enough that the submenu is still scannable. */
export const RECENTS_MAX = 8

/**
 * Whether something is a path this app would open.
 *
 * Absolute, because that is the only kind `session.new` and `session.open` take, and a
 * relative path here would be resolved against whatever the app's working directory happened
 * to be — a different directory between a dev run and a packaged one.
 */
export function isProjectPath(value: unknown): value is string {
  return typeof value === 'string' && value.startsWith('/') && !value.includes('\0')
}

/**
 * A recents list, always.
 *
 * Unlike [`parseLayout`] this never returns null, because there is nothing for null to mean:
 * no file, a truncated file, and a file with nothing usable in it all describe the same
 * empty submenu, and distinguishing them would only give the caller a decision it does not
 * have to make.
 *
 * Entries are filtered rather than the file refused. One bad line should not cost seven good
 * ones — this is a convenience list, and throwing it away wholesale is the tail wagging the
 * dog. Nothing is coerced, duplicates collapse to their newest position, and no path is
 * checked against the filesystem: a project on an unmounted volume is still a project you
 * had open, and it should still be in the menu when the volume comes back.
 */
export function parseRecents(value: unknown): StoredRecents {
  if (typeof value !== 'object' || value === null) return { directories: [] }
  const { directories } = value as { directories?: unknown }
  if (!Array.isArray(directories)) return { directories: [] }

  const seen = new Set<string>()
  const kept: string[] = []
  for (const entry of directories) {
    if (!isProjectPath(entry) || seen.has(entry)) continue
    seen.add(entry)
    kept.push(entry)
    if (kept.length === RECENTS_MAX) break
  }
  return { directories: kept }
}

/** The list with `directory` at the front, however many times it appeared before. */
export function withMostRecent(directories: string[], directory: string): string[] {
  return [directory, ...directories.filter((old) => old !== directory)].slice(0, RECENTS_MAX)
}
