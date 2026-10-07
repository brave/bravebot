import type { RewindGap, RewindPoint } from '../shared/protocol'
import type { Entry } from './transcript'

/** What a rewind of `steps` turns would do, worked out before anybody is asked to confirm it. */
export interface RewindPlan {
  steps: number
  /** The earliest turn it undoes: the session will stand before this one. */
  turn: number
  /** Every file it puts back, each once, in the order the agent listed them. */
  paths: string[]
  /** One sentence per kind of effect the backups do not cover. */
  warnings: string[]
}

const GAPS: Record<RewindGap, string> = {
  command: 'Commands that ran may have changed files that won’t be restored.',
  hook: 'Hooks that ran may have changed files that won’t be restored.',
  scratch: 'Files written to the scratch area won’t be restored.',
  'language-server': 'A language server may have changed files that won’t be restored.',
  desktop: 'Some work in this chat kept no backups, such as a plan run, and won’t be restored.',
  'backup-unavailable': 'Some files could not be backed up and won’t be restored.',
  unknown: 'Some changes may not be restored.',
}

/** The sentence for a gap. One this build has no words for reads as the unknown one. */
export function gapWarning(gap: string): string {
  return Object.hasOwn(GAPS, gap) ? GAPS[gap as RewindGap] : GAPS.unknown
}

/** Everything a rewind of `steps` turns covers, or `null` where the session holds no such point. */
export function planRewind(points: readonly RewindPoint[], steps: number): RewindPlan | null {
  const target = points.find((point) => point.steps === steps)
  if (!target) return null
  const covered = points.filter((point) => point.steps <= steps)
  const paths = [...new Set(covered.flatMap((point) => point.paths))]
  const warnings = [...new Set(covered.flatMap((point) => point.gaps).map(gapWarning))]
  return { steps, turn: target.turn, paths, warnings }
}

/**
 * The point whose turn a drawn prompt began, so a right-click on it can rewind to before it.
 *
 * Matched on the agent's ordinal and never on text or position, for the reason a fork is: two
 * prompts can say the same thing, and a prompt this window has just sent has no ordinal yet.
 */
export function pointForPrompt(points: readonly RewindPoint[], prompt: number | undefined): RewindPoint | null {
  if (typeof prompt !== 'number') return null
  return points.find((point) => point.prompt === prompt) ?? null
}

/**
 * The row whose footer offers Undo turn: the latest turn's, and only while the newest point is
 * that turn's.
 *
 * A reply drawn live carries its turn number. One drawn from a saved conversation does not, so it
 * is matched through the prompt above it instead.
 */
export function undoRow(entries: readonly Entry[], points: readonly RewindPoint[]): string | null {
  const newest = points[0]
  if (!newest) return null
  let at = entries.length - 1
  while (at >= 0) {
    const entry = entries[at]!
    if (entry.kind === 'assistant' || (entry.kind === 'error' && entry.turn !== undefined)) break
    at--
  }
  if (at < 0) return null
  const row = entries[at]!
  if (row.turn !== undefined) return row.turn === newest.turn ? row.id : null
  for (let above = at - 1; above >= 0; above--) {
    const entry = entries[above]!
    if (entry.kind === 'user') return newest.prompt !== null && entry.prompt === newest.prompt ? row.id : null
  }
  return null
}

/** A request the bridge answered with an error. The code is kept so a caller decides on it, not on the wording. */
export class RequestFailed extends Error {
  readonly code: string

  constructor(code: string, message: string) {
    super(`${code}: ${message}`)
    this.code = code
  }
}

/** What to tell a person whose rewind failed. A turn that started in the meantime is not a fault. */
export function rewindFailure(error: unknown): string {
  return error instanceof RequestFailed && error.code === 'turn_in_flight'
    ? 'A turn started, so nothing was undone. Try again once it finishes.'
    : String(error)
}
