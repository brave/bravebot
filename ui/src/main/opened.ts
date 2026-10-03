/**
 * Which folders somebody has pointed at, and the one thing that decides whether a path a window
 * names is one of them.
 *
 * A window is handed a folder by the native picker and hands it back on the next call, when it
 * names the project a new bot works in. What it must not be able to do is invent one. A folder this app accepts becomes the directory a confined file helper is pinned to, and
 * the grounds for reading and writing there are that somebody opened the place: `isProjectPath`
 * next door decides the shape of a string and nothing about whose folder it names, so a channel
 * checking that alone accepts every folder on the account.
 *
 * So the picker lives here rather than in the channel that offers it, and what it handed over is
 * remembered. That is the arrangement `chooseAttachments` already makes about files one level
 * down: authority comes from the dialog, never from an argument.
 *
 * In memory and for the life of the run. The recents list is deliberately not this: a window can
 * put a folder on that list by opening a session in it, so a check against the list would be a
 * check the window can answer for itself.
 */

import { dialog, type BrowserWindow } from 'electron'
import { isProjectPath } from '../shared/recents'

const chosen = new Set<string>()

/**
 * Folders this process itself put in front of the window: the recents list it keeps, and the
 * directories the agent reported on a session. A window may name one of these to open a
 * session, which is the other half of "what it may name is decided by what it has been given".
 *
 * Kept apart from `chosen` because a bot's folder has a stricter source (the picker alone), and
 * folding the two would widen that road.
 */
const offered = new Set<string>()

/**
 * Ask for a project folder, and remember what came back.
 *
 * `null` for a cancelled dialog, which is not a failure: somebody changed their mind, and nothing
 * was opened, so nothing is remembered either.
 */
export async function chooseDirectory(window: BrowserWindow): Promise<string | null> {
  const result = await dialog.showOpenDialog(window, {
    title: 'Open a project',
    properties: ['openDirectory', 'createDirectory'],
  })
  if (result.canceled) return null
  const directory = result.filePaths[0]
  if (!isProjectPath(directory)) return null
  chosen.add(directory)
  return directory
}

/** Whether a window is naming a folder this process handed it, rather than one it composed. */
export function isOpenedDirectory(value: unknown): value is string {
  return isProjectPath(value) && chosen.has(value)
}

/** Record folders this process is about to hand to the window, from a list it composed. */
export function offerDirectories(values: unknown): void {
  if (!Array.isArray(values)) return
  for (const value of values) if (isProjectPath(value)) offered.add(value)
}

/**
 * Whether a window may open a session in this folder: the picker handed it over, or this process
 * offered it from a list of its own. The shape of the string decides nothing.
 */
export function mayOpenSessionIn(value: unknown): value is string {
  return isOpenedDirectory(value) || (isProjectPath(value) && offered.has(value))
}
