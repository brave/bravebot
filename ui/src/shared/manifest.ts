/**
 * The task of a line that is `/manifest` or `/manifest <task>`, or null for any other line.
 *
 * `/manifest` alone gives `''`, which the caller refuses with the terminal's note that a run needs
 * a task. Applied only to the text a person typed into the composer (CMD-1). A prompt from the
 * queue of another kind, a template, a file or the model's output never goes through it, so only
 * a person starts a plan-then-execute run, and the session gets its turn loop back once the run
 * ends (MANIFEST-9). A longer word such as `/manifestx` is a prompt, as it is in the terminal.
 */
export function manifestTask(line: string): string | null {
  const rest = line.trim()
  if (rest === '/manifest') return ''
  if (!rest.startsWith('/manifest ')) return null
  return rest.slice('/manifest '.length).trim()
}
