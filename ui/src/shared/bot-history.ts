import type { Bot } from './bots'
import { conversationKey, type Experience } from './experience'
import type { SessionSummary } from './protocol'

export interface BotConversation {
  id: string
  /** The folder the conversation runs in: the bot's home, or a project. */
  directory: string
  session: SessionSummary | null
  archived: boolean
}

/** Keep saved history, live conversations and associated drafts together, including archives. */
export function botHistory(bot: Bot, sessions: SessionSummary[], experience: Experience): BotConversation[] {
  const known = new Map<string, { id: string; directory: string }>()
  const add = (directory: string, id: string) => known.set(conversationKey(directory, id), { id, directory })
  for (const each of bot.conversations) add(each.directory, each.id)
  for (const [key, preference] of Object.entries(experience.conversations)) {
    if (preference.botSlug !== bot.slug) continue
    try {
      const [directory, id] = JSON.parse(key)
      if (typeof directory === 'string' && typeof id === 'string' && !id.startsWith('draft:')) add(directory, id)
    } catch { /* Ignore invalid preference keys. */ }
  }
  const available = new Map<string, SessionSummary>()
  for (const session of sessions) {
    const key = conversationKey(session.directory, session.id)
    const preference = experience.conversations[key]
    if (known.has(key) || preference?.botSlug === bot.slug) {
      add(session.directory, session.id)
      available.set(key, session.id.startsWith('draft:') && preference?.draft.trim()
        ? { ...session, title: `Draft · ${preference.draft.slice(0, 60)}` } : session)
    }
  }
  return [...known].map(([key, { id, directory }]) => ({ id, directory, session: available.get(key) ?? null,
    archived: experience.conversations[key]?.archived ?? false,
  })).sort((a, b) => (b.session?.updated ?? -1) - (a.session?.updated ?? -1))
}
