import { AgentSettings } from './components/AgentSettings'
import type { FileAttachment } from '../shared/files'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type {
  AskAnswer,
  BridgeEvent,
  Checking,
  ForkedSession,
  KeptTrust,
  OpenedSession,
  RunRecord,
  SettingsRules,
  Waiting,
  SessionSummary,
  Shown,
  TodoRow,
} from '../shared/protocol'
import { Sidebar } from './components/Sidebar'
import { SessionInfo, type SessionInfoValue, type SessionStatus } from './components/Sessions'
import { FIND_EVENT, FOCUS_COMPOSER_EVENT, Transcript } from './components/Transcript'
import { Context } from './components/Context'
import { Gutter, useColumns } from './components/Gutter'
import { shown } from './columns'
import { TrustPrompt } from './components/TrustPrompt'
import { Unconfigured } from './components/Unconfigured'
import { Notice } from './components/Notice'
import { TooltipLayer } from './components/TooltipLayer'
import { useEvent, useStableValue } from './hooks'
import { About, type AboutInfo } from './components/About'
import { conversationModel, rememberModel } from './models'
import type { ExportFormat } from '../shared/export'
import { useCommandRouter, usePublishedState } from './commands'
import { type Fork, forkOf, forkedSessions } from '../shared/forks'
import { type Bot } from '../shared/bots'
import { projectLabel } from '../shared/recents'
import type { Doing } from './components/BotAvatar'
import * as t from './transcript'
import { receiveTurn, type Turns, type TurnDisclosure } from './turn-details'
import { AuditInspector } from './components/AuditInspector'
import { conversationKey } from '../shared/experience'
import { useExperience, conversationPreferences, setConversation, experienceError } from './experience'
import { showToast } from './toasts'
import { AppearancePicker } from './components/AppearancePicker'
import { applyAppearance } from './theme'
import { SYSTEM, parseAppearance, type Appearance } from '../shared/theme'

/** What the app is doing, which decides most of what the interface offers. */
interface Live {
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
  entries: t.Entry[]
  turns: Turns
  todos: TodoRow[]
  quarantine: Shown[]
  phase: Waiting | null
  /** What a running confined check was given. Beside the phase, which a check does not change. */
  checking: Checking | null
  /** The word of the tool call the model is writing, while it is; the call itself is not yet drawn. */
  composing: string | null
  tokens: number
  contextTokens?: number
  running: boolean
  /** Set when a fresh session needs the trust question answered before it can run. */
  askingTrust: string | null
  /** Where remembering the answer would write it, while the question offers that (TRUST-23). */
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
  outcome?: 'complete' | 'failed'
  draftId?: string
  queuePaused?: boolean
  queued?: { prompt: string; attachments: FileAttachment[] }[]
  attachments?: FileAttachment[]
}

/**
 * Where a session was cut from, ready for a banner, or `null` for one that was not.
 *
 * The title comes from the session list, which is the fresher of the two places it could come
 * from — a fork's record on disk says only what the parent was called; the list says what it is
 * called. A parent the list does not hold (one whose record has not been written yet, or one in
 * a project since deleted) falls back to its id, which is at least a true name.
 */
