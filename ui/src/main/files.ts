import { isAttachableProjectFile, readProjectText } from './project-files'
/**
 * Looking at the folder a session is working in.
 *
 * Two operations — list a directory, hand a file to whichever app macOS assigns it — and one
 * thing that makes them safe: the renderer names a *session* and a path relative to that
 * session's directory, never a path. The roots live here, learned from what the agent answered
 * when a session was opened, forked or made. So the renderer cannot ask about a folder no session
 * of its is running in, which is the same promise `chooseDirectory` makes next door: it is never
 * handed a path it invented, and it cannot forge one.
 *
 * The lexical half of the check is `isSubpath` in `shared/files.ts`, and it is not enough on its
 * own — `foo/link` is a fine relative path and the link can point at `/etc`. So every resolution
 * for listing/opening goes through `realpath` and must land inside the root's own `realpath`. A symlink out of
 * the project is refused rather than followed, including one whose *target* is what somebody
 * double-clicked: a link is a way out of the folder, and this panel is about the folder.
 *
 * Text reads additionally use pinned directory descriptors through the secure-file helper.
 * Previews are returned only to the person reviewing the project. The renderer names no path for
 * the agent to read: a file goes to it through a native selection that leaves an opaque attachment
 * token, or as a name written with `@` in the prompt, which the bridge reads back out of the
 * prompt at `turn.send` and confines to the session's workspace itself.
 */

import { dialog, type BrowserWindow } from 'electron'
import { openPath, showItemInFolder } from './external-open'
import { randomUUID } from 'node:crypto'
import { readdirSync, realpathSync, statSync } from 'node:fs'
import { readdir } from 'node:fs/promises'
import { basename, extname, join, sep, relative } from 'node:path'
import { isProjectPath } from '../shared/recents'
import { isSessionId } from '../shared/forks'
import {
  ROWS_MAX,
  parseListing,
  type FileRow,
  type Listing,
  type OpenOutcome,
  type FilePreview,
  type FileSearch,
  isSubpath,
  type FileAttachment,
} from '../shared/files'

/**
 * Which session is working in which directory.
 *
 * In memory and for the life of the run only: unlike the recents list this is not a record of
 * anything, it is the answer to "what does a relative path mean for this session", and a stale
 * one on disk would be a root nothing is running in.
 */
const roots = new Map<string, string>()
export const rootForSession = (handle: string): string | undefined => roots.get(handle)
const attachments = new Map<string, Map<string, string>>()

/**
 * The folders each session has open beside its project, as the agent last listed them.
 *
 * Taken only off the agent's answers (to `session.folders.list`, `.add` and `.close`, to a
 * `folder.reply`, and the list `session.new`, `.open`, `.fork` and `trust.reply` carry), never off
 * what the window sent, so a folder the tree can browse is one the bridge validated and opened,
 * and one it has closed stops being browsable with the answer that closed it.
 */
const folders = new Map<string, string[]>()

/** The folders `handle` has open beside its project, as the agent last listed them. */
export const foldersForSession = (handle: string): string[] => folders.get(handle) ?? []

/**
 * Remember which folders a session has open beside its project, from the agent's answer. Anything
 * that is not a list of absolute paths leaves the session with none, rather than with a guess.
 */
export function noteFolders(handle: string, listed: unknown): void {
  if (!Array.isArray(listed)) {
    folders.delete(handle)
    return
  }
  folders.set(handle, listed.filter((each): each is string => isProjectPath(each)))
}

/** The methods whose answer lists the session's open folders, and where the session is named. */
const LISTS_FOLDERS: Record<string, 'params' | 'answer'> = {
  'session.folders.list': 'params',
  'session.folders.close': 'params',
  // A yes to a directory a checkout's settings named opens it, as a picked folder is (TRUST-9).
  'folder.reply': 'params',
  // A session that opens bypassing has its named directories opened before anything is asked.
  'trust.reply': 'params',
  'session.new': 'answer',
  'session.open': 'answer',
  'session.fork': 'answer',
}

/**
 * Note the folders a session has open from the agent's answer `ok` to `method`, where that answer
 * lists them. The session is the one the request named, or for a session opened, the one the
 * answer names. An answer that does not list them leaves the list as it was, except from the
 * folder methods themselves, whose answer always does.
 */
export function noteAnsweredFolders(method: string, params: unknown, ok: unknown): void {
  const from = Object.hasOwn(LISTS_FOLDERS, method) ? LISTS_FOLDERS[method] : undefined
  if (!from || typeof ok !== 'object' || ok === null) return
  const answer = ok as { session?: unknown; folders?: unknown }
  const handle = from === 'answer' ? answer.session : (params as { session?: unknown } | null)?.session
  if (!isSessionId(handle)) return
  if (!Array.isArray(answer.folders) && !method.startsWith('session.folders.')) return
  noteFolders(handle, answer.folders)
}

