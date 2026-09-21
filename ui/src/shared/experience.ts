export interface ConversationPreferences {
  botSlug: string | null
  draft: string
  scroll: number | null
  pinned: boolean
  archived: boolean
}

export interface Experience {
  conversations: Record<string, ConversationPreferences>
  density: 'comfortable' | 'compact'
  recentModels: string[]
}

export const EMPTY_CONVERSATION: ConversationPreferences = {
  botSlug: null, draft: '', scroll: null, pinned: false, archived: false,
}

export function conversationKey(directory: string, id: string): string {
  return JSON.stringify([directory, id])
}

export function parseConversation(value: unknown): ConversationPreferences {
  const v = value && typeof value === 'object' ? value as Record<string, unknown> : {}
  return {
    botSlug: typeof v.botSlug === 'string' && /^[a-z0-9-]+$/.test(v.botSlug) ? v.botSlug : null,
    draft: typeof v.draft === 'string' ? v.draft.slice(0, 200000) : '',
    scroll: typeof v.scroll === 'number' && Number.isFinite(v.scroll) && v.scroll >= 0 ? v.scroll : null,
    pinned: v.pinned === true,
    archived: v.archived === true,
  }
}

export function parseExperience(value: unknown): Experience {
  const v = value && typeof value === 'object' ? value as Record<string, unknown> : {}
  const conversations: Record<string, ConversationPreferences> = Object.create(null) as Record<string, ConversationPreferences>
  if (v.conversations && typeof v.conversations === 'object') {
    for (const [key, entry] of Object.entries(v.conversations).slice(-5000)) {
      if (key.startsWith('[') && key.length <= 10000) conversations[key] = parseConversation(entry)
    }
  }
  return { conversations, density: v.density === 'compact' ? 'compact' : 'comfortable',
    recentModels: Array.isArray(v.recentModels) ? [...new Set(v.recentModels.filter((m): m is string => typeof m === 'string' && m.length < 1000))].slice(0, 8) : [] }
}
