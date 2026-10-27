/** What `session.purge` answers once it has deleted. */
export interface Purged {
  deleted: string[]
  failed: string[]
  held: string[]
}

const chats = (count: number) => `${count} chat${count === 1 ? '' : 's'}`

/**
 * What the window says after a purge: a toast when every named chat went, and an error when any
 * stayed, since a toast here says that something worked.
 */
export function purgeReport(answer: Purged): { problem: string } | { done: string } {
  const left = answer.failed.length + answer.held.length
  if (left > 0) return { problem: `${chats(answer.deleted.length)} deleted. ${chats(left)} could not be deleted and ${left === 1 ? 'was' : 'were'} kept.` }
  return { done: `Deleted ${chats(answer.deleted.length)}` }
}