/**
 * The directory a session-relative path is relative to: the project where `folder` is absent, and
 * otherwise `folder` itself, if and only if it is one of the session's open folders as the agent
 * listed them. The window names a folder by the path the agent gave it, and a path it composed is
 * no folder at all.
 */
function rootOf(handle: string, folder?: unknown): string | undefined {
  if (folder === undefined || folder === null) return roots.get(handle)
  if (typeof folder !== 'string') return undefined
  return folders.get(handle)?.includes(folder) ? folder : undefined
}

/**
 * Remember where a session is working.
 *
 * The caller reads the handle off the agent's *answer* rather than off the request, for the
 * reason the fork list gives: a root on this map is then one the agent confirmed, not one the
 * renderer asserted. The directory is checked as a project path here so a malformed one never
 * becomes a key somebody can browse from.
 */
export function noteRoot(handle: string, directory: unknown): void {
  if (isProjectPath(directory)) roots.set(handle, directory)
}

/** Forget a session that has been closed. Its handle is no longer an answer to anything. */
export function forgetRoot(handle: string): void {
  roots.delete(handle)
  attachments.delete(handle)
  folders.delete(handle)
}

/** Attachment authority comes from the native picker, never a renderer-supplied path. */
export async function chooseAttachments(window: BrowserWindow, handle: string): Promise<FileAttachment[]> {
  const root = roots.get(handle)
  if (!root) throw new Error('Open a project before attaching files.')
  const selected = await dialog.showOpenDialog(window, {
    title: 'Choose project files to attach as trusted context',
    message: 'Selected contents will be shared with the model when you send your message.',
    defaultPath: root, properties: ['openFile', 'multiSelections'],
  })
  if (selected.canceled) return []
  const chosen = selected.filePaths.map((file) => {
    const path = relative(realpathSync(root), realpathSync(file))
    const target = isSubpath(path) && path ? inside(handle, path) : null
    if (!target || !isAttachableProjectFile(root, path)) throw new Error(`Choose text files inside this project. ${path || 'That file'} is not one.`)
    return { id: randomUUID(), path }
  })
  const tokens = attachments.get(handle) ?? new Map<string, string>()
  for (const item of chosen) tokens.set(item.id, item.path)
  attachments.set(handle, tokens)
  return chosen
}

/** What a folder add came to: nothing picked, the agent's answer, or the agent's refusal. */
export type FolderAdded = null | { ok: unknown } | { error: { code: string; message: string } }

/**
 * Ask the person for a folder to open beside `handle`'s project, and ask the agent to open it.
 *
 * The path the agent is asked about is the one the native picker returned, never one the window
 * sent, and every check of it (absolute, a directory, not open already, not the project) is the
 * agent's (TRUST-9). The folders this process lets the tree browse are the ones the agent's answer
 * lists. `add` is the call to the agent, passed in so this can be tried without one.
 */
export async function chooseFolder(window: BrowserWindow, handle: string, add: (path: string) => Promise<unknown>): Promise<FolderAdded> {
  const root = roots.get(handle)
  if (!root) return { error: { code: 'bad_request', message: 'Open a project before adding a folder.' } }
  const selected = await dialog.showOpenDialog(window, {
    title: 'Choose a folder to open beside this project',
    message: 'The agent will be able to read and change files in it for the rest of this chat.',
    defaultPath: root, properties: ['openDirectory'],
  })
  const path = selected.canceled ? undefined : selected.filePaths[0]
  if (!path) return null
  const ok = await add(path)
  noteFolders(handle, (ok as { folders?: unknown } | null)?.folders)
  return { ok }
}

export function attachmentPaths(handle: string, ids: unknown): string[] {
  if (ids === undefined) return []
  if (!Array.isArray(ids)) throw new Error('Invalid attachments.')
  return ids.map((id) => {
    const path = typeof id === 'string' ? attachments.get(handle)?.get(id) : null
    const target = path ? inside(handle, path) : null
    if (!path || !target || !isAttachableProjectFile(roots.get(handle)!, path)) throw new Error('An attachment is no longer available. Choose it again.')
    return path
  })
}

/**
 * Names Windows reads as a device rather than as a file in the folder that was listed.
 *
 * `COM1` is the serial port whatever directory it is named in, and `NUL` discards what is written
 * to it, extension and all.
 */
const WINDOWS_DEVICE = /^(con|prn|aux|nul|com[1-9]|lpt[1-9])(\..*)?$/i

