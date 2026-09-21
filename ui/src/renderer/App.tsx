import { AgentSettings } from './components/AgentSettings'
import type { FileAttachment } from '../shared/files'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type {
  AskAnswer,
  BridgeEvent,
  ForkedSession,
  OpenedSession,
  Phase,
  SessionSummary,
  Shown,
  TodoRow,
} from '../shared/protocol'
import { Sidebar } from './components/Sidebar'
import { SessionInfo } from './components/Sessions'
import { Transcript } from './components/Transcript'
import { Context } from './components/Context'
import { Gutter, useColumns } from './components/Gutter'
import { shown } from './columns'
import { TrustPrompt } from './components/TrustPrompt'
import { Unconfigured } from './components/Unconfigured'
import { Notice } from './components/Notice'
import { About, type AboutInfo } from './components/About'
import { conversationModel, rememberModel } from './models'
import type { ExportFormat } from '../shared/export'
import { useCommandRouter, usePublishedState } from './commands'
import { type Fork, forkOf, forkedSessions } from '../shared/forks'
import { type Bot } from '../shared/bots'
import type { Doing } from './components/BotAvatar'
import * as t from './transcript'
import { receiveTurn, type Turns, type TurnDisclosure } from './turn-details'
import { AuditInspector } from './components/AuditInspector'
import { conversationKey } from '../shared/experience'
import { useExperience, conversationPreferences, setConversation, experienceError } from './experience'
import { ThemePicker } from './components/ThemePicker'
import { applyTheme, watchAppearance } from './theme'
import { BRAVE, BRAVE_THEME, BUILTINS, findTheme, type Theme } from '../shared/theme'

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
  phase: Phase | null
  tokens: number
  contextTokens?: number
  running: boolean
  /** Set when a fresh session needs the trust question answered before it can run. */
  askingTrust: string | null
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

/** Which kinds of question a person can be put. */
export type Asked = 'confirm' | 'run' | 'output' | 'vouch' | 'vet'

/**
 * Which method answers which question.
 *
 * Four methods rather than one taking a kind, so an answer cannot be delivered to the
 * wrong question by getting a field wrong: the agent derives the kind from the method it
 * was called on and checks it against what is actually waiting.
 */
