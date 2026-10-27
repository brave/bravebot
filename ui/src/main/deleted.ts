import { isSessionId } from '../shared/forks'

/**
 * The conversations an answer from the agent says it removed, among the ones the window named.
 *
 * Read off both halves: the agent's answer says what went, and the window's request bounds it, so
 * an answer cannot make this app forget a conversation nobody asked to delete.
 */
export function deletedConversations(method: string, params: unknown, ok: unknown): { directory: string; id: string }[] {
  const asked = (params ?? {}) as { directory?: unknown; id?: unknown; ids?: unknown }
  if (typeof asked.directory !== 'string') return []
  const directory = asked.directory
  if (method === 'session.delete') {
    const deleted = (ok as { deleted?: unknown } | null)?.deleted === true
    return deleted && isSessionId(asked.id) ? [{ directory, id: asked.id }] : []
  }
  if (method === 'session.purge') {
    const deleted = (ok as { deleted?: unknown } | null)?.deleted
    if (!Array.isArray(asked.ids) || !Array.isArray(deleted)) return []
    const named: unknown[] = asked.ids
    const ids = deleted.filter((id): id is string => isSessionId(id) && named.includes(id))
    return [...new Set(ids)].map((id) => ({ directory, id }))
  }
  return []
}
