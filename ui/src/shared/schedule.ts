/** What the bridge answers for `schedule.command`, and sends as `schedule.ran` when a run-now ends. */
export interface ScheduleAnswer {
  /** Each line already has its control characters pictured by the bridge. */
  lines: string[]
  /** True when the command ended in a refusal or a failure, as the shared command reported its outcome; it is not worked out from the text of a line. The renderer draws such an answer as an error note. */
  refused: boolean
  /** True for run-now: the answer is the one-line notice that the run started, and the result follows as `schedule.ran`. */
  running: boolean
}

/**
 * The argument of a line that is `/schedule` or `/schedule <argument>`, or null for any other line.
 *
 * Applied only to the text a person typed into the composer (CMD-1). A prompt from the queue of
 * another kind, a template, a file or the model's output never goes through it. A longer word such
 * as `/scheduled` is a prompt, as it is in the terminal.
 */
export function scheduleArgument(line: string): string | null {
  const rest = line.trim()
  if (rest === '/schedule') return ''
  if (!rest.startsWith('/schedule ')) return null
  return rest.slice('/schedule '.length).trim()
}
