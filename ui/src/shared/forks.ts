/**
 * Which sessions came out of which, and the one thing that decides whether a file on disk is
 * that list.
 *
 * A separate file from the recents and the layout, for the reason each of those gives about the
 * other: a validator's job is to be the single judgement about *one* shape, and folding two into
 * a file means a bad entry in either costs somebody the other.
 *
 * This exists at all because the agent's own record cannot hold it. `Record` has no field for a
 * parent, adding one is a change to a repository this app does not modify, and the agent rewrites
 * the whole record after every turn — so a key smuggled in beside it would not survive the fork's
 * first reply. See `docs/phase-0-rpc-protocol.md` §7.1.
 */

/** One session, durably: an id is unique only within the project it ran in. */
export interface SessionRef {
  directory: string
  id: string
}

export interface Fork {
  child: SessionRef
  parent: SessionRef
  /**
   * Which prompt of the parent the child was cut in front of, counted over what the transcript
   * drew. The same coordinate `session.fork` cut on, which is what lets a link land on the row.
   */
  prompt: number
  /** When the fork was taken, in milliseconds. */
  at: number
}

export interface StoredForks {
  forks: Fork[]
}

/**
 * Enough that a working history of forks survives, few enough that the file stays a file.
 *
 * Unlike the recents list this is not a menu, so the cap is not about what fits on screen. It is
 * about a list that only ever grows: a fork is never un-forked, and something has to stop it.
 */
export const FORKS_MAX = 500

/**
 * What is *not* here: the parent's title.
 *
 * It is user-authored text the window already holds, fresher, in the session list it reads from
 * the agent — and a session can be renamed. A copy on disk would be a second answer to a question
 * that already has one, and it would be the wrong one.
 */
export function isSessionId(value: unknown): value is string {
  return typeof value === 'string' && value.length > 0 && !value.includes('/') && !value.includes('\0')
}

function isProjectPath(value: unknown): value is string {
  return typeof value === 'string' && value.startsWith('/') && !value.includes('\0')
}

function parseRef(value: unknown): SessionRef | null {
  if (typeof value !== 'object' || value === null) return null
  const { directory, id } = value as Record<string, unknown>
  if (!isProjectPath(directory) || !isSessionId(id)) return null
  return { directory, id }
}

/**
 * A fork list, always.
 *
 * Entries are filtered rather than the file refused, the way the recents list is: one unreadable
 * line should not cost every other session its banner. Nothing is coerced, and a child appears
 * once — it has exactly one parent, and a file saying otherwise is a file that has been edited.
 */
export function parseForks(value: unknown): StoredForks {
  if (typeof value !== 'object' || value === null) return { forks: [] }
  const { forks } = value as { forks?: unknown }
  if (!Array.isArray(forks)) return { forks: [] }

  const seen = new Set<string>()
  const kept: Fork[] = []
  for (const entry of forks) {
    if (typeof entry !== 'object' || entry === null) continue
    const { child, parent, prompt, at } = entry as Record<string, unknown>
    const childRef = parseRef(child)
    const parentRef = parseRef(parent)
    if (!childRef || !parentRef) continue
    if (typeof prompt !== 'number' || !Number.isInteger(prompt) || prompt < 0) continue
    if (typeof at !== 'number' || !Number.isFinite(at)) continue

    const key = keyOf(childRef.directory, childRef.id)
    if (seen.has(key)) continue
    seen.add(key)
    kept.push({ child: childRef, parent: parentRef, prompt, at })
    if (kept.length === FORKS_MAX) break
  }
  return { forks: kept }
}

/** The list with `fork` at the front, however many times its child appeared before. */
export function withFork(forks: Fork[], fork: Fork): Fork[] {
  const key = keyOf(fork.child.directory, fork.child.id)
  return [fork, ...forks.filter((old) => keyOf(old.child.directory, old.child.id) !== key)].slice(
    0,
    FORKS_MAX,
  )
}

/** How a session is named in a set or a map: the pair, since an id alone is not unique. */
export function keyOf(directory: string, id: string): string {
  return `${directory}/${id}`
}

/** Where this session was cut from, if it was. */
export function forkOf(forks: Fork[], directory: string, id: string): Fork | null {
  return (
    forks.find((fork) => fork.child.directory === directory && fork.child.id === id) ?? null
  )
}

/** Every session in this list that came out of another one. */
export function forkedSessions(forks: Fork[]): Set<string> {
  return new Set(forks.map((fork) => keyOf(fork.child.directory, fork.child.id)))
}

/**
 * A fork read out of what `session.fork` answered.
 *
 * The agent is not an adversary here, but this is still a boundary and the rest of the app
 * treats one the same way everywhere: what gets written down is composed from fields that were
 * understood, never the object that arrived. A response this cannot read leaves no line, which
 * costs a banner and nothing else.
 */
export function parseForkResult(value: unknown): Fork | null {
  if (typeof value !== 'object' || value === null) return null
  const { id, directory, parent } = value as Record<string, unknown>
  const child = parseRef({ id, directory })
  if (!child) return null

  if (typeof parent !== 'object' || parent === null) return null
  const { prompt } = parent as Record<string, unknown>
  const from = parseRef(parent)
  if (!from) return null
  if (typeof prompt !== 'number' || !Number.isInteger(prompt) || prompt < 0) return null

  return { child, parent: from, prompt, at: Date.now() }
}
