import type { AskAnswer } from '../shared/protocol'

/**
 * Where a person is in a series of questions: the one being put, and what each earlier one was
 * answered with. The questions are put one at a time (ASK-6), so this is all the window holds
 * about a series until the last of them is settled.
 */
export interface Step {
  at: number
  settled: AskAnswer[]
}

/** The first question of a series, with nothing settled. */
export const first: Step = { at: 0, settled: [] }

/** What a person has settled and entered so far in a series that is still being put. */
export interface Held {
  step: Step
  chosen: number[]
  typed: string
}

/**
 * Progress through the series each entry draws, kept by the entry's id.
 *
 * The card holds its state locally, and a card is drawn again from nothing when the person
 * returns to its conversation from another one. Without this they would be back at the first
 * question with what they had settled gone.
 */
const held = new Map<string, Held>()

/** The progress kept for an entry's series, or the start of one. */
export function recall(id: string): Held {
  return held.get(id) ?? { step: first, chosen: [], typed: '' }
}

export function keep(id: string, progress: Held): void {
  held.set(id, progress)
}

/** Drop what was kept for an entry whose series was answered or ended, which has no progress left. */
export function release(id: string): void {
  held.delete(id)
}

/** The next question, or, once the last is settled, every answer in the order the questions were put. */
export type Next = { step: Step } | { answers: AskAnswer[] }

/**
 * Take the answer to the question being put and move on, finishing when there is none left.
 *
 * Whether the series is over depends on how many questions it holds and never on what any of them
 * was answered with, so skipping a question and answering it are the same operation.
 */
export function settle(step: Step, answer: AskAnswer, total: number): Next {
  const settled = [...step.settled, answer]
  return settled.length >= total ? { answers: settled } : { step: { at: step.at + 1, settled } }
}

/**
 * What the question being put is answered with.
 *
 * Typed words win over a selection, matching what the agent does with a reply that carries both:
 * they are the more specific thing to have done. Neither is an empty answer, which is how a
 * question is skipped.
 */
export function answerOf(chosen: readonly number[], typed: string): AskAnswer {
  const words = typed.trim()
  if (words) return { typed: words }
  return chosen.length > 0 ? { chosen: [...chosen] } : {}
}

/**
 * Where the question at `at` (counted from zero) stands in a series of `total`, or `null` for a
 * lone question, which has no position worth saying.
 */
export function position(at: number, total: number): string | null {
  return total > 1 ? `Question ${at + 1} of ${total}` : null
}
