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
 * A drive path (`C:\work`, `C:/work`) or a UNC share (`\\server\share`), which are the two
 * spellings Windows resolves without asking where the process happens to be.
 *
 * `C:work` is not one of them: it names the working directory of that drive, which is the thing an
 * absolute path is required here to rule out. Neither is `\\?\` or `\\.\`, the prefixes that
 * hand a path to the device namespace with no normalisation at all, so a share called `?` is
 * refused along with them rather than given a rule of its own. Nor is a spelling carrying a second
 * colon, which names an alternate data stream rather than a folder, on either form.
 */
function isWindowsAbsolute(value: string): boolean {
  const drive = /^[A-Za-z]:[\\/]/.test(value)
  const share = /^\\\\[^\\/:?.][^\\/:]*\\[^\\/:]/.test(value)
  return (drive || share) && !value.slice(2).includes(':')
}

/**
 * Whether something is a path this app would open.
 *
 * Absolute, because that is the only kind `session.new` and `session.open` take, and a
 * relative path here would be resolved against whatever the app's working directory happened
 * to be: a different directory between a dev run and a packaged one.
 *
 * Both platforms' spellings are accepted wherever it runs, rather than the running one's. This
 * module is shared with the renderer, which is given no `process` and so cannot be told which
 * platform it is on, and the cost of the union is nothing: a spelling this host cannot resolve is
 * refused by every step after this one. Nothing is granted by the shape of a path in any case.
 * `isOpenedDirectory` next door is what decides whose folder one names, and it asks the picker.
 */
export function isProjectPath(value: unknown): value is string {
  if (typeof value !== 'string' || value.includes('\0')) return false
  return value.startsWith('/') || isWindowsAbsolute(value)
}

/**
 * The folder's own name, for a row that shows the whole path beside it.
 *
 * Both separators, because a Windows path is spelled with `\` and this is read in the renderer,
 * which cannot ask which platform it is on. A trailing separator is dropped first, so a drive root
 * is labelled with its drive rather than with nothing at all.
 */
export function projectLabel(directory: string): string {
  const leaf = directory.replace(/[\\/]+$/, '').split(/[\\/]/).pop()
  return leaf ? leaf : directory
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
