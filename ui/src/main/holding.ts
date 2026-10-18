import type { BridgeEvent } from '../shared/protocol'

/** The session `message` begins or ends work on, if it does either. */
export function holding(message: BridgeEvent): { session: string; begins: boolean } | null {
  const session = message.session
  if (!session) return null
  if (message.event === 'turn.started') return { session, begins: true }
  // A compaction asked for outside a turn has no `turn.started`; its first word is this phase.
  if (message.event === 'phase' && message.data.phase === 'compacting') return { session, begins: true }
  if (message.event === 'turn.done' || message.event === 'turn.error' || message.event === 'compact.done' || message.event === 'compact.error') return { session, begins: false }
  return null
}

/**
 * What a compaction somebody asked for did to the archive of the bot `botOf` finds behind its
 * session, and whether it rose, which owes the bot a consolidation. `null` for anything else.
 */
export function compactedBot(
  message: BridgeEvent,
  botOf: (handle: string) => { slug: string; archived: number } | null,
): { handle: string; slug: string; archived: number; consolidate: boolean } | null {
  if (message.event !== 'compact.done' || !message.data.compacted || typeof message.session !== 'string') return null
  const archived = message.data.archived
  if (archived === undefined) return null
  const before = botOf(message.session)
  if (!before) return null
  return { handle: message.session, slug: before.slug, archived, consolidate: archived > before.archived }
}
