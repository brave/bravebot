import type { FileAttachment } from '../shared/files'
import type {
  Checking,
  Hook,
  KeptTrust,
  PermissionMode,
  RewindPoint,
  SandboxMode,
  SessionSummary,
  SettingsRules,
  Shown,
  TodoRow,
  Waiting,
} from '../shared/protocol'
import { conversationKey } from '../shared/experience'
import { type Fork, forkOf } from '../shared/forks'
import type { Entry } from './transcript'
import type { Turns } from './turn-details'
import { conversationPreferences, setConversation } from './experience'

/** What the app is doing, which decides most of what the interface offers. */
export interface Live {
  model: string | null
  handle: string
  /**
   * The record's own id, or `null` for a session made in this window and not yet listed.
   *
   * Kept rather than looked up by title: two sessions can share a title, and a context menu
   * that closed whichever one matched first would close the wrong one.
   */
  summary: {
    id: string | null
    title: string
    project: string
    branch: string | null
    directory: string
  }
  entries: Entry[]
  turns: Turns
  todos: TodoRow[]
  quarantine: Shown[]
  phase: Waiting | null
  /** What a running confined check was given. Beside the phase, which a check does not change. */
  checking: Checking | null
  hook: Hook | null
  /** The word of the tool call the model is writing, while it is; the call itself is not yet drawn. */
  composing: string | null
  tokens: number
  contextTokens?: number
  running: boolean
  /** Set when a fresh session needs the trust question answered before it can run. */
  askingTrust: string | null
  /** Where a yes would be kept, while the question is asked and the answer may be kept (TRUST-23). */
  keepingTrust: string | null
  /** The yes kept about this directory, as last heard: said above the transcript while it lasts. */
  trustRemembered: KeptTrust | null
  /**
   * Where this session was cut from, for a session that was forked out of another.
   *
   * The title is carried because it is what the banner says, and the window may not have the
   * parent in its list — a session with no record yet is in no list at all.
   */
  forkedFrom: { directory: string; id: string; title: string; prompt: number } | null
  /**
   * A prompt to scroll to and mark, counted over the prompts the transcript drew.
   *
   * An ordinal rather than an entry id, because ids are minted fresh every time a session is
   * opened and this arrives from a session that was open a moment ago. The ordinal is the same
   * coordinate the fork itself was cut on, so it is the one thing about a place in a transcript
   * that two sessions can both mean.
   */
  focus: number | null
  /**
   * The bot whose session this is, and whether it still knows it is one.
   *
   * `null` for an ordinary session. For a bot's, `grounded` says whether the conversation
   * currently carries its briefing: false when the session has just been opened, and false again
   * once compaction has taken the briefing out of it. The next prompt sent while it is false
   * carries the briefing with it, which is the whole of what makes a bot persist.
   */
  bot: { slug: string; grounded: boolean } | null
  /**
   * How many messages compaction has taken out of this conversation, as of the last thing heard
   * about it.
   *
   * Watched rather than the `compacting` phase, which is emitted before compaction is attempted
   * and so also fires when there was nothing worth compacting — and then on every round of a
   * conversation that is over budget and cannot get under it. This only rises, and it rises
   * exactly once per compaction that actually happened.
   */
  archived: number
  /** Whether the session opened with auto-vetting on, as the agent settled it then. */
  autoVetting: boolean
  /** What the session's next turn asks before it acts, as the bridge last reported it. */
  permissionMode: PermissionMode
  /** What the session's next turn holds a program to, as the bridge last reported it. */
  sandboxMode: SandboxMode
  /** The permission rules this session opened under. Null where the bridge reported none. */
  rules?: SettingsRules | null
  /**
   * The entry id of a prompt this window has sent and not yet been told the ordinal of.
   *
   * A prompt is drawn the moment it is sent, and where it landed among the things the user said
   * is not knowable here: the turn adds a user message for every file named in the prompt, and
   * another if it spends its tool budget. So the agent reports it when the turn ends, and this
   * says which row to put it on. `null` for a turn nobody in this window asked for, a watch
   * firing or a bot bringing its memory up to date, which draw no prompt and must not take the
   * ordinal of the one above them.
   */
  awaitingOrdinal?: string | null
  /** The points the session can be put back to, newest first, as the agent last listed them. */
  rewind?: RewindPoint[]
  outcome?: 'complete' | 'failed'
  draftId?: string
  queuePaused?: boolean
  queued?: { prompt: string; attachments: FileAttachment[] }[]
  attachments?: FileAttachment[]
  /** Why the last message did not go: a name written with `@` that is not a text file in the project. */
  sendRefused?: string | null
}

/**
 * Where a session was cut from, ready for a banner, or `null` for one that was not.
 *
 * The title comes from the session list, which is the fresher of the two places it could come
 * from — a fork's record on disk says only what the parent was called; the list says what it is
 * called. A parent the list does not hold (one whose record has not been written yet, or one in
 * a project since deleted) falls back to its id, which is at least a true name.
 */
export function cameFrom(
  forks: Fork[],
  directory: string,
  id: string,
  sessions: SessionSummary[],
): Live['forkedFrom'] {
  const fork = forkOf(forks, directory, id)
  if (!fork) return null
  const parent = sessions.find(
    (session) => session.id === fork.parent.id && session.directory === fork.parent.directory,
  )
  return {
    directory: fork.parent.directory,
    id: fork.parent.id,
    title: parent?.title ?? fork.parent.id,
    prompt: fork.prompt,
  }
}

/** What a chat is called until its first prompt names it. */
export const NEW_CHAT = 'New Chat'

/** The fields every new `Live` starts with, whichever answer it is built from. */
type Starting = 'turns' | 'quarantine' | 'phase' | 'checking' | 'hook' | 'composing' | 'tokens' | 'running'

/**
 * A session as the window first holds it: what the bridge's answer says, and nothing yet said,
 * heard, checked or running.
 */
export function newLive(fields: Omit<Live, Starting>): Live {
  return {
    ...fields,
    turns: {},
    quarantine: [],
    phase: null,
    checking: null,
    hook: null,
    composing: null,
    tokens: 0,
    running: false,
  }
}

/** The key a session's preferences are stored under once it has a record id. */
export function liveKey(live: Live): string {
  return conversationKey(live.summary.directory, live.summary.id ?? live.draftId ?? live.handle)
}

/** The key they were stored under before it had one, which is where an unsent draft still is. */
export function preIdKey(live: Live): string {
  return conversationKey(live.summary.directory, live.draftId ?? live.handle)
}

/**
 * Carries a session's preferences and draft over to its record id, once it has one.
 *
 * A session made in this window is keyed by its draft until the agent writes its record; the
 * draft is cleared under the old key so a later session under that id does not inherit it.
 */
export function promoteDraft(old: Live, next: Live): void {
  if (!old.summary.id && next.summary.id) {
    setConversation(liveKey(next), conversationPreferences(preIdKey(old)))
    setConversation(preIdKey(old), { draft: '' })
  }
}
