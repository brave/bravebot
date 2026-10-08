/**
 * What the composer has staged beside its draft, and the markers that stand for it in the text.
 *
 * The terminal's model (`Session::drop_files`, `attachments_named`): each staged thing gets a
 * numbered marker written at the caret, `[Image #1]`, and the draft is the only record of what the
 * person still wants. What is sent is what the draft still names, so deleting a marker takes its
 * file off and undoing the delete puts it back (DROP-6). Staged entries are never pruned while the
 * draft is edited, only when it is sent.
 *
 * One counter numbers everything staged in a composer, so no two markers share a number and a
 * number is never reused. A pasted picture is numbered from the same counter.
 */

import type { DropOutcome, DroppedFile } from '../shared/drops'

/** One staged thing and the marker that stands for it. Only drops for now; a paste is the next. */
export type Staged = { marker: string; via: 'drop'; file: DroppedFile }

export interface Staging {
  /** How many markers this composer has ever written. */
  made: number
  staged: Staged[]
}

export const EMPTY_STAGING: Staging = { made: 0, staged: [] }

/** What a drop did to the composer: the new draft, where the caret goes, and what to tell. */
export interface StagedDrop {
  staging: Staging
  draft: string
  caret: number
  skipped: Extract<DropOutcome, { kind: 'skipped' }>[]
}

/**
 * Write a drop into `draft` at `caret`: a marker for each file taken and the path for each type
 * nothing takes, in the order they were dropped, separated by spaces and followed by one so what
 * is typed next does not run into the last marker.
 */
export function stageDrop(staging: Staging, outcomes: DropOutcome[], draft: string, caret: number): StagedDrop {
  let made = staging.made
  const staged = [...staging.staged]
  const written: string[] = []
  const skipped: StagedDrop['skipped'] = []
  for (const outcome of outcomes) {
    if (outcome.kind === 'skipped') {
      skipped.push(outcome)
    } else if (outcome.kind === 'named') {
      written.push(outcome.text)
    } else {
      made += 1
      const marker = `[${outcome.file.noun} #${made}]`
      staged.push({ marker, via: 'drop', file: outcome.file })
      written.push(marker)
    }
  }
  if (written.length === 0) return { staging, draft, caret, skipped }
  const at = Math.max(0, Math.min(caret, draft.length))
  const before = draft.slice(0, at)
  const lead = before.length > 0 && !/\s$/.test(before) ? ' ' : ''
  const text = `${lead}${written.join(' ')} `
  return {
    staging: { made, staged },
    draft: before + text + draft.slice(at),
    caret: at + text.length,
    skipped,
  }
}

/** What `text` still names, in the order it was staged. */
export function named(staging: Staging | undefined, text: string): Staged[] {
  return (staging?.staged ?? []).filter((item) => text.includes(item.marker))
}

/** The draft with `marker` taken out, along with the one space written after it. */
export function withoutMarker(draft: string, marker: string): string {
  return draft.split(`${marker} `).join('').split(marker).join('')
}

/** After a send: nothing staged, and the counter where it was so no number comes round again. */
export function sent(staging: Staging | undefined): Staging {
  return { made: staging?.made ?? 0, staged: [] }
}
