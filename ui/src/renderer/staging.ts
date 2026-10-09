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
 * number is never reused. A pasted picture is numbered from the same counter, as the terminal's
 * `attachments_made` numbers both (PASTE-6).
 */

import type { DropOutcome, DroppedFile } from '../shared/drops'
import type { PastedPicture } from '../shared/pastes'

/** One staged thing and the marker that stands for it. */
export type Staged =
  | { marker: string; via: 'drop'; file: DroppedFile }
  | { marker: string; via: 'paste'; picture: PastedPicture }

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
  return { staging: { made, staged }, ...writtenAt(draft, caret, written), skipped }
}

/** What a paste did to the composer: the new draft and where the caret goes. */
export type StagedPaste = Omit<StagedDrop, 'skipped'>

/** Write a pasted picture's marker into `draft` at `caret`, numbered from the counter drops share. */
export function stagePaste(staging: Staging, picture: PastedPicture, draft: string, caret: number): StagedPaste {
  const made = staging.made + 1
  const marker = `[${picture.noun} #${made}]`
  return { staging: { made, staged: [...staging.staged, { marker, via: 'paste', picture }] }, ...writtenAt(draft, caret, [marker]) }
}

function writtenAt(draft: string, caret: number, written: string[]): { draft: string; caret: number } {
  const at = Math.max(0, Math.min(caret, draft.length))
  const before = draft.slice(0, at)
  const lead = before.length > 0 && !/\s$/.test(before) ? ' ' : ''
  const text = `${lead}${written.join(' ')} `
  return { draft: before + text + draft.slice(at), caret: at + text.length }
}

/** The grants a send names, split the way the main process takes them: `drops` and `pastes`. */
export function grantsOf(items: Staged[]): { drops: string[]; pastes: string[] } {
  return {
    drops: items.flatMap((item) => item.via === 'drop' ? [item.file.id] : []),
    pastes: items.flatMap((item) => item.via === 'paste' ? [item.picture.id] : []),
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
