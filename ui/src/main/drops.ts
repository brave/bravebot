/**
 * Grants for files a person dropped on the window.
 *
 * A drop is the one gesture that may hand the agent a file outside the project (DROP-3), so where
 * its path comes from is the whole of its justification (DROP-1). The path is taken in the preload,
 * from a drop event the browser marked trusted, and comes straight here. The page is never told it.
 * What the page gets back is an opaque id bound to the session the file was dropped on, and a name
 * to show, the way the native picker's grants work in `files.ts`.
 *
 * At send the page names ids, never paths, and each id is checked again: the file is still there,
 * still a regular file, and a picture or PDF is still within the agent's cap. A grant that fails
 * refuses the send with a message saying which file, so the turn never starts without it (DROP-10).
 */

import { randomUUID } from 'node:crypto'
import { lstatSync, realpathSync } from 'node:fs'
import { basename, isAbsolute } from 'node:path'
import { MAX_ATTACHMENT_BYTES, kindOf, type DropKind, type DropOutcome } from '../shared/drops'
import { rootForSession } from './files'

interface Grant {
  /** The dropped path resolved through any links, so a link moved later cannot redirect it. */
  path: string
  name: string
  kind: DropKind
}

const grants = new Map<string, Map<string, Grant>>()

/** More than anybody drags at once; a longer list is not a drop. */
const MOST_IN_ONE_DROP = 100

/** A send refused because of what it carried, in words the person can act on. */
export class SendRefused extends Error {}

/**
 * Grant each dropped path to `session`, in the order they were dropped.
 *
 * Only a session this process confirmed (one with a root in `files.ts`) can hold a grant, so a drop
 * cannot outlive or precede the session it was meant for.
 */
export function stageDrops(session: unknown, paths: unknown): DropOutcome[] {
  if (typeof session !== 'string' || rootForSession(session) === undefined) return []
  if (!Array.isArray(paths) || paths.length > MOST_IN_ONE_DROP) return []
  const held = grants.get(session) ?? new Map<string, Grant>()
  grants.set(session, held)
  return paths.map((path): DropOutcome => {
    if (typeof path !== 'string' || !isAbsolute(path) || path.includes('\0')) {
      return { kind: 'skipped', name: typeof path === 'string' ? basename(path) : '', why: 'unreadable' }
    }
    const name = basename(path)
    let real: string
    try {
      real = realpathSync(path)
    } catch {
      return { kind: 'skipped', name, why: 'unreadable' }
    }
    const found = regularFile(real)
    if (found === 'folder') return { kind: 'skipped', name, why: 'folder' }
    if (found === null) return { kind: 'skipped', name, why: 'unreadable' }
    // Classified by the name the agent will check, which is the resolved one: a link named
    // `shot.png` pointing at `blob` would otherwise pass here and be refused at send.
    const kind = kindOf(real)
    if (kind === null) return { kind: 'named', text: path }
    if (kind !== 'text' && found > MAX_ATTACHMENT_BYTES) return { kind: 'skipped', name, why: 'too-large' }
    const id = randomUUID()
    held.set(id, { path: real, name, kind })
    return { kind: 'staged', file: { id, name, kind } }
  })
}

/**
 * The size of a regular file at a resolved path, `'folder'` for a directory, or `null` otherwise.
 *
 * `lstat` rather than `stat`: the path was resolved when it was granted, so a link standing there
 * now was put there since, and is refused rather than followed.
 */
function regularFile(real: string): number | 'folder' | null {
  try {
    const found = lstatSync(real)
    if (found.isDirectory()) return 'folder'
    return found.isFile() ? found.size : null
  } catch {
    return null
  }
}

/**
 * The paths `ids` grant in `session`, split the way `turn.send` takes them: text files in
 * `dropped`, pictures and PDFs in `attachments`, each in the order the ids were given.
 *
 * Throws `SendRefused` for anything that is not a live grant of this session or no longer names
 * what was dropped.
 */
export function dropsFor(session: string, ids: unknown): { dropped: string[]; attachments: string[] } {
  const composed = { dropped: [] as string[], attachments: [] as string[] }
  if (ids === undefined || ids === null) return composed
  if (!Array.isArray(ids) || ids.length > MOST_IN_ONE_DROP) throw new SendRefused('The dropped files could not be read. Drop them again.')
  const held = grants.get(session)
  for (const id of ids) {
    const grant = typeof id === 'string' ? held?.get(id) : undefined
    if (!grant) throw new SendRefused('A dropped file is no longer available. Drop it again.')
    const size = regularFile(grant.path)
    if (typeof size !== 'number') throw new SendRefused(`${grant.name} is no longer there. Drop it again.`)
    if (grant.kind === 'text') composed.dropped.push(grant.path)
    else if (size > MAX_ATTACHMENT_BYTES) throw new SendRefused(`${grant.name} is larger than 8 MB, too large to send.`)
    else composed.attachments.push(grant.path)
  }
  return composed
}

/**
 * `params` carrying what `ids` grant: text files after any `dropped` already there, which is where a
 * bot's briefing is, and pictures and PDFs as `attachments`. A list with nothing in it is left off.
 */
export function withDrops(session: string, ids: unknown, params: Record<string, unknown>): Record<string, unknown> {
  const { dropped, attachments } = dropsFor(session, ids)
  const already = Array.isArray(params.dropped) ? (params.dropped as string[]) : []
  const carried = { ...params }
  if (already.length + dropped.length > 0) carried.dropped = [...already, ...dropped]
  if (attachments.length > 0) carried.attachments = attachments
  return carried
}

/** Forget a closed session's grants. */
export function forgetDrops(session: string): void {
  grants.delete(session)
}