const METHOD: Record<Asked, string> = {
  confirm: 'confirm.reply',
  run: 'run.reply',
  output: 'output.reply',
  vouch: 'vouch.reply',
  vet: 'vet.reply',
}

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
  const liveRef = useRef<Live | null>(null)
  const handleRef = useRef<string | null>(null)
  const openedLives = useRef(new Map<string, Live>())
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
   * What this window is painted in, and whether the picker is open.
   *
   * The name is remembered in `bravebot-ui.json` like the columns and the panels, and arrives the
   * same way theirs do — asynchronously, so it cannot seed `useState`.
   *
   * Seeded from the set compiled into the app rather than from nothing, so that the picker has
   * rows even if the read never answers — the palettes somebody wrote are the only part of the
   * list that has to come off disk.
   *
   * The list is kept beside the name for two reasons: the picker has something to open on without
   * a round trip, and `applyTheme` has a palette to re-resolve against when the system flips to
   * dark — a palette that names only an accent inherits the other eight, and what it inherits
   * changes. It can also change under the window, which is what `onThemeChanged` below is for:
   * somebody editing a palette file should see the window follow.
   */
  const [themes, setThemes] = useState<readonly Theme[]>(BUILTINS)
  const [chosen, setChosen] = useState(BRAVE)
  const [themesDirectory, setThemesDirectory] = useState('')
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
   * The theme in force, kept as a ref so that the appearance watcher below has the current one
   * without being torn down and reinstalled every time the choice changes.
   */
  const themeRef = useRef<Theme | null>(null)
  themeRef.current = findTheme(themes, chosen) ?? null

  /**
   * Whether the picker has the window, which decides whether an answer from disk may repaint it.
   *
   * A ref and not the state below, because `takeTheme` is installed once and would otherwise read
   * whatever `picking` was when it was made.
   */
  const previewing = useRef(false)

  /**
   * Take what the main process answered, and paint the window in it.
   *
   * Except while the picker is open, when the list is taken and the painting is not. Opening the
   * picker asks for the list again, and the watcher can answer at any moment; either reply landing
   * a frame after somebody pressed an arrow would put the *chosen* theme back over the preview
   * they were looking at. The picker owns the window until it closes, and Escape is what puts the
   * previous one back.
   */
  const takeTheme = useCallback((state: { themes: Theme[]; chosen: string; directory: string }) => {
    setThemes(state.themes)
    setChosen(state.chosen)
    setThemesDirectory(state.directory)
    if (previewing.current) return
    const theme = findTheme(state.themes, state.chosen)
    if (theme) applyTheme(theme)
  }, [])

  /**
   * Read the list again.
   *
   * Called when the picker opens as well as at startup, because the watcher can only tell the
   * window about a palette written while the window was running — and it is the one moment the
   * list has to be right. A round trip nobody is waiting on is cheaper than a picker that does not
   * offer a file somebody just saved.
   */
  const refreshThemes = useCallback(() => {
    void window.bravebot
      .readTheme()
      .then(takeTheme)
      .catch(() => undefined)
  }, [takeTheme])

  /**
   * Paint the window, and keep it painted as the answer changes underneath.
   *
   * Three things can change it: this window's own picker, a palette file being written — which
   * arrives on `onThemeChanged` — and the system flipping to dark, which only matters for a
   * palette that inherits some of its roles but matters a lot to that one. All three land in the
   * same place.
   */
  useEffect(() => {
    refreshThemes()
    const stopListening = window.bravebot.onThemeChanged(takeTheme)
    const stopWatching = watchAppearance(() => themeRef.current ?? BRAVE_THEME)
    return () => {
      stopListening()
      stopWatching()
    }
  }, [takeTheme, refreshThemes])

  const refresh = useCallback(async () => {
    try {
      const { sessions } = await call<{ sessions: SessionSummary[] }>('session.list')
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
        tokens: 0,
        running: false,
        // A record with no stored map was written before maps were kept. Nothing
        // recorded is not the same as nothing trusted, so it is asked about again.
        contextTokens: opened.contextTokens,
        askingTrust: opened.trust.known ? null : opened.record.directory,
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
      })
      const notes = [opened.branchNote, opened.buildNote].filter(Boolean) as string[]
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
      const made = await call<{ session: string; branch: string | null; model: string | null }>('session.new', {
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
          project: chosen.split('/').pop() ?? chosen,
          branch: made.branch,
          directory: chosen,
        },
        entries: [],
        turns: {},
        todos: [],
        quarantine: [],
        phase: null,
        tokens: 0,
        running: false,
        contextTokens: 0,
        askingTrust: chosen,
        forkedFrom: null,
        focus: null,
        bot: bot ? { slug: bot.slug, grounded: false } : null,
        archived: 0,
      })
      setProblem(null)
    } catch (error) {
      setProblem(String(error))
    }
  }, [])

  const answerTrust = useCallback(async (trusted: boolean) => {
    const handle = handleRef.current
    if (!handle) return
    try {
      await call('trust.reply', { session: handle, trusted })
      updateSession(handle, (old) => (old ? { ...old, askingTrust: null } : old))
    } catch (error) {
      setProblem(String(error))
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
    updateSession(handle, (old) =>
      old
        ? {
            ...old,
            summary: old.summary.title === 'New session' ? { ...old.summary, title: prompt.slice(0, 70) } : old.summary,
            attachments: selectedFiles ? old.attachments : [],
            entries: [...old.entries, ...attachments.map((file): t.Entry => ({ kind: 'attached', id: crypto.randomUUID(), path: file.path })), t.userSaid(prompt)],
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
        updateSession(handle, (old) => (old ? { ...old, running: false, queuePaused: true, entries: [...old.entries, t.errored(error.message)] } : old))
        return
      }
      updateSession(handle, (old) =>
        old
          ? {
              ...old,
              entries: [...old.entries, t.errored(String(error))],
              queuePaused: true,
              running: false,
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
   * One callback for all four, because the shape of the exchange is identical and the
   * differences are entirely in which method carries it. `remember` is only ever sent for
   * a run — it is the second answer that question has and the others do not.
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
  const ownSessions = sessions.map((summary) => {
    const current = [...openedLives.current.values()].find((item) => item.summary.id === summary.id && item.summary.directory === summary.directory)
    return current ? { ...summary, title: current.summary.title } : summary
  })
  for (const current of openedLives.current.values()) {
    const id = current.summary.id ?? current.draftId
    if (!id || ownSessions.some((summary) => summary.id === id && summary.directory === current.summary.directory)) continue
    ownSessions.unshift({ ...current.summary, id, updated: Date.now() / 1000, bytes: 0 })
  }
  for (const [key, preference] of Object.entries(preferences.conversations)) {
    if (!preference.draft.trim()) continue
    try {
      const [directory, id]: unknown[] = JSON.parse(key)
      if (typeof directory !== 'string' || typeof id !== 'string' || !id.startsWith('draft:') || ownSessions.some((session) => session.directory === directory && session.id === id)) continue
      ownSessions.unshift({ id, directory, title: `Draft · ${preference.draft.slice(0, 60)}`, project: directory.split('/').pop() ?? directory, branch: null, updated: Date.now() / 1000, bytes: 0 })
    } catch { /* Ignore malformed preference keys. */ }
  }
  const sessionInfo: Record<string, { bot?: string; state?: string }> = {}
  for (const summary of ownSessions) {
    const current = [...openedLives.current.values()].find((item) => (item.summary.id ?? item.draftId) === summary.id && item.summary.directory === summary.directory)
    const owner = preferences.conversations[conversationKey(summary.directory, summary.id)]?.botSlug
    const bot = bots.find((item) => (item.conversations.includes(summary.id) || item.slug === owner) && item.directory === summary.directory)
    sessionInfo[conversationKey(summary.directory, summary.id)] = {
      bot: bot?.name ?? bots.find((item) => item.slug === current?.bot?.slug)?.name,
      state: current ? t.outstanding(current.entries) ? t.outstanding(current.entries)?.kind === 'ask' ? 'Needs answer' : 'Needs approval' : current.running ? 'Working' : current.entries.at(-1)?.kind === 'error' ? 'Failed' : current.outcome === 'complete' ? 'Completed' : 'Ready' : undefined,
    }
  }

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
   * `shared/export.ts`. A saved file is reported through `Notice`, whose body is a `<pre>`,
   * so a long path wraps instead of running off the panel; a failure goes to the header note
   * where every other recoverable failure in this component already goes.
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
        setNotice({ title: 'Exported', body: `Saved to\n${outcome.where}` })
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
    void navigator.clipboard.writeText(text).catch(() => setProblem('Could not copy that.'))
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

      const at = entries.findIndex((candidate) => candidate.id === id)
      const entry = at === -1 ? null : entries[at]
      // Only a prompt. The menu offers this on nothing else, but the id arrives from outside
      // this component and a check here is cheaper than trusting the round trip.
      if (!entry || entry.kind !== 'user') return
      // Counted over what the *conversation* holds rather than over what this column drew, and the
      // two are not the same list: a file somebody named is a user message upstream, and is drawn
      // here as an attachment line instead of as a prompt. The agent resolves this ordinal against
      // its own messages and checks the text against it, so counting only the bubbles would put
      // every fork in a session with an attachment one or more places out — refused rather than
      // taken in the wrong place, which is the right failure and still a broken feature.
      const prompt = entries
        .slice(0, at)
        .filter((before) => before.kind === 'user' || before.kind === 'attached').length

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
            project: forked.directory.split('/').pop() ?? forked.directory,
            branch: forked.branch,
            directory: forked.directory,
          },
          entries: t.fromSaid(forked.said),
          turns: {},
          todos: Object.values(forked.todos).flat(),
          quarantine: [],
          phase: null,
          tokens: 0,
          running: false,
          contextTokens: forked.contextTokens,
          askingTrust: forked.trust.known ? null : forked.directory,
          // A fork is a session and not a bot, even when it was cut out of a bot's. A bot is one
        // conversation resumed forever; a second one carrying its name would be a second bot
        // wearing it, with the same memory file and no way to tell them apart in the list.
        bot: null,
        // A child begins with the archive its parent had at the cut, which the fork answer does
        // not carry. Zero is the safe way to be wrong: it can only ask for a briefing that is not
        // needed, where too high a figure would miss the one that is — and a fork is not a bot, so
        // in this build it asks for nothing at all.
        archived: 0,
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
      previewing.current = true
      refreshThemes()
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
        onNewBotConversation={(bot) => { void create(bot.directory, { slug: bot.slug, model: bot.model }) }}
        onBotConversation={(bot, summary) => { void showSession(summary, undefined, { slug: bot.slug, model: bot.model }) }}
        openId={live?.summary.id ?? live?.draftId ?? undefined}
        forked={forked}
        onOpen={showSession}
        onNew={create}
        bots={bots}
        openSlug={live?.bot?.slug ?? null}
        openDoing={openDoing}
        onSaveBot={saveBot}
        onRetireBot={retireBot}
        onRemoveBot={removeBot}
        build={build}
        onSettings={() => setAgentSettings(true)}
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
        draft={draft}
        onDraft={setDraft}
        onModel={(model) => void chooseModel(model)}
        onSubmit={submit}
        onCancel={cancel}
        canExport={canExport}
        includeTools={includeTools}
        onToggleTools={() => setIncludeTools((on) => !on)}
        onExport={(format) => void exportSession(format)}
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
      <Context live={live} onClose={() => toggle('right')} audit={selectedAudit ?
        <AuditInspector key={`${selectedAudit.handle}:${selectedAudit.turn}`} details={selectedAudit.turn === null ? undefined : live?.turns[selectedAudit.turn]} onClose={closeAudit} /> : null} />
      {[...openedLives.current.values()].some((item) => item.handle !== live?.handle && item.running) && (
        <div className="background-tasks" aria-label="Background tasks">
          {[...openedLives.current.values()].filter((item) => item.handle !== live?.handle && item.running).map((item) => (
            <button key={item.handle} onClick={() => setLive(item)}>
              {t.outstanding(item.entries) ? t.outstanding(item.entries)?.kind === 'ask' ? 'Answer needed' : 'Approval needed' : 'Working'} · {item.summary.title}
            </button>
          ))}
        </div>
      )}
      {aboutInfo && <About info={aboutInfo} onClose={() => setAboutInfo(null)} />}
      {notice && (
        <Notice title={notice.title} body={notice.body} onClose={() => setNotice(null)} />
      )}
      {unconfigured && <Unconfigured detail={unconfigured} onClose={() => setUnconfigured(null)} />}
      {agentSettings && <AgentSettings session={live?.handle} onClose={() => setAgentSettings(false)} onChanged={() => { void checkBackend() }} />}
      {live?.askingTrust && (
        <TrustPrompt directory={live.askingTrust} onAnswer={answerTrust} />
      )}
      {picking && (
        <ThemePicker
          themes={themes}
          chosen={chosen}
          directory={themesDirectory}
          onKeep={(name) => {
            window.bravebot.writeTheme(name)
            setChosen(name)
            previewing.current = false
            setPicking(false)
          }}
          onClose={() => {
            previewing.current = false
            setPicking(false)
          }}
        />
      )}
    </div>
  )
}

/** Fold one event into the live session. */
function apply(
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
        return { ...old, running: true, phase: null, tokens: 0,
          entries: t.beginTurn(old.entries, message.data.turn) }
      case 'audit':
        return old
      case 'phase':
        return { ...old, phase: message.data.phase }
      case 'tokens':
        return { ...old, tokens: message.data.written }
      case 'narration':
        return { ...old, entries: [...old.entries, t.narrated(message.data.text)] }
      case 'tool.started':
        return { ...old, entries: [...old.entries, t.started(message.data)] }
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
          entries: [...old.entries, t.replied(message.data.reply, message.data.turn)],
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
          entries: [...t.interruptPending(old.entries), { ...t.errored(`${kind}: ${detail}`), category: kind === 'cancelled' ? 'cancelled' : message.data.category, attempts: message.data.attempts, status: message.data.status, turn: message.data.turn }],
          queuePaused: true,
        }
      }
      default:
        return old
    }
  })
}
