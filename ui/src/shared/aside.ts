/** What the bridge answers for `session.btw` and `session.recap`: the question is out, and its answer follows as `aside.done`. */
export interface AsideStarted {
  started: boolean
  /** True for a `/btw` asked over the exchange of the turn that runs, rather than the conversation at rest (CMD-8). */
  besideTurn?: boolean
}

/** What a `/btw` or `/recap` came back with, as the bridge sends it in `aside.done`. Shown, never sent to the model. */
export interface AsideDone {
  status: 'answered' | 'stopped' | 'failed'
  /** The row's label: the person's question, or `/recap` for a recap. */
  question: string
  recap: boolean
  answer?: string
  /** Whether the session's record keeps the answer. False where the exchange met something untrusted (WATCH-20). */
  kept?: boolean
  message?: string
}

/** One aside of the session: a question and its answer, `answer` null where the record did not keep it. */
export interface AsideRow {
  question: string
  answer: string | null
  kept: boolean
}

/**
 * The question of a line that is `/btw` or `/btw <question>`, or null for any other line.
 *
 * Applied only to the text a person typed into the composer (CMD-1). A longer word such as `/btwx`
 * is a prompt, as it is in the terminal (CMD-2). `/btw` alone gives an empty question, which the
 * bridge refuses with the terminal's sentence.
 */
export function btwQuestion(line: string): string | null {
  const rest = line.trim()
  if (rest === '/btw') return ''
  if (!rest.startsWith('/btw ')) return null
  return rest.slice('/btw '.length).trim()
}

/** Whether a line a person typed is `/recap` and nothing else (CMD-1, CMD-2). `/recap now` is a prompt. */
export function isRecap(line: string): boolean {
  return line.trim() === '/recap'
}
