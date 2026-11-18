/**
 * The folders a chat has open beside its project (TRUST-9), as the window reads the agent's answers.
 *
 * Nothing here decides whether a folder may be opened or closed: the agent does, and the main
 * process lets the tree read under a folder only while the agent's last answer lists it. This is
 * the reading of those answers into what the Folders section and the tree draw.
 */
import type { RewindPoint } from '../shared/protocol'
import { t } from './i18n'

/** What an add, a close or a list answer says about the folders, where it says anything. */
export interface FoldersAnswer {
  folders: string[]
  /** The rewind points the answer left, where it carried them. An add and a close shut them all. */
  rewind?: RewindPoint[]
}

/** The folders and rewind points an answer carries. Null for an answer with no folder list. */
export function foldersAnswer(answer: unknown): FoldersAnswer | null {
  const held = answer as { folders?: unknown; rewind?: unknown } | null
  if (!held || !Array.isArray(held.folders)) return null
  const folders = held.folders.filter((each): each is string => typeof each === 'string')
  return Array.isArray(held.rewind) ? { folders, rewind: held.rewind as RewindPoint[] } : { folders }
}

/** What pressing Add folder came to, from what `window.bravebot.addFolder` resolved with. */
export type FolderAdded =
  | { kind: 'cancelled' }
  | { kind: 'added'; folder: string | null; answer: FoldersAnswer }
  | { kind: 'refused'; message: string }

export function folderAdded(result: unknown): FolderAdded {
  if (result === null || result === undefined) return { kind: 'cancelled' }
  const held = result as { ok?: unknown; error?: { message?: unknown } | null }
  if (held.error) return { kind: 'refused', message: String(held.error.message ?? t.uiFoldersNotAdded()) }
  const answer = foldersAnswer(held.ok)
  if (!answer) return { kind: 'refused', message: t.uiFoldersNotAdded() }
  const folder = (held.ok as { folder?: unknown }).folder
  return { kind: 'added', folder: typeof folder === 'string' ? folder : null, answer }
}

/** A folder's own name, the last part of its path, for a row that has the whole path as well. */
export function folderName(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean)
  return parts.at(-1) ?? path
}

/**
 * The roots the file tree draws: the project first, then each folder in the order it was opened.
 * `folder` is what the main process is told a path is relative to, and is absent for the project.
 */
export interface TreeRoot {
  /** Unique among the roots, for React and for which roots are folded. */
  key: string
  /** The whole path, for the tooltip. */
  path: string
  name: string
  folder?: string
}

export function treeRoots(project: string, folders: readonly string[]): TreeRoot[] {
  return [
    { key: '', path: project, name: folderName(project) },
    ...folders.map((folder) => ({ key: folder, path: folder, name: folderName(folder), folder })),
  ]
}