/**
 * Whether every segment of a relative path names on Windows the file it appears to name.
 *
 * None of these is a way out of the project, which is what the `realpath` check below decides.
 * Each is a request that resolves to something other than what it spells: `x:stream` reads an
 * alternate data stream of `x`, a device name reaches a device, and Win32 drops a trailing dot or
 * space before it looks, so `notes. ` opens `notes`. A panel whose whole job is showing somebody
 * their own folder must not serve a path that means something else.
 *
 * Here rather than beside the other path rules in `shared/files.ts` because this is the only side
 * that knows which platform it is on: that module is shared with the renderer, which is given no
 * `process`, and applying these everywhere would cost a POSIX project every file it has with a
 * colon in its name, which is one timestamped log away. On a POSIX host every one of these is an
 * ordinary file name and resolves to itself.
 */
export function namesWhatItSpells(subpath: string, platform: NodeJS.Platform = process.platform): boolean {
  if (platform !== 'win32') return true
  return subpath
    .split(/[\\/]/)
    .every((segment) => !segment.includes(':') && !/[. ]$/.test(segment) && !WINDOWS_DEVICE.test(segment))
}

/**
 * Where a session-and-subpath actually points, or `null` if it points nowhere this may look.
 *
 * `null` covers every refusal — an unknown session, a path that does not exist, a link that leads
 * out of the project — deliberately without distinguishing them. The caller has nothing different
 * to do about any of them, and a message that told the difference would tell the renderer whether
 * a path it may not read exists.
 */
function inside(handle: string, subpath: string, folder?: unknown): string | null {
  if (!isSubpath(subpath) || !namesWhatItSpells(subpath)) return null
  const root = rootOf(handle, folder)
  if (root === undefined) return null
  try {
    const base = realpathSync(root)
    const target = subpath === '' ? base : realpathSync(join(base, subpath))
    if (target !== base && !target.startsWith(base + sep)) return null
    return target
  } catch {
    // No such path, or one this process cannot resolve. Either way there is nothing to list.
    return null
  }
}

/** Human-only raw preview. Reading here never releases content to the agent. */
export function preview(handle: string, subpath: string, folder?: unknown): FilePreview | null {
  const root = rootOf(handle, folder)
  if (!root || !isSubpath(subpath) || !subpath || !namesWhatItSpells(subpath)) return null
  const file = readProjectText(root, subpath)
  return file ? { path: subpath, ...file } : null
}

/** Search names recursively, independently of which tree branches the user expanded. */
export async function search(handle: string, query: string, hidden: boolean, folder?: unknown): Promise<FileSearch> {
  const paths: string[] = [], pending = [''], seen = new Set<string>()
  const terms = query.toLowerCase().trim().split(/\s+/).filter(Boolean)
  let examined = 0, incomplete = false
  while (pending.length && examined < 50000 && paths.length < 500) {
    const path = pending.shift()!
    const directory = inside(handle, path, folder)
    if (!directory || seen.has(directory)) continue
    seen.add(directory)
    let rows
    try { rows = await readdir(directory, { withFileTypes: true }) }
    catch { incomplete = true; continue }
    for (const row of rows) {
      if (++examined > 50000 || paths.length >= 500) { incomplete = true; break }
      if ((!hidden && row.name.startsWith('.')) || ['node_modules', '.git', 'target', 'dist'].includes(row.name)) continue
      const relative = path ? `${path}/${row.name}` : row.name
      if (row.isDirectory()) pending.push(relative)
      else if (row.isFile() && terms.every((term) => relative.toLowerCase().includes(term))) paths.push(relative)
    }
  }
  return { paths: paths.sort(), incomplete: incomplete || pending.length > 0 }
}

/** What kind of thing an entry is, with a symlink resolved to what it points at. */
function kindOf(directory: string, entry: { name: string; isDirectory(): boolean; isSymbolicLink(): boolean }): FileRow['kind'] {
  if (entry.isDirectory()) return 'directory'
  if (!entry.isSymbolicLink()) return 'file'
  try {
    return statSync(join(directory, entry.name)).isDirectory() ? 'directory' : 'file'
  } catch {
    // A broken link. Shown as a file, because that is the row that does the least: opening it
    // fails and says so, where offering to expand it would promise children that cannot exist.
    return 'file'
  }
}

/**
 * One directory of a session's project, or `null` for anything this may not look at.
 *
 * Directories first and then names, case-insensitively — the order somebody scanning for a folder
 * reads in, and the one Finder and every editor sidebar already use.
 *
 * What comes back is `parseListing`'s output rather than the array this built, the same way the
 * layout and recents writers hand on the parsed value: the one judgement about this shape runs on
 * the way out as well, so a bug here cannot put a row on screen that the renderer would refuse.
 */