function cameFrom(
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

/** Raised for the one failure that needs its own screen rather than a line of text. */
class Unconfigurable extends Error {}

/** Which kinds of question a person can be put. Declared beside the method each is answered by. */
export type Asked = t.Asked

/** Which method answers which question. See [`t.REPLY`], which is where they are written down. */
const METHOD: Record<Asked, string> = t.REPLY

async function call<T>(method: string, params?: Record<string, unknown>): Promise<T> {
  const answer = await window.bravebot.request<T>(method, params)
  if (answer.error) {
    // `config` is `Config::from_env` failing, which for a packaged or npm-launched app
    // means the credentials were not baked in at compile time. It is not recoverable
    // from here and needs saying properly.
    if (answer.error.code === 'config') throw new Unconfigurable(answer.error.message)
    throw new Error(`${answer.error.code}: ${answer.error.message}`)
  }
  return answer.ok as T
}

/**
 * Send a turn as a bot.
 *
 * The same failure handling as [`call`], over a different channel — and a different channel
 * because this is the one kind of turn that carries files. See `bravebot:bots:send`.
 */
async function callBot(request: {
  session: string
  slug: string
  prompt: string
  grounded: boolean
  model: string | null
  attachments?: string[]
}): Promise<void> {
  const answer = await window.bravebot.sendBotTurn(request)
  if (answer.error) {
    if (answer.error.code === 'config') throw new Unconfigurable(answer.error.message)
    throw new Error(`${answer.error.code}: ${answer.error.message}`)
  }
}

export function App(): React.JSX.Element {
  const [agentSettings, setAgentSettings] = useState(false)
  const [sessions, setSessions] = useState<SessionSummary[]>([])
  const [live, renderLive] = useState<Live | null>(null)
  /** A saved manifest run being read. Shown in place of a session, and only while none is. */
  const [reading, setReading] = useState<RunRecord | null>(null)
  const liveRef = useRef<Live | null>(null)
  const handleRef = useRef<string | null>(null)
  const openedLives = useRef(new Map<string, Live>())
  const answeringTrust = useRef(false)
  const [livesRevision, refreshLives] = useState(0)
  const preferences = useExperience()
  const setLive = useCallback((action: React.SetStateAction<Live | null>) => {
    const old = liveRef.current
    const next = typeof action === 'function' ? action(old) : action
    if (old && next && old.handle === next.handle && !old.summary.id && next.summary.id) {
      setConversation(conversationKey(next.summary.directory, next.summary.id),
        conversationPreferences(conversationKey(old.summary.directory, old.draftId ?? old.handle)))
      setConversation(conversationKey(old.summary.directory, old.draftId ?? old.handle), { draft: '' })
    }
    liveRef.current = next
    handleRef.current = next?.handle ?? null
    if (next) openedLives.current.set(next.handle, next)
    // A session on screen takes the place of a run being read.
    if (next) setReading(null)
    renderLive(next)
  }, [])
  const updateSession = useCallback((handle: string, action: React.SetStateAction<Live | null>) => {
    if (liveRef.current?.handle === handle) { setLive(action); return }
    const old = openedLives.current.get(handle) ?? null
    const next = typeof action === 'function' ? action(old) : action
    if (next) {
      if (old && !old.summary.id && next.summary.id) { setConversation(
        conversationKey(next.summary.directory, next.summary.id),
        conversationPreferences(conversationKey(old.summary.directory, old.draftId ?? old.handle)))
        setConversation(conversationKey(old.summary.directory, old.draftId ?? old.handle), { draft: '' })
      }
      openedLives.current.set(handle, next)
      refreshLives((n) => n + 1)
    }
  }, [setLive])
  const [problem, setProblem] = useState<string | null>(null)
  const [build, setBuild] = useState<string | null>(null)
  const [unconfigured, setUnconfigured] = useState<string | null>(null)
  const [backendReady, setBackendReady] = useState<boolean | null>(null)
  const checkBackend = useCallback(async () => {
    try {
      const info = await call<{ configured?: boolean }>('agent.info', { session: live?.handle })
      setBackendReady(info.configured ?? null)
    } catch { setBackendReady(false) }
  }, [live?.handle])
  useEffect(() => { void checkBackend() }, [checkBackend])
  const [notice, setNotice] = useState<{ title: string; body: string } | null>(null)
  const [aboutInfo, setAboutInfo] = useState<AboutInfo | null>(null)
  // The composer's text lives here rather than in `Transcript` because the Send menu item
  // has to be grey when there is nothing to send, and only this component talks to the menu.
  const draftKey = live ? conversationKey(live.summary.directory, live.summary.id ?? live.draftId ?? live.handle) : ''
  const draft = preferences.conversations[draftKey]?.draft ?? ''
  const setDraft = useCallback((text: string) => {
    const current = liveRef.current
    if (current) setConversation(conversationKey(current.summary.directory, current.summary.id ?? current.draftId ?? current.handle), { draft: text })
  }, [])
  useEffect(() => {
    if (live?.summary.id) rememberModel(live.summary.directory, live.summary.id, live.model)
  }, [live?.summary.id, live?.summary.directory, live?.model])
  /**
   * Whether an export carries the tool calls as well as the conversation.
   *
   * Off to begin with, because the common reason to export a session is to show somebody
   * what was asked and what came back. Held for the window rather than per session and not
   * written to disk: it is answered next to the format, at the moment of exporting, and a
   * setting remembered across launches would decide the contents of a file somebody is about
   * to hand to another person without being on screen when they do.
   */
  const [includeTools, setIncludeTools] = useState(false)

  const [forks, setForks] = useState<Fork[]>([])
  const [bots, setBots] = useState<Bot[]>([])

  /**
   * Appearance (System / Light / Dark) and whether the picker is open.
   *
   * Remembered in `bravebot-ui.json` like the columns; arrives asynchronously, so it cannot
   * seed `useState`.
   */
  const [chosen, setChosen] = useState<Appearance>(SYSTEM)
  const [picking, setPicking] = useState(false)

  // Read inside the event handler, which is installed once and must not close over a
  // stale session handle.


  // The whole of what is on screen, for the same reason: `send` is installed once and has to know
  // whether this session belongs to a bot and whether it still carries its briefing.


  // Read inside `open`, which is installed once for the same reason. The list lives in the
  // main process; this is the copy on screen, refreshed when it can have changed.
  const forksRef = useRef<Fork[]>(forks)
  forksRef.current = forks

  // The list is where a parent's *current* name lives, so a session renamed since the fork was
  // taken is named on the banner by what it is called now.
  const sessionsRef = useRef<SessionSummary[]>(sessions)
  sessionsRef.current = sessions

  const readForks = useCallback(async () => {
    setForks(await window.bravebot.readForks().catch(() => []))
  }, [])

  /**
   * Read the bot list again.
   *
   * The main process owns it, and writes to it that this window did not make: a bot's session id
   * and its compaction watermark are taken off what the agent answered. So this is asked for after
   * a turn as well as after an edit — the copy on screen is a copy.
   */
  const readBots = useCallback(async () => {
    setBots(await window.bravebot.readBots().catch(() => []))
  }, [])

  // Read inside the event listener and inside `send`, both installed once and neither able to
  // close over a list that changes under them.
  const botsRef = useRef<Bot[]>(bots)
  botsRef.current = bots

  /**
   * Paint the window from the remembered appearance at startup, and follow writes
   * that arrive through the main process (another window, a driver, the picker).
   */
  useEffect(() => {
    const take = (state: { chosen: Appearance }): void => {
      const appearance = parseAppearance(state.chosen)
      setChosen(appearance)
      applyAppearance(appearance)
    }
    void window.bravebot
      .readTheme()
      .then(take)
      .catch(() => undefined)
    return window.bravebot.onThemeChanged(take)
  }, [])

  useEffect(() => window.bravebot.onWindowActive((active) => {
    if (active) delete document.documentElement.dataset.windowInactive
    else document.documentElement.dataset.windowInactive = ''
  }), [])

  const refresh = useCallback(async () => {
    try {
      const { sessions } = await call<{ sessions: SessionSummary[] }>('session.list')
      // The `sessions` here is the field destructured from the reply on the line above, which
      // shadows the state of the same name rather than being it. Setting state from itself
      // would be the no-op this rule is about; setting it from what the agent just answered
      // is the refresh.
      // nosemgrep: javascript.react.correctness.hooks.set-state-no-op.calling-set-state-on-current-state
      setSessions(sessions)
    } catch (error) {
      setProblem(String(error))
    }
  }, [])

  useEffect(() => {
    void refresh()
    void readForks()
    void readBots()
    const stop = window.bravebot.onEvent((message: BridgeEvent) => {
      if (message.event === 'agent.ready') apply(message, setLive, setBuild, refresh)
      else if (message.session) apply(message, (action) => updateSession(message.session!, action), setBuild, refresh)
      // The list holds two things this window did not write — the session behind a bot and how
      // much compaction has taken from it — and a turn is when either can have changed. Read back
      // rather than assumed, since the main process is the one that saw the agent's answer.
      if (message.event === 'turn.done' || message.event === 'turn.error') void readBots()
    })

    // Turns nobody on this side asked for. The main process answers a compaction by asking the bot
    // to bring its memory up to date, and the window learns about it here rather than by inferring
    // it from a `turn.started` it did not cause — an inference that would be wrong the moment
    // anything else ever sends a turn.
    //
    // `turn.started` already sets `running`, so the composer locks itself and nothing here needs
    // to. What is added is the line above the reply, and the note that the briefing has been said.
    const stopConsolidation = window.bravebot.onBotConsolidation(({ session, running, delivered }) => {
      updateSession(session, (old) =>
        old
          ? {
              ...old,
              entries: running ? [...old.entries, t.consolidating()] : old.entries,
              running,
              // Only when it actually ran. Such a turn carries the briefing, so the session is
              // grounded again and the next prompt must not carry it twice — but one that never
              // left delivered nothing, and marking it said would cost the bot the briefing over
              // a turn that did not happen.
              bot: old.bot && !running && delivered ? { ...old.bot, grounded: true } : old.bot,
            }
          : old,
      )
      if (!running) void readBots()
    })

    return () => {
      stop()
      stopConsolidation()
    }
  }, [refresh, readForks, readBots])

  /**
   * Show a stored session, optionally scrolled to one of its prompts.
   *
   * `focus` is how the fork banner points back: an ordinal over the prompts, resolved to a row
   * when the transcript draws it. See `Live.focus`.
   *
   * Not called `open`, which is what it was: that shadows `window.open`, so every call to it
   * reads — to a scanner, and to anybody who does not know this file — as opening a browser
   * tab. A name that has to be recognised before it can be understood is the wrong name.
   */
  const showSession = useCallback(
    async (summary: SessionSummary, focus?: number, bot?: { slug: string; model: string | null }) => {
    try {
      // A plan run has no conversation, so it is read and not opened. No session is made for
      // it. Sessions already open stay open behind it.
      if (summary.manifest) {
        const run = await call<RunRecord>('manifest.read', { directory: summary.directory, id: summary.id })
        setLive(null)
        setReading(run)
        setProblem(null)
        return
      }
      if (summary.id.startsWith('draft:')) {
        const cached = [...openedLives.current.values()].find((item) => item.draftId === summary.id)
        if (cached) setLive(cached)
        else {
          const slug = conversationPreferences(conversationKey(summary.directory, summary.id)).botSlug
          const savedBot = botsRef.current.find((item) => item.slug === slug && item.directory === summary.directory && item.retired === 0)
          await create(summary.directory, savedBot, summary.id)
        }
        return
      }
      const cached = [...openedLives.current.values()].find((item) =>
        item.summary.directory === summary.directory && item.summary.id === summary.id)
      if (cached) {
        setLive({ ...cached, focus: focus ?? null })
        setProblem(null)
        return
      }
      bot ??= botsRef.current.find((item) => item.directory === summary.directory && (item.conversations.includes(summary.id) || item.slug === conversationPreferences(conversationKey(summary.directory, summary.id)).botSlug))
      const opened = await call<OpenedSession>('session.open', {
        directory: summary.directory,
        id: summary.id,
      })
      if (bot) setConversation(conversationKey(summary.directory, summary.id), { botSlug: bot.slug })
      setLive({
        handle: opened.session,
        model: bot?.model ?? conversationModel(summary.directory, summary.id, opened.model),
        summary: {
          id: summary.id,
          title: opened.record.title,
          project: summary.project,
          branch: opened.record.branch,
          directory: opened.record.directory,
        },
        entries: t.fromSaid(opened.said),
        turns: {},
        todos: Object.values(opened.todos).flat(),
        quarantine: [],
        phase: null,
        checking: null,
        tokens: 0,
        composing: null,
        running: false,
        // A record with no stored map was written before maps were kept. Nothing
        // recorded is not the same as nothing trusted, so it is asked about again.
        contextTokens: opened.contextTokens,
        // A kept answer settles a record whose map does not, which is all it may settle: a
        // map the record carries is this session's own and comes first.
        askingTrust: opened.trust.known ? null : opened.record.directory,
        keepingTrust: opened.keeping ?? null,
        trustRemembered: opened.remembered ?? null,
        // Looked up rather than carried: this session may have been forked in another launch
        // entirely, and the file the main process keeps is where that is written down.
        forkedFrom: cameFrom(forksRef.current, summary.directory, summary.id, sessionsRef.current),
        focus: focus ?? null,
        // Ungrounded on purpose, even though this session has said the briefing before. What was
        // said before is in the conversation only until compaction takes it, and a resumed session
        // is exactly the case where nothing on screen can tell whether that has happened. One
        // extra reading of a short file is cheaper than a bot that has quietly forgotten itself.
        bot: bot ? { slug: bot.slug, grounded: false } : null,
        archived: opened.archived,
        autoVetting: opened.autoVetting,
        rules: opened.settingsRules ?? null,
      })
      const notes = [opened.branchNote, opened.buildNote, opened.frontNote].filter(Boolean) as string[]
      setProblem(notes.length ? notes.join(' · ') : null)
    } catch (error) {
      setProblem(String(error))
    }
    },
    [],
  )

  const create = useCallback(async (directory?: string, bot?: { slug: string; model: string | null }, draftId = `draft:${crypto.randomUUID()}`) => {
    // A directory only ever arrives here from a list somebody else handed over: File > Open
    // Recent and the chevron beside New session, which the main process keeps, or a group
    // heading in the session list, whose path came off a session the bridge reported. Never
    // a path the renderer composed — which is the promise `chooseDirectory` makes, and the
    // reason a plus on a heading needs no new way in.
    const chosen = directory ?? (await window.bravebot.chooseDirectory())
    if (!chosen) return
    try {
      const made = await call<{
        session: string
        branch: string | null
        model: string | null
        autoVetting: boolean
        settingsRules?: SettingsRules | null
        remembered?: KeptTrust | null
        keeping?: string | null
      }>('session.new', {
        directory: chosen,
      })
      setConversation(conversationKey(chosen, draftId), { botSlug: bot?.slug ?? null })
      setLive({
        handle: made.session,
        draftId,
        model: bot?.model ?? made.model,
        summary: {
          id: null,
          title: 'New session',
          project: projectLabel(chosen),
          branch: made.branch,
          directory: chosen,
        },
        entries: [],
        turns: {},
        todos: [],
        quarantine: [],
        phase: null,
        checking: null,
        tokens: 0,
        composing: null,
        running: false,
        contextTokens: 0,
        askingTrust: made.remembered ? null : chosen,
        keepingTrust: made.keeping ?? null,
        trustRemembered: made.remembered ?? null,
        forkedFrom: null,
        focus: null,
        bot: bot ? { slug: bot.slug, grounded: false } : null,
        archived: 0,
        autoVetting: made.autoVetting,
        rules: made.settingsRules ?? null,
      })
      setProblem(null)
    } catch (error) {
      setProblem(String(error))
    }
  }, [])

  const answerTrust = useCallback(async (trusted: boolean, remember = false) => {
    const handle = handleRef.current
    // One answer per question: a second click would be refused, the offer to remember being spent.
    if (!handle || answeringTrust.current) return
    answeringTrust.current = true
    const path = openedLives.current.get(handle)?.keepingTrust ?? null
    try {
      const { kept } = await call<{ trusted: boolean; kept: boolean | null }>('trust.reply', { session: handle, trusted, remember })
      const remembered = kept === true && path ? { at: Math.floor(Date.now() / 1000), path } : null
      updateSession(handle, (old) => (old ? {
        ...old,
        askingTrust: null,
        keepingTrust: null,
        trustRemembered: remembered ?? old.trustRemembered,
      } : old))
      // Still a yes, for this session. Said, because the next session here will ask after all.
      if (kept === false) {
        setProblem(`Trusting this directory for this session only: the answer could not be written to ${path}, so the next session here will ask.`)
      }
    } catch (error) {
      setProblem(String(error))
    } finally {
      answeringTrust.current = false
    }
  }, [])

  const send = useCallback(async (prompt: string, target?: string, selectedFiles?: FileAttachment[]) => {
    const handle = target ?? handleRef.current
    if (!handle) return
    // Read before the state below is changed, because that is what clears it: this is the turn
    // that carries the briefing, and by the time it has been sent the session is grounded again.
    const sending = openedLives.current.get(handle)
    const bot = sending?.bot ?? null
    const model = sending?.model ?? null
    const attachments = selectedFiles ?? sending?.attachments ?? []
    // Made here rather than inside the update so its id can be remembered: the agent says where
    // this prompt landed when the turn ends, and that answer has to find the row it belongs to.
    const said = t.userSaid(prompt)
    updateSession(handle, (old) =>
      old
        ? {
            ...old,
            summary: old.summary.title === 'New session' ? { ...old.summary, title: prompt.slice(0, 70) } : old.summary,
            attachments: selectedFiles ? old.attachments : [],
            entries: [...old.entries, ...attachments.map((file): t.Entry => ({ kind: 'attached', id: crypto.randomUUID(), path: file.path })), said],
            awaitingOrdinal: said.id,
            running: true,
            queuePaused: old.queued?.length ? old.queuePaused : false,
            bot: old.bot ? { ...old.bot, grounded: true } : null,
          }
        : old,
    )
    try {
      if (bot) {
        // A bot's turn never goes through `call`. It needs files attached, and the main process
        // strips those from anything a window sends — a window that could name a file to read
        // would be a window that could have the planner read any file on the machine. So this
        // names the bot and says whether the briefing is due, and the paths are composed over
        // there from a definition this side cannot reach.
        await callBot({ session: handle, slug: bot.slug, prompt, grounded: !bot.grounded, model, attachments: attachments.map((file) => file.id) })
      } else {
        await call('turn.send', { session: handle, prompt, model, attachments: attachments.map((file) => file.id) })
      }
    } catch (error) {
      if (error instanceof Unconfigurable) {
        setUnconfigured(error.message)
        updateSession(handle, (old) => (old ? { ...old, running: false, queuePaused: true, awaitingOrdinal: null, entries: [...old.entries, t.errored(error.message)] } : old))
        return
      }
      updateSession(handle, (old) =>
        old
          ? {
              ...old,
              entries: [...old.entries, t.errored(String(error))],
              queuePaused: true,
              running: false,
              // Nothing was sent, so this prompt is in no conversation and there is no ordinal
              // coming for it. Left standing, it would take the ordinal of the next turn to
              // finish, which may be one nobody in this window asked for.
              awaitingOrdinal: null,
              // Put back. Nothing was sent, so nothing was said — a briefing marked delivered by a
              // turn that failed would be one the bot never received.
              bot: old.bot ? { ...old.bot, grounded: bot?.grounded ?? false } : null,
            }
          : old,
      )
    }
  }, [])

  useEffect(() => {
    for (const item of openedLives.current.values()) {
      if (item.running || !item.queued?.length || item.queuePaused || item.askingTrust) continue
      const [message, ...remaining] = item.queued
      updateSession(item.handle, (old) => old ? { ...old, queued: remaining } : old)
      if (message) void send(message.prompt, item.handle, message.attachments)
    }
  }, [live, livesRevision, send, updateSession])

  const cancel = useCallback(async () => {
    const handle = handleRef.current
    if (handle) {
      updateSession(handle, (old) => old ? { ...old, queuePaused: true } : old)
      await call('turn.cancel', { session: handle }).catch((error) => setProblem(String(error)))
    }
  }, [])

  /**
   * Answer whichever question is on screen.
   *
   * One callback for every kind in [`t.REPLY`], because the shape of the exchange is identical
   * and the differences are entirely in which method carries it. `remember` is only ever true
   * for a run: it is the second answer that question has and the others do not.
   *
   * The card is only marked once the agent has accepted the answer. Marking it first would
   * draw an approval the turn never received if the call failed, which is the one direction
   * this must not be wrong in.
   */
  const answer = useCallback(
    async (kind: Asked, request: number, approve: boolean, remember = false) => {
      const handle = handleRef.current
      if (!handle) return
      const decision = approve ? 'approve' : 'reject'
      try {
        await call(METHOD[kind], { session: handle, request, decision, remember })
        updateSession(handle, (old) =>
          old
            ? { ...old, entries: t.decide(old.entries, kind, request, decision, remember) }
            : old,
        )
      } catch (error) {
        setProblem(String(error))
      }
    },
    [],
  )

  /**
   * Answer a series of questions.
   *
   * Separate from [`answer`] because this reply is not a decision: it carries one answer per
   * question, and there is no approve/reject for it to be a flavour of.
   */
  const answerQuestions = useCallback(async (request: number, answers: AskAnswer[]) => {
    const handle = handleRef.current
    if (!handle) return
    try {
      await call('ask.reply', { session: handle, request, answers })
      updateSession(handle, (old) => (old ? { ...old, entries: t.answered(old.entries, request, answers) } : old))
    } catch (error) {
      setProblem(String(error))
    }
  }, [])

  const pending = useMemo(
    () => (live ? t.outstanding(live.entries) : null),
    [live],
  )

  const { widths, collapsed, dragging, folding, start, reset, nudge, toggle } = useColumns()
  const [audit, setAudit] = useState<{ handle: string; turn: number | null; trigger: HTMLButtonElement; wasFolded: boolean } | null>(null)
  useEffect(() => { setAudit(null) }, [live?.handle])
  const selectedAudit = audit?.handle === live?.handle ? audit : null
  const openAudit = (turn: number | null, trigger: HTMLButtonElement) => {
    if (!live) return
    setAudit({ handle: live.handle, turn, trigger, wasFolded: selectedAudit?.wasFolded ?? collapsed.right })
    if (collapsed.right) toggle('right')
  }
  const closeAudit = () => {
    if (selectedAudit?.wasFolded && !collapsed.right) toggle('right')
    setAudit(null)
    // The live trigger disappears at completion; return to that turn's new footer instead.
    const trigger = selectedAudit?.trigger.isConnected ? selectedAudit.trigger :
      selectedAudit && selectedAudit.turn !== null ? document.querySelector<HTMLButtonElement>(`.turn-footer [data-audit-turn="${selectedAudit.turn}"]`) : null
    trigger?.focus({ preventScroll: true })
  }
  const discloseTurn = (turn: number, field: TurnDisclosure, open: boolean) => {
    if (!live) return
    updateSession(live.handle, (old) => old?.turns[turn] ? { ...old, turns: { ...old.turns, [turn]: { ...old.turns[turn]!, [field]: open } } } : old)
  }

  /** Which sessions in the list came out of another one, for the mark beside their names. */
  const forked = useMemo(() => forkedSessions(forks), [forks])

  // Bot conversations and unsent drafts remain reachable from the unified session list.
  // Unrecorded sessions are stamped to the minute so the list serialises the same between
  // keystrokes and the column is not re-rendered for a clock that nobody can see move.
  const now = Math.floor(Date.now() / 60000) * 60
  const unstableSessions = sessions.map((summary) => {
    const current = [...openedLives.current.values()].find((item) => item.summary.id === summary.id && item.summary.directory === summary.directory)
    return current ? { ...summary, title: current.summary.title } : summary
  })
  for (const current of openedLives.current.values()) {
    const id = current.summary.id ?? current.draftId
    if (!id || unstableSessions.some((summary) => summary.id === id && summary.directory === current.summary.directory)) continue
    unstableSessions.unshift({ ...current.summary, id, updated: now, bytes: 0 })
  }
  for (const [key, preference] of Object.entries(preferences.conversations)) {
    if (!preference.draft.trim()) continue
    try {
      const [directory, id]: unknown[] = JSON.parse(key)
      if (typeof directory !== 'string' || typeof id !== 'string' || !id.startsWith('draft:') || unstableSessions.some((session) => session.directory === directory && session.id === id)) continue
      unstableSessions.unshift({ id, directory, title: `Draft · ${preference.draft.slice(0, 60)}`, project: projectLabel(directory), branch: null, updated: now, bytes: 0 })
    } catch { /* Ignore malformed preference keys. */ }
  }
  const ownSessions = useStableValue(unstableSessions)
  const unstableInfo: Record<string, SessionInfoValue> = {}
  for (const summary of ownSessions) {
    const current = [...openedLives.current.values()].find((item) => (item.summary.id ?? item.draftId) === summary.id && item.summary.directory === summary.directory)
    const owner = preferences.conversations[conversationKey(summary.directory, summary.id)]?.botSlug
    const bot = bots.find((item) => (item.conversations.includes(summary.id) || item.slug === owner) && item.directory === summary.directory)
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
    unstableInfo[conversationKey(summary.directory, summary.id)] = {
      bot: bot ? { name: bot.name, avatar: bot.avatar } : undefined,
      status,
    }
  }
  const sessionInfo = useStableValue(unstableInfo)
  const attention = Object.entries(sessionInfo).filter(([key, info]) =>
    (info.status === 'answer' || info.status === 'approval') &&
    key !== (live ? conversationKey(live.summary.directory, live.summary.id ?? live.draftId ?? '') : '')).length

  /**
   * Show a bot.
   *
   * Two paths, and which one is taken says whether the bot has ever spoken. One that has is
   * resumed — the same session, every time, which is the whole of what makes it persistent. One
   * that has not has no session to resume: the agent writes no record until a first turn, so
   * there is nothing on disk to open and a fresh one is begun in its checkout instead. Its
   * durable id is learned when the turn that creates it finishes, by the main process, off what
   * the agent answered.
   */
  /** The bot whose session is on screen, if one is — for the header, which names it. */
  useEffect(() => {
    const edited = (event: Event) => {
      const slug = (event as CustomEvent<string>).detail
      for (const session of openedLives.current.values()) {
        if (session.bot?.slug === slug) updateSession(session.handle, (old) => old?.bot ? { ...old, bot: { ...old.bot, grounded: false } } : old)
      }
    }
    document.addEventListener('bravebot:memory-edited', edited)
    return () => document.removeEventListener('bravebot:memory-edited', edited)
  }, [])

  const openBotRecord = useMemo(
    () => (live?.bot ? (bots.find((each) => each.slug === live.bot?.slug) ?? null) : null),
    [live?.bot, bots],
  )

  /**
   * What the open bot's face should be doing — for the header, and for its row in the list, which
   * mirrors it. Working while a turn runs; `failed` if the last thing the transcript got was an
   * error, which is what a `turn.error` leaves at the end of it; otherwise looking at the reader.
   * The turn's completion is not a state of its own — the face nods on leaving `working`.
   */
  const openDoing: Doing = live?.running
    ? 'working'
    : live?.entries.at(-1)?.kind === 'error'
      ? 'failed'
      : 'open'

  const saveBot = useCallback(
    async (bot: { slug?: string; avatar?: string; model?: string | null; name: string; purpose: string; directory: string }) => {
      try {
        const saved = await window.bravebot.writeBot(bot)
        if (!saved) throw new Error('Could not save bot.')
        setLive((old) => old?.bot?.slug === saved.slug && !old.running
          ? { ...old, model: saved.model ?? old.model } : old)
        await readBots()
        return true
      } catch (error) { setProblem(String(error)); return false }
    },
    [readBots],
  )

  const chooseModel = useCallback(async (model: string) => {
    const current = liveRef.current
    if (!current || current.running) return
    try {
      if (current.bot) {
        const saved = await window.bravebot.writeBotModel(current.bot.slug, model)
        if (!saved) throw new Error('Could not save the bot’s model.')
        await readBots()
      }
      setLive((old) => old?.handle === current.handle && !old.running ? { ...old, model } : old)
    } catch (error) { setProblem(String(error)) }
  }, [readBots])

  /**
   * Take a bot away for good.
   *
   * No session to let go of, unlike `retireBot` below. This is only ever reached from a row in
   * the archive, and archiving is what put it there — which closed its session on the way past.
   * A bot that is deletable is therefore a bot that is not on screen, by construction rather than
   * by a check here.
   */
  const removeBot = useCallback(
    async (slug: string) => {
      try {
        await window.bravebot.removeBot(slug)
        await readBots()
      } catch (error) { setProblem(String(error)) }
    },
    [readBots],
  )

  /**
   * Start a manifest run for a task: plan all of it, ask once, then walk the plan.
   *
   * One run per press. The session does not hold this as a mode, so the next message is an
   * ordinary turn unless the button is pressed again.
   */
  const plan = useCallback(async (task: string) => {
    const handle = handleRef.current
    if (!handle) return
    const model = openedLives.current.get(handle)?.model ?? null
    updateSession(handle, (old) => old ? { ...old, running: true, entries: [...old.entries, t.planAsked(task)] } : old)
    try {
      await call('manifest.run', { session: handle, task, model })
    } catch (error) {
      if (error instanceof Unconfigurable) setUnconfigured(error.message)
      updateSession(handle, (old) => old ? { ...old, running: false, queuePaused: true, entries: [...old.entries, t.errored(error instanceof Unconfigurable ? error.message : String(error))] } : old)
    }
  }, [])

  /** Start a run from whatever is in the composer, where a run can take it. */
  const submitPlan = useCallback(() => {
    const task = draft.trim()
    if (!task || !handleRef.current || live?.running || live?.askingTrust || backendReady === false) return
    // A run reads nothing before it plans, so it cannot take attached files, and a bot's turn
    // carries a briefing a run has no place for.
    if (live?.attachments?.length || live?.bot) return
    setDraft('')
    void plan(task)
  }, [draft, live?.running, live?.askingTrust, live?.attachments, live?.bot, plan, backendReady])

  /** Send whatever is in the composer, on the same terms the Send button uses. */
  const submit = useCallback(() => {
    const prompt = draft.trim()
    if (!prompt || !handleRef.current || live?.running || live?.askingTrust || backendReady === false) return
    setDraft('')
    void send(prompt)
  }, [draft, live?.running, send, backendReady])

  /**
   * Let go of the open session.
   *
   * The agent is told, which cancels the turn and refuses whatever it was waiting on — both,
   * in that order, and that is why this is a call rather than just clearing the state here.
   * Dropping the session on the floor would leave a write blocked on an answer nobody is
   * going to give.
   */
  const closeSession = useCallback(async () => {
    const handle = handleRef.current
    if (!handle) return
    await call('session.close', { session: handle }).catch(() => undefined)
    openedLives.current.delete(handle)
    setLive(null)
    void refresh()
  }, [refresh])

  /**
   * Put a bot away, or bring it back.
   *
   * Defined down here rather than beside `saveBot` because of the one case that needs more than
   * a write and a re-read: archiving the bot whose session is on screen. Its row goes, and a
   * header still naming it — with a composer still willing to send to it — would be the window
   * disagreeing with itself about whether that bot is in use. So the open session is let go of
   * the same way closing it by hand does, which is what `closeSession` above is for.
   */
  const retireBot = useCallback(
    async (slug: string, retired: boolean) => {
      if (retired && live?.bot?.slug === slug) await closeSession()
      await window.bravebot.retireBot(slug, retired).catch(() => null)
      await readBots()
    },
    [closeSession, live?.bot?.slug, readBots],
  )

  const about = useCallback(async () => {
    try {
      const info = await call<AboutInfo>('agent.info')
      setAboutInfo(info)
    } catch (error) {
      setProblem(String(error))
    }
  }, [])

  const doctor = useCallback(async () => {
    try {
      const report = await call<{ found: boolean; text: string }>('doctor')
      setNotice({ title: 'Diagnostics', body: report.text.trim() || 'It said nothing at all.' })
    } catch (error) {
      setProblem(String(error))
    }
  }, [])

  /**
   * The conversation, as an export would carry it.
   *
   * Derived rather than built at the moment somebody picks a format, because whether there
   * is anything to export decides two things that have to agree: whether the button is grey
   * and whether the menu item is. One list, read twice.
   */
  const exportable = useMemo(
    () => (live ? t.conversation(live.entries, includeTools) : []),
    [live, includeTools],
  )

  /**
   * Whether there is anything worth writing to a file.
   *
   * Counts what was *said*, not what is in `exportable`: a session that has only made tool
   * calls would otherwise offer an export that `parseExportRequest` then refuses, because a
   * file of nothing but calls is not a conversation. Two places decide this and they have to
   * agree; this is the one that greys the button, and the boundary is the one that cannot be
   * got around.
   */
  const canExport = useMemo(() => exportable.some((turn) => turn.role !== 'tool'), [exportable])

  /**
   * Write the conversation to a file.
   *
   * The turns go over structured and the main process composes the document — see
   * `shared/export.ts`. A saved file is confirmed in a toast that names where it went; a failure
   * goes to the problem toast where every other recoverable failure in this component goes.
   */
  const exportSession = useCallback(
    async (format: ExportFormat) => {
      if (!live || !canExport) return
      const outcome = await window.bravebot.exportSession({
        format,
        document: {
          title: live.summary.title,
          directory: live.summary.directory,
          branch: live.summary.branch,
          turns: exportable,
        },
      })
      if (outcome.status === 'saved') {
        showToast('Exported', `Saved to ${outcome.where}`)
      } else if (outcome.status === 'failed') {
        setProblem(`Could not export that: ${outcome.message}`)
      }
      // A cancelled sheet says nothing. Somebody changed their mind, which is not news.
    },
    [live, canExport, exportable],
  )

  const resetColumns = useCallback(() => {
    reset('left')
    reset('right')
  }, [reset])

  /**
   * Copy, done here rather than in the main process.
   *
   * The text never leaves the renderer, which is what keeps the context-menu channel free of
   * anything the agent read off disk. A clipboard write is also the one thing a person can
   * unambiguously do with confined content: it is their own machine, and copying is not a
   * decision the planner benefits from.
   */
  const copy = useCallback((text: string) => {
    void navigator.clipboard.writeText(text).then(() => showToast('Copied to clipboard'), () => setProblem('Could not copy that.'))
  }, [])

  const openSession = useCallback(
    (id: string) => {
      const found = sessions.find((session) => session.id === id)
      if (found) void showSession(found)
    },
    [sessions, showSession],
  )

  /**
   * Close a session named by a right-click.
   *
   * Only the open one can actually be closed — the others have no handle, because this build
   * opens one at a time. Closing a row that is not the open one is therefore nothing rather
   * than an error, and the menu item is the same item either way.
   */
  const closeNamed = useCallback(
    (id: string) => {
      if (live?.summary.id === id) void closeSession()
    },
    [live, closeSession],
  )

  const copyProjectPath = useCallback(
    (id: string) => {
      const found = sessions.find((session) => session.id === id)
      if (found) copy(found.directory)
    },
    [sessions, copy],
  )

  /**
   * Begin a session from what was said before a prompt, with that prompt to edit.
   *
   * What crosses to the agent is the prompt's *place* — how many prompts precede it — and what
   * it said. Not the entry's id, which means nothing outside this window, and not a slice of
   * the transcript, which is a projection of the conversation rather than the conversation. The
   * agent cuts its own history and hands back what the fork now holds, so what is drawn after
   * this is what the next turn will actually be working from.
   */
  const forkFrom = useCallback(
    async (id: string) => {
      const handle = handleRef.current
      const entries = live?.entries
      if (!handle || !entries) return

      const entry = entries.find((candidate) => candidate.id === id) ?? null
      // Only a prompt. The menu offers this on nothing else, but the id arrives from outside
      // this component and a check here is cheaper than trusting the round trip.
      if (!entry || entry.kind !== 'user') return
      // The agent's own ordinal, read rather than counted. A window counting what it drew holds
      // a second copy of a rule the agent already applies, and the two agree only for as long as
      // nobody adds a kind of user message: a turn nudged for spending its tool budget adds one
      // and no event tells this window about it. The agent resolves the ordinal against its own
      // messages and checks the text against it, so a count that is short by one is a fork
      // refused rather than one taken in the wrong place, which is the right failure and still a
      // broken feature.
      const prompt = entry.prompt
      if (prompt === undefined) return

      try {
        const forked = await call<ForkedSession>('session.fork', {
          session: handle,
          prompt,
          text: entry.text,
        })

        // The fork first and the parent second: the cut is made out of the parent's live state,
        // so letting go of it before asking would be asking about a session that had gone.
        setLive({
          handle: forked.session,
          model: live.model,
          summary: {
            id: forked.id,
            // What a fork is called is decided by the agent from the history it kept, and it
            // has no record yet to read it off. Named after its parent here for the same
            // reason: this is where it came from, and the first turn will settle it.
            title: live.summary.title,
            project: projectLabel(forked.directory),
            branch: forked.branch,
            directory: forked.directory,
          },
          entries: t.fromSaid(forked.said),
          turns: {},
          todos: Object.values(forked.todos).flat(),
          quarantine: [],
          phase: null,
          checking: null,
          tokens: 0,
          composing: null,
          running: false,
          contextTokens: forked.contextTokens,
          askingTrust: forked.trust.known ? null : forked.directory,
          keepingTrust: forked.keeping ?? null,
          trustRemembered: forked.remembered ?? null,
          // A fork is a session and not a bot, even when it was cut out of a bot's. A bot is one
        // conversation resumed forever; a second one carrying its name would be a second bot
        // wearing it, with the same memory file and no way to tell them apart in the list.
        bot: null,
        // A child begins with the archive its parent had at the cut, which the fork answer does
        // not carry. Zero is the safe way to be wrong: it can only ask for a briefing that is not
        // needed, where too high a figure would miss the one that is — and a fork is not a bot, so
        // in this build it asks for nothing at all.
        archived: 0,
        autoVetting: forked.autoVetting,
        rules: forked.settingsRules ?? null,
        forkedFrom: {
            directory: forked.parent.directory,
            id: forked.parent.id,
            title: forked.parent.title ?? live.summary.title,
            prompt: forked.parent.prompt,
          },
          focus: null,
        })
        // From the agent rather than from `entry.text`: what the composer opens with should be
        // the prompt that was actually cut out, not the one this window thought it clicked.
        setDraft(forked.prefill)
        setProblem(null)
        void refresh()
        void readForks()
      } catch (error) {
        setProblem(String(error))
      }
    },
    [live, refresh, readForks],
  )

  /** Show the session this one was cut out of, at the point of the cut. */
  const openParent = useCallback(() => {
    const from = live?.forkedFrom
    if (!from) return
    const found = sessions.find(
      (session) => session.id === from.id && session.directory === from.directory,
    )
    if (found) void showSession(found, from.prompt)
    else setProblem('the session this was forked from is no longer in the list')
  }, [live, sessions, showSession])

  /** Stop marking the prompt a fork link landed on, so the transcript behaves normally again. */
  const clearFocus = useCallback(() => {
    setLive((old) => (old && old.focus !== null ? { ...old, focus: null } : old))
  }, [])

  const copyEntry = useCallback(
    (id: string) => {
      const entry = live?.entries.find((candidate) => candidate.id === id)
      const text = entry ? t.plainText(entry) : null
      if (text) copy(text)
    },
    [live, copy],
  )

  useCommandRouter({
    create,
    closeSession: () => void closeSession(),
    send: submit,
    cancel: () => void cancel(),
    toggle,
    resetColumns,
    find: () => document.dispatchEvent(new Event(FIND_EVENT)),
    focusComposer: () => document.dispatchEvent(new Event(FOCUS_COMPOSER_EVENT)),
    about: () => void about(),
    doctor: () => void doctor(),
    openSession,
    closeNamed,
    copyProjectPath,
    copyEntry,
    forkEntry: (id) => void forkFrom(id),
    exportSession: (format) => void exportSession(format),
    toggleExportTools: () => setIncludeTools((on) => !on),
    theme: () => {
      setPicking(true)
    },
  })

  // What the menu is allowed to offer. Assembled here because this is the only component
  // that can see all of it at once.
  const menuState = useMemo(
    () => ({
      hasSession: live !== null,
      running: live?.running ?? false,
      canSend: live !== null && !live.running && !live.askingTrust && backendReady !== false && draft.trim().length > 0,
      canExport,
      includeTools,
      folded: collapsed,
    }),
    [live, draft, collapsed, canExport, includeTools, backendReady],
  )
  usePublishedState(menuState)

  // Stable, so the memoised columns either side of the transcript are not re-drawn by typing.
  const newBotConversation = useEvent((bot: Bot) => { void create(bot.directory, { slug: bot.slug, model: bot.model }) })
  const botConversation = useEvent((bot: Bot, summary: SessionSummary) => { void showSession(summary, undefined, { slug: bot.slug, model: bot.model }) })
  const stableShowSession = useEvent(showSession)
  const stableCreate = useEvent(create)
  const openSettings = useEvent(() => setAgentSettings(true))
  const closeContext = useEvent(() => toggle('right'))
  const stableCloseAudit = useEvent(closeAudit)
  const auditTurn = selectedAudit && selectedAudit.turn !== null ? live?.turns[selectedAudit.turn] : undefined
  const auditPanel = useMemo(() => selectedAudit
    ? <AuditInspector key={`${selectedAudit.handle}:${selectedAudit.turn}`} details={auditTurn} onClose={stableCloseAudit} />
    : null, [selectedAudit, auditTurn, stableCloseAudit])

  return (
    <div
      className={[
        'app',
        !live ? 'no-session' : '',
        preferences.density,
        dragging ? 'resizing' : '',
        folding ? 'folding' : '',
        collapsed.left ? 'left-folded' : '',
        collapsed.right ? 'right-folded' : '',
      ]
        .filter(Boolean)
        .join(' ')}
      // The widths drive the grid through custom properties, so a drag repaints the
      // layout without React touching the columns themselves. The `-open` pair is what a
      // folded column's contents keep as their width, so they slide out under a clip
      // rather than reflowing into a shape nobody will ever read.
      style={
        {
          '--col-left': `${shown({ widths, collapsed }, 'left')}px`,
          '--col-right': `${shown({ widths, collapsed }, 'right')}px`,
          '--col-left-open': `${widths.left}px`,
          '--col-right-open': `${widths.right}px`,
        } as React.CSSProperties
      }
    >
      <SessionInfo.Provider value={sessionInfo}><Sidebar
        sessions={ownSessions}
        onNewBotConversation={newBotConversation}
        onBotConversation={botConversation}
        openId={live?.summary.id ?? live?.draftId ?? reading?.record.id ?? undefined}
        forked={forked}
        onOpen={stableShowSession}
        onNew={stableCreate}
        bots={bots}
        openSlug={live?.bot?.slug ?? null}
        openDoing={openDoing}
        onSaveBot={saveBot}
        onRetireBot={retireBot}
        onRemoveBot={removeBot}
        build={build}
        onSettings={openSettings}
      /></SessionInfo.Provider>
      <Gutter
        side="left"
        width={shown({ widths, collapsed }, 'left')}
        dragging={dragging === 'left'}
        collapsed={collapsed.left}
        onStart={start}
        onReset={reset}
        onNudge={nudge}
      />
      {/* The conversation and the inspector share one raised card. A subgrid, so its columns
          are still the window's tracks and a fold or a drag moves them without this knowing. */}
      <div className="workspace">
      <Transcript
        onAudit={openAudit}
        onTurnDisclosure={discloseTurn}
        backendReady={backendReady}
        onCheckBackend={() => void checkBackend()}
        onDiagnostics={() => void doctor()}
        onSetup={() => setAgentSettings(true)}
        storageKey={draftKey}
        attachments={live?.attachments ?? []}
        onAttach={() => {
          const handle = handleRef.current
          if (!handle) return
          void window.bravebot.chooseAttachments(handle).then((files) => {
            updateSession(handle, (old) => old ? { ...old, attachments: [...(old.attachments ?? []), ...files].slice(0, 5) } : old)
          }).catch((error) => setProblem(String(error)))
        }}
        onRemoveAttachment={(id) => setLive((old) => old ? { ...old, attachments: old.attachments?.filter((file) => file.id !== id) } : old)}
        queuePaused={live?.queuePaused ?? false}
        onResumeQueued={() => setLive((old) => old ? { ...old, queuePaused: false } : old)}
        queued={live?.queued?.map((message) => message.prompt) ?? []}
        onQueue={() => {
          if (!draft.trim()) return
          setLive((old) => old ? { ...old, queued: [...(old.queued ?? []), { prompt: draft.trim(), attachments: old.attachments ?? [] }], attachments: [] } : old)
          setDraft('')
        }}
        onRemoveQueued={(index) => setLive((old) => old ? { ...old, queued: old.queued?.filter((_, at) => at !== index) } : old)}
        onNew={create}
        bot={openBotRecord}
        doing={openDoing}
        live={live}
        pending={pending}
        problem={experienceError() || problem}
        collapsed={collapsed}
        onToggle={toggle}
        attention={attention}
        draft={draft}
        onDraft={setDraft}
        onModel={(model) => void chooseModel(model)}
        onSubmit={submit}
        onPlan={submitPlan}
        reading={reading}
        onCancel={cancel}
        canExport={canExport}
        includeTools={includeTools}
        onToggleTools={() => setIncludeTools((on) => !on)}
        onExport={(format) => void exportSession(format)}
        // Followed but never raised here: the banner says a kept answer settled this session, and
        // one another session kept since did not.
        onTrustRemembered={(handle, kept) => updateSession(handle, (old) => (old ? { ...old, trustRemembered: old.trustRemembered && kept } : old))}
        onFork={(id) => void forkFrom(id)}
        onOpenParent={openParent}
        onFocused={clearFocus}
        onDecide={answer}
        onAnswer={answerQuestions}
      />
      <Gutter
        side="right"
        width={shown({ widths, collapsed }, 'right')}
        dragging={dragging === 'right'}
        collapsed={collapsed.right}
        onStart={start}
        onReset={reset}
        onNudge={nudge}
      />
      <Context live={live} onClose={closeContext} audit={auditPanel} />
      </div>
      {aboutInfo && <About info={aboutInfo} onClose={() => setAboutInfo(null)} />}
      {notice && (
        <Notice title={notice.title} body={notice.body} onClose={() => setNotice(null)} />
      )}
      {unconfigured && <Unconfigured detail={unconfigured} onClose={() => setUnconfigured(null)} />}
      {agentSettings && <AgentSettings session={live?.handle} onClose={() => setAgentSettings(false)} onChanged={() => { void checkBackend() }} />}
      {live?.askingTrust && (
        <TrustPrompt directory={live.askingTrust} keeping={live.keepingTrust} onAnswer={answerTrust} />
      )}
      {picking && (
        <AppearancePicker
          chosen={chosen}
          onKeep={(appearance) => {
            window.bravebot.writeTheme(appearance)
            setChosen(appearance)
            setPicking(false)
          }}
          onClose={() => setPicking(false)}
        />
      )}
      <TooltipLayer />
    </div>
  )
}

/** Fold one event into the live session. */
export function apply(
  message: BridgeEvent,
  setLive: React.Dispatch<React.SetStateAction<Live | null>>,
  setBuild: (build: string) => void,
  refresh: () => void,
): void {
  if (message.event === 'agent.ready') {
    setBuild((message.data as { build: string }).build)
    return
  }

  setLive((old) => {
    if (!old) return old
    old = { ...old, turns: receiveTurn(old.turns, message) }
    switch (message.event) {
      case 'watch.fired':
        return { ...old, entries: [...old.entries, t.watchFired(message.data.number, message.data.path)] }
      case 'watch.ended':
        return { ...old, entries: [...old.entries, t.narrated(`Watch ${message.data.number} ended: ${message.data.reason}${message.data.message ? `. ${message.data.message}` : ''}`)] }
      case 'turn.started':
        return { ...old, running: true, phase: null, checking: null, composing: null, tokens: 0,
          entries: t.beginTurn(old.entries, message.data.turn) }
      case 'audit':
        return old
      // A phase opens a round, so a call written in the last one is over.
      case 'phase':
        return { ...old, phase: message.data.phase, composing: null }
      case 'composing':
        return { ...old, composing: message.data.call }
      // The phase is left alone: it is what the word goes back to once the check is over.
      case 'check.started':
        return { ...old, checking: message.data }
      case 'check.finished':
        return { ...old, checking: null }
      case 'tokens':
        return { ...old, tokens: message.data.written }
      case 'narration':
        return { ...old, composing: null, entries: [...old.entries, t.narrated(message.data.text)] }
      case 'tool.started':
        return { ...old, composing: null, entries: [...old.entries, t.started(message.data)] }
      case 'tool.finished':
        return { ...old, entries: t.finish(old.entries, message.data) }
      case 'landed':
        return { ...old, entries: t.land(old.entries, message.data.landing) }
      case 'quarantined':
        return {
          ...old,
          quarantine: [...old.quarantine, message.data],
          entries: [...old.entries, t.quarantined(message.data)],
        }
      case 'todos':
        return { ...old, todos: message.data.rows }
      case 'confirm.request':
        return { ...old, entries: [...old.entries, t.asked(message.data)] }
      case 'run.request':
        return { ...old, entries: [...old.entries, t.askedRun(message.data)] }
      case 'output.request':
        return { ...old, entries: [...old.entries, t.askedOutput(message.data)] }
      case 'vet.request':
        return { ...old, entries: [...old.entries, t.askedVet(message.data)] }
      case 'fetch.request':
        return { ...old, entries: [...old.entries, t.askedFetch(message.data)] }
      case 'server.request':
        return { ...old, entries: [...old.entries, t.askedServer(message.data)] }
      case 'manifest.request':
        return { ...old, entries: [...old.entries, t.askedManifest(message.data, t.narrowing(old.rules))] }
      case 'exposure.request':
        return { ...old, entries: [...old.entries, t.askedExposure(message.data)] }
      case 'mcp-server.request':
        return { ...old, entries: [...old.entries, t.askedMcpServer(message.data)] }
      case 'mcp-tools.request':
        return { ...old, entries: [...old.entries, t.askedMcpTools(message.data)] }
      case 'mcp-call.request':
        return { ...old, entries: [...old.entries, t.askedMcpCall(message.data)] }
      case 'mcp-move.request':
        return { ...old, entries: [...old.entries, t.askedMcpMove(message.data)] }
      // A server has up to a minute to answer its handshake, and the turn says nothing else
      // while it waits, so the wait is named where the phase is.
      case 'mcp.starting':
        return { ...old, phase: 'starting-servers' }
      case 'mcp.started':
        return { ...old, phase: null, entries: [...old.entries, t.mcpStarted(message.data)] }
      // A run is not a turn, so it adds no turn marker and no reply to the conversation.
      case 'manifest.started':
        return { ...old, running: true, phase: null, checking: null, tokens: 0 }
      case 'manifest.done':
        refresh()
        return { ...old, running: false, phase: null, checking: null, outcome: 'complete',
          entries: [...old.entries, t.planReplied(message.data.reply, message.data.record)] }
      case 'manifest.error':
        refresh()
        return { ...old, running: false, phase: null, checking: null, queuePaused: true,
          entries: [...t.interruptPending(old.entries), t.planEnded(message.data)] }
      case 'vouch.request':
        return { ...old, entries: [...old.entries, t.askedVouch(message.data)] }
      case 'ask.request':
        return { ...old, entries: [...old.entries, t.askedQuestions(message.data)] }
      case 'turn.done': {
        refresh()
        // A rise means compaction summarised part of this conversation away, and what it takes it
        // takes from the front — so a briefing put at the top of the session is the first thing
        // gone. Saying the bot is no longer grounded is what makes the next prompt carry it again.
        const compacted = message.data.archived > old.archived
        return {
          ...old,
          contextTokens: message.data.contextTokens,
          summary: { ...old.summary, id: message.data.id ?? old.summary.id },
          outcome: 'complete',
          running: message.data.consolidating === true,
          phase: null,
          // A consolidating turn stays running, so a check whose end was never heard would stay drawn.
          checking: null,
          composing: null,
          entries: [...t.number(old.entries, old.awaitingOrdinal ?? '', message.data.prompt), t.replied(message.data.reply, message.data.turn)],
          awaitingOrdinal: null,
          archived: message.data.archived,
          bot: old.bot && compacted ? { ...old.bot, grounded: false } : old.bot,
        }
      }
      case 'turn.error': {
        refresh()
        const { kind, message: detail } = message.data
        return {
          ...old,
          contextTokens: message.data.contextTokens,
          summary: { ...old.summary, id: message.data.id ?? old.summary.id },
          running: false,
          phase: null,
          checking: null,
          composing: null,
          entries: [...t.interruptPending(t.number(old.entries, old.awaitingOrdinal ?? '', message.data.prompt)), { ...t.errored(`${kind}: ${detail}`), category: kind === 'cancelled' ? 'cancelled' : message.data.category, attempts: message.data.attempts, status: message.data.status, cutOff: message.data.cutOff, turn: message.data.turn }],
          awaitingOrdinal: null,
          queuePaused: true,
        }
      }
      default:
        return old
    }
  })
}
