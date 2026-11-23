/**
 * The task of a line that is `/plan` or `/plan <task>`: `''` for `/plan` alone, the trimmed task
 * otherwise, or null for any other line.
 *
 * Applied only to the text a person typed into the composer (CMD-1). A prompt from the queue of
 * another kind, a template, a file or the model's output never goes through it. A longer word such
 * as `/planned`, or `/plan` in the middle of a line, is a prompt, as it is in the terminal. The
 * line sets plan mode, and only the task is sent as the turn's prompt (MODE-12).
 */
export function planTask(line: string): string | null {
  const rest = line.trim()
  if (rest === '/plan') return ''
  if (!rest.startsWith('/plan ')) return null
  return rest.slice('/plan '.length).trim()
}
