import type { SessionSummary } from '../shared/protocol'
import type { Bot } from '../shared/bots'
import { conversationKey, type ConversationPreferences } from '../shared/experience'
import { projectLabel } from '../shared/recents'
import type { SessionInfoValue, SessionStatus } from './components/Sessions'
import * as t from './transcript'
import type { Live } from './live'

/** Whether a bot has recorded this conversation, in the folder it ran in. */
export function hadConversation(bot: Bot, summary: { id: string; directory: string }): boolean {
  return bot.conversations.some((each) => each.id === summary.id && each.directory === summary.directory)
}

/** What a chat in a bot's home folder is listed under. */
export const NO_PROJECT = 'No project'

interface Listing {
  sessions: SessionSummary[]
  /** The sessions the window has opened, whose titles and state are fresher than the record's. */
  opened: Live[]
  /** `preferences.conversations`, keyed by `conversationKey`. */
  conversations: Record<string, ConversationPreferences>
  bots: Bot[]
  /** The time unrecorded sessions are stamped with, already rounded to the minute. */
  now: number
  /** The key of the session on screen, which is not counted as needing attention. */
  currentKey: string
}

/** The sidebar's rows, what each row says about its session, and how many sessions ask something of the reader. */
export function listSessions({ sessions, opened, conversations, bots, now, currentKey }: Listing): {
  ownSessions: SessionSummary[]
  info: Record<string, SessionInfoValue>
  attention: number
} {
  // A bot's home folder is not a project, so a conversation there is listed as having none.
  const homes = new Set(bots.map((bot) => bot.home))
  const placed = (summary: SessionSummary): SessionSummary =>
    homes.has(summary.directory) ? { ...summary, project: NO_PROJECT, branch: null } : summary
  const ownSessions = sessions.map((summary) => {
    const current = opened.find((item) => item.summary.id === summary.id && item.summary.directory === summary.directory)
    return placed(current ? { ...summary, title: current.summary.title } : summary)
  })
  for (const current of opened) {
    const id = current.summary.id ?? current.draftId
    if (!id || ownSessions.some((summary) => summary.id === id && summary.directory === current.summary.directory)) continue
    ownSessions.unshift(placed({ ...current.summary, id, updated: now, bytes: 0 }))
  }
  for (const [key, preference] of Object.entries(conversations)) {
    if (!preference.draft.trim()) continue
    try {
      const [directory, id]: unknown[] = JSON.parse(key)
      if (typeof directory !== 'string' || typeof id !== 'string' || !id.startsWith('draft:') || ownSessions.some((session) => session.directory === directory && session.id === id)) continue
      ownSessions.unshift(placed({ id, directory, title: `Draft · ${preference.draft.slice(0, 60)}`, project: projectLabel(directory), branch: null, updated: now, bytes: 0 }))
    } catch { /* Ignore malformed preference keys. */ }
  }
  const info: Record<string, SessionInfoValue> = {}
  for (const summary of ownSessions) {
    const current = opened.find((item) => (item.summary.id ?? item.draftId) === summary.id && item.summary.directory === summary.directory)
    const owner = conversations[conversationKey(summary.directory, summary.id)]?.botSlug
    const bot = bots.find((item) => hadConversation(item, summary) || item.slug === owner)
      ?? bots.find((item) => item.slug === current?.bot?.slug)
    const outstanding = current ? t.outstanding(current.entries) : undefined
    // Only what asks something of the reader. "Ready" and "Completed" were true of nearly
    // every row and so said nothing about any of them.
    const status: SessionStatus | undefined = !current ? undefined
      : outstanding ? (outstanding.kind === 'ask' ? 'answer' : 'approval')
        : current.running ? 'working'
          : current.entries.at(-1)?.kind === 'error' ? 'failed'
            : undefined
    if (!bot && !status) continue
    info[conversationKey(summary.directory, summary.id)] = {
      bot: bot ? { name: bot.name, avatar: bot.avatar } : undefined,
      status,
    }
  }
  const attention = Object.entries(info).filter(([key, each]) =>
    (each.status === 'answer' || each.status === 'approval') && key !== currentKey).length
  return { ownSessions: ownSessions, info, attention }
}
