/**
 * What one directory of a session's project looks like, and the one judgement about whether a
 * value is that.
 *
 * Its own file for the reason the recents list and the fork list each give about the other: a
 * validator's job is to be the single judgement about *one* shape, and folding two into a file
 * means a bad value in either costs somebody the other.
 *
 * The shape here is deliberately small. A row is a name and whether it is a directory — no size,
 * no modification time, no contents. The panel this feeds shows what is in the folder and hands a
 * file to the app macOS assigns it; neither of those needs a byte of the file, and a channel that
 * carried file contents into the renderer would be a way for something the agent was never
 * allowed to read to arrive there anyway.
 *
 * Every path in here is *relative* to the session's own directory, and `''` is that directory.
 * The renderer never learns an absolute path and never composes one, which is the promise
 * `chooseDirectory` makes in `main/index.ts`; the main process holds the roots and decides what a
 * relative path means.
 */

/** One entry in a directory. */
export interface FileRow {
  name: string
  /**
   * A symlink is reported as whatever it resolves to, and a broken one as a file. The panel is
   * about what is there to look at, and a link to a directory opens like a directory.
   */
  kind: 'directory' | 'file'
  /**
   * Dot-prefixed. Carried rather than filtered out, so one listing serves the panel with hidden
   * entries shown and with them hidden — the toggle costs no round trip.
   *
   * Checked rather than trusted on the way through `parseListing`: it must agree with the name it
   * sits beside, so nothing can arrive claiming `.env` is an ordinary file.
   */
  hidden: boolean
}

/** One directory, listed. */
export interface Listing {
  /** Relative to the session's directory; `''` is that directory itself. */
  path: string
  rows: FileRow[]
  /**
   * Whether there was more than this. Said out loud rather than silently dropped: a panel showing
   * two thousand of six thousand names without saying so is the interface lying about a folder.
   */
  truncated: boolean
}

/** What became of a double-click. */
export type OpenOutcome = { status: 'opened' } | { status: 'failed'; message: string }

export interface FilePreview { path: string; text: string; truncated: boolean }
export interface FileSearch { paths: string[]; incomplete: boolean }
export interface FileAttachment { id: string; path: string }

/**
 * Enough rows that no real source directory is clipped, few enough that one `readdir` cannot
 * hand the renderer a list it will spend a second laying out.
 */
export const ROWS_MAX = 2000

/**
 * Whether a value is a path this app will resolve inside a session's directory.
 *
 * Relative, with no empty, `.` or `..` segment, and no NUL. This is the cheap half of the
 * promise and not the whole of it: it is lexical, and a symlink is not. The main process resolves
 * the pair and checks with `realpath` that what came out is still inside the root — see
 * `main/files.ts`. Both halves are needed, and this one runs first because a request it refuses
 * never becomes a syscall.
 */
export function isSubpath(value: unknown): value is string {
  if (typeof value !== 'string' || value.includes('\0')) return false
  if (value === '') return true
  if (value.startsWith('/')) return false
  return value
    .split('/')
    .every((segment) => segment.length > 0 && segment !== '.' && segment !== '..')
}

/** Whether a value is a name a directory entry can have: one segment, and a real one. */
function isEntryName(value: unknown): value is string {
  return (
    typeof value === 'string' &&
    value.length > 0 &&
    !value.includes('/') &&
    !value.includes('\0') &&
    value !== '.' &&
    value !== '..'
  )
}

/**
 * A listing, or `null` for a value that is not one.
 *
 * Nothing is coerced and nothing is guessed at, the discipline `shared/layout.ts` states. Unlike
 * the fork list, a bad *row* here fails the whole listing rather than being skipped: the recents
 * and forks files are conveniences whose worst failure is a missing banner, whereas a directory
 * silently missing an entry is this panel telling somebody a file is not there. A listing that
 * cannot be read is better said as nothing at all.
 *
 * Rows past `ROWS_MAX` are the one exception, and they set `truncated` on the way out — dropped
 * with the panel told, rather than dropped quietly.
 */
export function parseListing(value: unknown): Listing | null {
  if (typeof value !== 'object' || value === null) return null
  const { path, rows, truncated } = value as Record<string, unknown>
  if (!isSubpath(path)) return null
  if (!Array.isArray(rows)) return null
  if (typeof truncated !== 'boolean') return null

  const kept: FileRow[] = []
  for (const row of rows) {
    if (kept.length === ROWS_MAX) return { path, rows: kept, truncated: true }
    if (typeof row !== 'object' || row === null) return null
    const { name, kind, hidden } = row as Record<string, unknown>
    if (!isEntryName(name)) return null
    if (kind !== 'directory' && kind !== 'file') return null
    if (typeof hidden !== 'boolean' || hidden !== name.startsWith('.')) return null
    kept.push({ name, kind, hidden })
  }
  return { path, rows: kept, truncated }
}

/** Where a row sits, as a path this app will accept back. `''` is the root. */
export function under(path: string, name: string): string {
  return path === '' ? name : `${path}/${name}`
}
