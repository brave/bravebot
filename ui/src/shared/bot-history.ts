import type { Bot } from './bots'
import { conversationKey, type Experience } from './experience'
import type { SessionSummary } from './protocol'

export interface BotConversation {
  id: string
  session: SessionSummary | null
  archived: boolean
}

/** Keep saved history, live conversations and associated drafts together, including archives. */
export function botHistory(bot: Bot, sessions: SessionSummary[], experience: Experience): BotConversation[] {
  const ids = new Set([...bot.conversations, ...(bot.session ? [bot.session] : [])])
  for (const [key, preference] of Object.entries(experience.conversations)) {
    if (preference.botSlug !== bot.slug) continue
    try {
      const [directory, id] = JSON.parse(key)
      if (directory === bot.directory && typeof id === 'string' && !id.startsWith('draft:')) ids.add(id)
    } catch { /* Ignore invalid preference keys. */ }
  }
  const available = new Map<string, SessionSummary>()
  for (const session of sessions) {
    if (session.directory !== bot.directory) continue
    const preference = experience.conversations[conversationKey(session.directory, session.id)]
    if (ids.has(session.id) || preference?.botSlug === bot.slug) {
      ids.add(session.id)
      available.set(session.id, session.id.startsWith('draft:') && preference?.draft.trim()
        ? { ...session, title: `Draft · ${preference.draft.slice(0, 60)}` } : session)
    }
  }
  return [...ids].map(id => ({ id, session: available.get(id) ?? null,
    archived: experience.conversations[conversationKey(bot.directory, id)]?.archived ?? false,
  })).sort((a, b) => (b.session?.updated ?? -1) - (a.session?.updated ?? -1))
}