export function list(handle: string, subpath: string, folder?: unknown): Listing | null {
  const target = inside(handle, subpath, folder)
  if (target === null) return null

  let entries
  try {
    entries = readdirSync(target, { withFileTypes: true })
  } catch {
    // A directory that cannot be read — permissions, or one that went away between the click and
    // the syscall. Nothing to say beyond that there is no listing.
    return null
  }

  const rows: FileRow[] = entries.map((entry) => ({
    name: entry.name,
    kind: kindOf(target, entry),
    hidden: entry.name.startsWith('.'),
  }))
  rows.sort((left, right) => {
    if (left.kind !== right.kind) return left.kind === 'directory' ? -1 : 1
    return left.name.localeCompare(right.name, undefined, { sensitivity: 'base' })
  })

  return parseListing({
    path: subpath,
    rows: rows.slice(0, ROWS_MAX),
    truncated: rows.length > ROWS_MAX,
  })
}

/**
 * Extensions of the files handed to the system's default app without a question: text, PDF,
 * images, audio and video.
 *
 * An allow list, because which files the system runs when they are opened differs by platform and
 * grows: `.command` and `.terminal` on macOS, `.desktop` on Linux, `.bat`, `.lnk` and `.js` on
 * Windows. A program a person approved with `run` can write any of them into the project, and
 * opening one runs it on the host, outside the sandbox. Source files are left out because Windows
 * runs `.js` and `.vbs` when they are opened, and HTML and SVG because the browser that opens them
 * runs the script they hold. Spreadsheets, office and OpenDocument files and RTF are left out
 * because the app that opens them updates links and fields and evaluates formulas, and a file a
 * confined program wrote carries no download mark that makes that app treat it with suspicion.
 */
const OPENS_IN_APP = new Set([
  'txt', 'md', 'markdown', 'log', 'json', 'pdf',
  'png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'tif', 'tiff', 'heic',
  'mp3', 'm4a', 'wav', 'flac', 'ogg', 'aac', 'mp4', 'm4v', 'mov', 'webm',
])

const OPEN = 'Open'
const SHOW_IN_FOLDER = 'Show in folder'
const CANCEL = 'Cancel'

/**
 * Hand a file to whichever app the system assigns its type.
 *
 * Only a regular file: a directory would open a Finder window nobody asked for, and a device or a
 * socket is not something to hand to `open(1)` at all. `shell.openPath` resolves to an empty
 * string when it worked and to a message when it did not, and that message is worth repeating —
 * somebody double-clicked and nothing happened, which is the one case here they can act on.
 *
 * A file whose extension is not on `OPENS_IN_APP`, or that is marked executable, is opened only
 * after the person answers a question the main process asks. The renderer cannot answer it. The
 * default button is Show in folder, so a stray Enter never starts a program.
 */
export async function open(handle: string, subpath: string, parent?: BrowserWindow | null, folder?: unknown): Promise<OpenOutcome> {
  const target = inside(handle, subpath, folder)
  // The same refusal for a path outside the project as for one that is not a file. The renderer
  // has no business telling those apart, and the person sees a row that would not open either way.
  if (target === null) return { status: 'failed', message: 'that is not a file in this project' }
  let mode: number
  try {
    const stat = statSync(target)
    if (!stat.isFile()) {
      return { status: 'failed', message: 'that is not a file' }
    }
    mode = stat.mode
  } catch {
    return { status: 'failed', message: 'that is not a file in this project' }
  }
  // `target` is past `realpath`, so a link is judged by the name and mode of the file it reaches.
  if ((mode & 0o111) !== 0 || !OPENS_IN_APP.has(extname(target).slice(1).toLowerCase())) {
    const options = {
      type: 'warning' as const,
      message: 'Opening this file may run it',
      detail: `"${basename(target)}" is not a text, PDF, image, audio or video file, or it is marked as a program. A program run in this project may have written it, and opening it may run it on this computer, outside the sandbox.`,
      buttons: [OPEN, SHOW_IN_FOLDER, CANCEL],
      defaultId: 1,
      cancelId: 2,
      noLink: true,
    }
    const { response } = parent ? await dialog.showMessageBox(parent, options) : await dialog.showMessageBox(options)
    if (options.buttons[response] === SHOW_IN_FOLDER) {
      await showItemInFolder(target)
      return { status: 'revealed' }
    }
    if (options.buttons[response] !== OPEN) return { status: 'cancelled' }
  }
  const failure = await openPath(target)
  return failure === '' ? { status: 'opened' } : { status: 'failed', message: failure }
}
