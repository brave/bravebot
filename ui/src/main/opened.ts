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
