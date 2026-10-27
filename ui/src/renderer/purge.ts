/** What `session.purge` answers once it has deleted. */
export interface Purged {
  deleted: string[]
  failed: string[]
  held: string[]
}

const chats = (count: number) => `${count} chat${count === 1 ? '' : 's'}`
const was = (count: number) => (count === 1 ? 'was' : 'were')

/**
 * What the window says after a purge: a toast when every named chat went, and an error when any
 * stayed, since a toast here says that something worked. A held chat is one something else is
 * still using, which is a different reason from a removal that failed.
 */
export function purgeReport(answer: Purged): { problem: string } | { done: string } {
  if (answer.failed.length + answer.held.length === 0) return { done: `Deleted ${chats(answer.deleted.length)}` }
  const parts = [`${chats(answer.deleted.length)} deleted.`]
  if (answer.failed.length > 0) parts.push(`${chats(answer.failed.length)} could not be deleted and ${was(answer.failed.length)} kept.`)
  if (answer.held.length > 0) parts.push(`${chats(answer.held.length)} still in use by a background session ${was(answer.held.length)} kept.`)
  return { problem: parts.join(' ') }
}

/**
 * Whether a session this window has open in `directory` is running a turn, which the agent refuses
 * a purge for. Asked before anything is closed, so a refused purge closes nothing.
 */
export function turnRunningIn(opened: Iterable<{ summary: { directory: string }; running: boolean }>, directory: string): boolean {
  for (const item of opened) if (item.summary.directory === directory && item.running) return true
  return false
}
