import { SettingsView, type SettingsPage } from './components/SettingsView'
import { BotDetails } from './components/BotDetails'
import type { ComposerFooterProps, ProjectChoice } from './components/Composer'
import { botHistory } from '../shared/bot-history'
import type { Tab } from '../shared/view'
import type { FileAttachment } from '../shared/files'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type {
  AskAnswer,
  BridgeEvent,
  ForkedSession,
  KeptTrust,
  OpenedSession,
  PermissionMode,
  SandboxMode,
  RunRecord,
  SettingsRules,
  SessionSummary,
} from '../shared/protocol'
import { nextPermissionMode } from '../shared/protocol'
import { Sidebar } from './components/Sidebar'
import { SessionInfo, firstChat } from './components/Sessions'
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
import { canSend } from './can-send'
import { useCommandRouter, usePublishedState } from './commands'
import { type Fork, forkedSessions } from '../shared/forks'
import { activeBots, botProjects, type Bot } from '../shared/bots'
import { projectLabel } from '../shared/recents'
import type { Doing } from './components/BotAvatar'
import * as t from './transcript'
import { useAudit } from './use-audit'
import { useExport } from './use-export'
import { useRewind } from './use-rewind'
import { useBots } from './use-bots'
import { useAppearance } from './use-appearance'
import { conversationKey } from '../shared/experience'
import { useExperience, conversationPreferences, setConversation, dropConversation, experienceError } from './experience'
import { showToast } from './toasts'
import { planRewind } from './rewind'
import { Unconfigurable, METHOD, call, callBot } from './rpc'
import { RewindConfirm } from './components/Rewind'
import { apply } from './apply'
import { hadConversation, listSessions } from './session-list'
import { type Live, NEW_CHAT, cameFrom, liveKey, newLive, preIdKey, promoteDraft } from './live'

export function App(): React.JSX.Element {
  /** Settings take the place of the chat view while open; which page was last open is kept. */
  const [settings, setSettings] = useState(false)
  const [settingsPage, setSettingsPage] = useState<SettingsPage>('general')
  /** A bot's own page, shown in place of a conversation. */
  const [botView, setBotView] = useState<string | null>(null)
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
    if (old && next && old.handle === next.handle) promoteDraft(old, next)
    liveRef.current = next
    handleRef.current = next?.handle ?? null
    if (next) openedLives.current.set(next.handle, next)
    // A session on screen takes the place of a run being read, and of a bot's own page.
    if (next) { setReading(null); setBotView(null) }
    renderLive(next)
  }, [])
  const updateSession = useCallback((handle: string, action: React.SetStateAction<Live | null>) => {
    if (liveRef.current?.handle === handle) { setLive(action); return }
    const old = openedLives.current.get(handle) ?? null
    const next = typeof action === 'function' ? action(old) : action
    if (next) {
      if (old) promoteDraft(old, next)
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
  const draftKey = live ? liveKey(live) : ''
  const draft = preferences.conversations[draftKey]?.draft ?? ''
  const setDraft = useCallback((text: string) => {
    const current = liveRef.current
    if (current) setConversation(liveKey(current), { draft: text })
  }, [])
  useEffect(() => {
    if (live?.summary.id) rememberModel(live.summary.directory, live.summary.id, live.model)
  }, [live?.summary.id, live?.summary.directory, live?.model])
  const [forks, setForks] = useState<Fork[]>([])
  const { bots, botsRef, readBots, saveBot, removeBot } = useBots({ openedLives, updateSession, setLive, setProblem })
  const { chosen, writeAppearance } = useAppearance()

  // Read inside `showSession`, which is installed once for the same reason. The list lives in the
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
          const savedBot = botsRef.current.find((item) => item.slug === slug && item.retired === 0)
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
      bot ??= botsRef.current.find((item) => hadConversation(item, summary) || item.slug === conversationPreferences(conversationKey(summary.directory, summary.id)).botSlug)
      const opened = await call<OpenedSession>('session.open', {
        directory: summary.directory,
        id: summary.id,
      })
      if (bot) setConversation(conversationKey(summary.directory, summary.id), { botSlug: bot.slug })
      setLive(newLive({
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
        todos: Object.values(opened.todos).flat(),
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
        permissionMode: opened.permissionMode,
        sandboxMode: opened.sandboxMode,
        rules: opened.settingsRules ?? null,
        rewind: opened.rewind ?? [],
      }))
      const notes = [opened.branchNote, opened.buildNote, opened.frontNote].filter(Boolean) as string[]
      setProblem(notes.length ? notes.join(' · ') : null)
    } catch (error) {
      setProblem(String(error))
    }
    },
    [],
  )

  const create = useCallback(async (directory?: string, bot?: { slug: string; model: string | null }, draftId = `draft:${crypto.randomUUID()}`): Promise<string | null> => {
    // A directory only ever arrives here from a list somebody else handed over: File > Open
    // Recent, the recents the composer's project menu and New chat read, which the main process
    // keeps, a group heading in the chat list, whose path came off a session the bridge reported,
    // or a bot's home and recorded folders, which the main process wrote. Never a path the
    // renderer composed, which is the promise `chooseDirectory` makes.
    const chosen = directory ?? (await window.bravebot.chooseDirectory())
    if (!chosen) return null
    try {
      const made = await call<{
        session: string
        branch: string | null
        model: string | null
        autoVetting: boolean
        permissionMode: PermissionMode
        sandboxMode: SandboxMode
        settingsRules?: SettingsRules | null
        remembered?: KeptTrust | null
        keeping?: string | null
      }>('session.new', {
        directory: chosen,
      })
      setConversation(conversationKey(chosen, draftId), { botSlug: bot?.slug ?? null })
      setLive(newLive({
        handle: made.session,
        draftId,
        model: bot?.model ?? made.model,
        summary: {
          id: null,
          title: NEW_CHAT,
          project: projectLabel(chosen),
          branch: made.branch,
          directory: chosen,
        },
        entries: [],
        todos: [],
        contextTokens: 0,
        askingTrust: made.remembered ? null : chosen,
        keepingTrust: made.keeping ?? null,
        trustRemembered: made.remembered ?? null,
        forkedFrom: null,
        focus: null,
        bot: bot ? { slug: bot.slug, grounded: false } : null,
        archived: 0,
        autoVetting: made.autoVetting,
        permissionMode: made.permissionMode,
        sandboxMode: made.sandboxMode,
        rules: made.settingsRules ?? null,
      }))
      setProblem(null)
      return made.session
    } catch (error) {
      setProblem(String(error))
      return null
    }
  }, [])

  const answerTrust = useCallback(async (trusted: boolean) => {
    const handle = handleRef.current
    // One answer per question: a second click would be refused, the offer to keep a yes being spent.
    if (!handle || answeringTrust.current) return
    answeringTrust.current = true
    const path = openedLives.current.get(handle)?.keepingTrust ?? null
    try {
      const { kept } = await call<{ trusted: boolean; kept: boolean | null }>('trust.reply', { session: handle, trusted })
      const remembered = kept === true && path ? { at: Math.floor(Date.now() / 1000), path } : null
      updateSession(handle, (old) => (old ? {
        ...old,
        askingTrust: null,
        keepingTrust: null,
        trustRemembered: remembered ?? old.trustRemembered,
      } : old))
      // Still a yes, for this session. Said, because the next session here will ask after all.
      if (kept === false) {
        setProblem(`Trusting this directory for this chat only: the answer could not be written to ${path}, so the next chat here will ask.`)
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
    // The files the prompt names with `@`, asked of the bridge before anything is drawn: a name
    // that cannot go stops the send here, with the message still where it was written. The bridge
    // reads the prompt again at `turn.send`, and that check is the one that decides what goes.
    const mentioned = await window.bravebot.request<{ files: string[] }>('mentions.named', { session: handle, prompt })
    if (!mentioned.ok) {
      const refused = mentioned.error?.message ?? 'The files this message names could not be checked.'
      const queued = selectedFiles !== undefined
      updateSession(handle, (old) => old ? {
        ...old,
        sendRefused: refused,
        queued: queued ? [{ prompt, attachments }, ...(old.queued ?? [])] : old.queued,
        queuePaused: queued ? true : old.queuePaused,
      } : old)
      if (!queued && sending) {
        const key = liveKey(sending)
        if (!conversationPreferences(key).draft.trim()) setConversation(key, { draft: prompt })
      }
      return
    }
    const named = Array.isArray(mentioned.ok.files) ? mentioned.ok.files : []
    const reads = [...new Set([...attachments.map((file) => file.path), ...named])]
    // Made here rather than inside the update so its id can be remembered: the agent says where
    // this prompt landed when the turn ends, and that answer has to find the row it belongs to.
    const said = t.userSaid(prompt)
    updateSession(handle, (old) =>
      old
        ? {
            ...old,
            summary: old.summary.title === NEW_CHAT ? { ...old.summary, title: prompt.slice(0, 70) } : old.summary,
            attachments: selectedFiles ? old.attachments : [],
            sendRefused: null,
            entries: [...old.entries, ...reads.map((path): t.Entry => ({ kind: 'attached', id: crypto.randomUUID(), path })), said],
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

  // Chats whose queued message is between leaving the queue and `send` returning. `send` awaits the
  // bridge's check of the message's `@` names before it marks the chat running, and without this the
  // queue would take the next message in that gap and send both.
  const dequeuing = useRef(new Set<string>())
  useEffect(() => {
    for (const item of openedLives.current.values()) {
      if (item.running || !item.queued?.length || item.queuePaused || item.askingTrust) continue
      if (dequeuing.current.has(item.handle)) continue
      const [message, ...remaining] = item.queued
      updateSession(item.handle, (old) => old ? { ...old, queued: remaining } : old)
      if (!message) continue
      const handle = item.handle
      dequeuing.current.add(handle)
      void send(message.prompt, handle, message.attachments).finally(() => {
        dequeuing.current.delete(handle)
        refreshLives((revision) => revision + 1)
      })
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
    async (kind: t.Asked, request: number, approve: boolean, remember = false) => {
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
  const { selectedAudit, openAudit, discloseTurn, auditPanel } = useAudit(live, collapsed, toggle, updateSession)

  /** Which sessions in the list came out of another one, for the mark beside their names. */
  const forked = useMemo(() => forkedSessions(forks), [forks])

  // Bot conversations and unsent drafts remain reachable from the unified session list.
  // Unrecorded sessions are stamped to the minute so the list serialises the same between
  // keystrokes and the column is not re-rendered for a clock that nobody can see move.
  const now = Math.floor(Date.now() / 60000) * 60
  const homes = useMemo(() => new Set(bots.map((bot) => bot.home)), [bots])
  const { ownSessions: unstableSessions, info: unstableInfo, attention } = listSessions({
    sessions,
    opened: [...openedLives.current.values()],
    conversations: preferences.conversations,
    bots,
    now,
    currentKey: live ? conversationKey(live.summary.directory, live.summary.id ?? live.draftId ?? '') : '',
  })
  const ownSessions = useStableValue(unstableSessions)
  const ownSessionsRef = useRef(ownSessions)
  ownSessionsRef.current = ownSessions
  const sessionInfo = useStableValue(unstableInfo)

  /** The bot whose session is on screen, if one is — for the header, which names it. */
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

  // Allowed while a turn runs: the running turn follows the mode chosen last (MODE-8). What the bridge answered is what is drawn.
  const chooseMode = useCallback(async (mode: PermissionMode) => {
    const handle = handleRef.current
    if (!handle) return
    try {
      const { permissionMode } = await call<{ permissionMode: PermissionMode }>('session.mode', { session: handle, mode })
      updateSession(handle, (old) => (old ? { ...old, permissionMode } : old))
    } catch (error) { setProblem(String(error)) }
  }, [updateSession])

  // Allowed while a turn runs: a turn holds the mode it began with, and the next one takes this. What the bridge answered is what is drawn.
  const chooseSandbox = useCallback(async (mode: SandboxMode) => {
    const handle = handleRef.current
    if (!handle) return
    try {
      const { sandboxMode } = await call<{ sandboxMode: SandboxMode }>('session.sandbox', { session: handle, mode })
      updateSession(handle, (old) => (old ? { ...old, sandboxMode } : old))
    } catch (error) { setProblem(String(error)) }
  }, [updateSession])

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
    if (!handleRef.current || !canSend(live, backendReady, task)) return
    // A run reads nothing before it plans, so it cannot take attached files, and a bot's turn
    // carries a briefing a run has no place for.
    if (live?.attachments?.length || live?.bot) return
    setDraft('')
    void plan(task)
  }, [draft, live?.running, live?.askingTrust, live?.attachments, live?.bot, plan, backendReady])

  /** Send whatever is in the composer, on the same terms the Send button uses. */
  const submit = useCallback(() => {
    const prompt = draft.trim()
    if (!handleRef.current || !canSend(live, backendReady, prompt)) return
    setDraft('')
    void send(prompt)
  }, [draft, live?.running, live?.askingTrust, send, backendReady])

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
      if (retired) setBotView((open) => (open === slug ? null : open))
      await window.bravebot.retireBot(slug, retired).catch(() => null)
      await readBots()
    },
    [closeSession, live?.bot?.slug, readBots],
  )

  /**
   * Delete an archived conversation from disk.
   *
   * The agent refuses to delete a conversation it has open, and a conversation looked at earlier
   * stays open behind the window after another is chosen. So every open handle for it is closed
   * first, the one on screen included.
   */
  const deleteConversation = useCallback(async (summary: SessionSummary) => {
    try {
      const held = [...openedLives.current.values()].filter((item) => item.summary.id === summary.id && item.summary.directory === summary.directory)
      for (const item of held) {
        if (item.handle === handleRef.current) await closeSession()
        else {
          await call('session.close', { session: item.handle }).catch(() => undefined)
          openedLives.current.delete(item.handle)
        }
      }
      await call('session.delete', { directory: summary.directory, id: summary.id })
      dropConversation(conversationKey(summary.directory, summary.id))
      await Promise.all([refresh(), readForks(), readBots()])
    } catch (error) { setProblem(String(error)) }
  }, [closeSession, refresh, readForks, readBots])

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

  const { includeTools, toggleTools, canExport, exportSession } = useExport(live, setProblem)

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
        setLive(newLive({
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
          todos: Object.values(forked.todos).flat(),
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
          permissionMode: forked.permissionMode,
          sandboxMode: forked.sandboxMode,
          rules: forked.settingsRules ?? null,
          // The parent's backups stay the parent's: nothing in the child has written anything yet.
          rewind: [],
          forkedFrom: {
            directory: forked.parent.directory,
            id: forked.parent.id,
            title: forked.parent.title ?? live.summary.title,
            prompt: forked.parent.prompt,
          },
          focus: null,
        }))
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

  const { rewinding, askRewind, rewindEntry, confirmRewind, cancel: cancelRewind } = useRewind({ liveRef, updateSession, refresh, draft, setDraft, setProblem })

  /** Show the session this one was cut out of, at the point of the cut. */
  const openParent = useCallback(() => {
    const from = live?.forkedFrom
    if (!from) return
    const found = sessions.find(
      (session) => session.id === from.id && session.directory === from.directory,
    )
    if (found) void showSession(found, from.prompt)
    else setProblem('the chat this was forked from is no longer in the list')
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

  /** Show a bot's own page in place of whatever conversation is open. The conversation stays open. */
  const openBot = useCallback((bot: Bot) => {
    setLive(null)
    setReading(null)
    setBotView(bot.slug)
  }, [setLive])

  /**
   * Start a conversation with a bot from its page, in a project or in its home folder.
   *
   * The prompt is queued rather than sent, so it waits for the trust question a new folder asks,
   * the same as any queued message. Whether a conversation was made is the answer, so the page
   * that asked can take its guard off when one was not.
   */
  const startBotChat = useCallback(async (bot: Bot, prompt: string, directory: string | null): Promise<boolean> => {
    const handle = await create(directory ?? bot.home, { slug: bot.slug, model: bot.model })
    if (!handle) return false
    updateSession(handle, (old) => old ? { ...old, queued: [...(old.queued ?? []), { prompt, attachments: [] }] } : old)
    return true
  }, [create, updateSession])

  /** A new chat in the project used last, or the picker when there is none yet. */
  const newChat = useCallback(async () => {
    const [last] = await window.bravebot.readRecents().catch((): string[] => [])
    await create(last)
  }, [create])

  /**
   * Switching the sidebar's tab opens the first item of that list, unless what is open already
   * belongs to it.
   */
  const conversationsRef = useRef(preferences.conversations)
  conversationsRef.current = preferences.conversations
  const switchTab = useCallback((tab: Tab) => {
    const open = liveRef.current
    if (tab === 'sessions') {
      if (open) return
      const first = firstChat(ownSessionsRef.current, conversationsRef.current)
      if (first) void showSession(first)
      return
    }
    if (open?.bot) return
    const first = activeBots(botsRef.current)[0]
    if (first) openBot(first)
  }, [showSession, openBot])

  /**
   * Move a chat nothing has been said in to another folder.
   *
   * A session belongs to the folder it was opened in, so this opens a new one there and lets go of
   * the old one. The draft text comes along.
   */
  const switchProject = useCallback(async (choice: ProjectChoice) => {
    const current = liveRef.current
    if (!current || current.entries.length > 0 || current.running) return
    const owner = current.bot ? botsRef.current.find((each) => each.slug === current.bot?.slug) ?? null : null
    let directory: string | null
    if (choice.kind === 'pick') directory = await window.bravebot.chooseDirectory()
    else if (choice.kind === 'none') directory = owner?.home ?? null
    else directory = choice.directory
    if (!directory || directory === current.summary.directory) return
    const oldKey = preIdKey(current)
    const text = conversationPreferences(oldKey).draft
    const handle = await create(directory, owner ? { slug: owner.slug, model: current.model } : undefined)
    if (!handle) return
    await call('session.close', { session: current.handle }).catch(() => undefined)
    openedLives.current.delete(current.handle)
    setConversation(oldKey, { draft: '', botSlug: null })
    if (text) setDraft(text)
    refreshLives((n) => n + 1)
  }, [create, setDraft])

  const openSettingsPage = useCallback((page?: SettingsPage) => {
    if (page) setSettingsPage(page)
    setSettings(true)
  }, [])

  useCommandRouter({
    create: (directory) => { void (directory ? create(directory) : newChat()) },
    closeSession: () => void closeSession(),
    send: submit,
    cancel: () => void cancel(),
    cycleMode: () => { if (liveRef.current) void chooseMode(nextPermissionMode(liveRef.current.permissionMode)) },
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
    rewindEntry,
    undoTurn: () => { const newest = live?.rewind?.[0]; if (newest) askRewind(newest.steps) },
    exportSession: (format) => void exportSession(format),
    toggleExportTools: toggleTools,
    theme: () => openSettingsPage('general'),
  })

  // What the menu is allowed to offer. Assembled here because this is the only component
  // that can see all of it at once.
  const menuState = useMemo(
    () => ({
      hasSession: live !== null,
      running: live?.running ?? false,
      canSend: canSend(live, backendReady, draft),
      canExport,
      includeTools,
      canRewind: live !== null && !live.running && (live.rewind?.length ?? 0) > 0,
      folded: collapsed,
    }),
    [live, draft, collapsed, canExport, includeTools, backendReady],
  )
  usePublishedState(menuState)

  // Stable, so the memoised columns either side of the transcript are not re-drawn by typing.
  const stableShowSession = useEvent(showSession)
  const stableCreate = useEvent(create)
  const stableNewChat = useEvent(() => { void newChat() })
  const stableSwitchTab = useEvent(switchTab)
  const stableOpenBot = useEvent(openBot)
  const openSettings = useEvent(() => openSettingsPage())
  const stableSwitchProject = useEvent((choice: ProjectChoice) => { void switchProject(choice) })

  // A bot's own page, while no conversation is open over it.
  const viewedBot = !live && !reading && botView ? bots.find((each) => each.slug === botView && each.retired === 0) ?? null : null
  const viewedHistory = useMemo(() => viewedBot ? botHistory(viewedBot, ownSessions, preferences) : [], [viewedBot, ownSessions, preferences])
  const botPage = useMemo(() => viewedBot ? { bot: viewedBot, history: viewedHistory } : null, [viewedBot, viewedHistory])
  const details = useMemo(() => viewedBot
    ? <BotDetails bot={viewedBot} onSave={saveBot} onArchive={() => void retireBot(viewedBot.slug, true)} />
    : null, [viewedBot, saveBot, retireBot])

  // A conversation in a bot's home folder has no project, so no branch and no context column.
  const noProject = !!live && homes.has(live.summary.directory)
  const [recents, setRecents] = useState<string[]>([])
  useEffect(() => { void window.bravebot.readRecents().then(setRecents).catch(() => undefined) }, [live?.handle])
  const fresh = !!live && live.entries.length === 0 && !live.running
  const footer = useMemo((): ComposerFooterProps | undefined => {
    if (!live) return undefined
    return {
      directory: noProject ? null : live.summary.directory,
      branch: noProject ? null : live.summary.branch,
      // Only a bot is offered no project, and only the folders it may work in; see `worksIn`.
      choices: !fresh ? null : openBotRecord
        ? { folders: botProjects(openBotRecord).filter((each) => each !== live.summary.directory), noProject: !noProject }
        : { folders: recents.filter((each) => each !== live.summary.directory), noProject: false },
      onChoose: stableSwitchProject,
    }
  }, [live, noProject, fresh, openBotRecord, recents, stableSwitchProject])

  return (
    <div
      className={[
        'app',
        !live && !viewedBot ? 'no-session' : '',
        // The column comes back while an audit is open, since that is where it is drawn.
        noProject && !selectedAudit ? 'no-context' : '',
        settings ? 'in-settings' : '',
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
      {/* Kept mounted under the settings, so a filter, a fold or a draft survives a look at them. */}
      <div className="app-main" inert={settings}>
      <SessionInfo.Provider value={sessionInfo}><Sidebar
        sessions={ownSessions}
        openId={live?.summary.id ?? live?.draftId ?? reading?.record.id ?? undefined}
        forked={forked}
        onOpen={stableShowSession}
        onDelete={deleteConversation}
        onNew={stableCreate}
        onNewChat={stableNewChat}
        onTab={stableSwitchTab}
        bots={bots}
        openSlug={live?.bot?.slug ?? viewedBot?.slug ?? null}
        openDoing={live ? openDoing : 'open'}
        onOpenBot={stableOpenBot}
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
      {/* The conversation and the inspector are two cards with ground between them. A subgrid, so
          their columns are still the window's tracks and a fold or a drag moves them without this
          knowing. */}
      <div className="workspace">
      <Transcript
        onAudit={openAudit}
        onTurnDisclosure={discloseTurn}
        backendReady={backendReady}
        onCheckBackend={() => void checkBackend()}
        onDiagnostics={() => void doctor()}
        onSetup={() => openSettingsPage('agent')}
        storageKey={draftKey}
        attachments={live?.attachments ?? []}
        onAttach={() => {
          const handle = handleRef.current
          if (!handle) return
          void window.bravebot.chooseAttachments(handle).then((files) => {
            updateSession(handle, (old) => old ? { ...old, attachments: [...(old.attachments ?? []), ...files] } : old)
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
        onDismissRefusal={() => setLive((old) => old ? { ...old, sendRefused: null } : old)}
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
        onMode={(mode) => void chooseMode(mode)}
        onSandbox={(mode) => void chooseSandbox(mode)}
        onSubmit={submit}
        onPlan={submitPlan}
        reading={reading}
        onCancel={cancel}
        canExport={canExport}
        includeTools={includeTools}
        onToggleTools={toggleTools}
        onExport={(format) => void exportSession(format)}
        // Followed but never raised here: the banner says a kept answer settled this session, and
        // one another session kept since did not.
        onTrustRemembered={(handle, kept) => updateSession(handle, (old) => (old ? { ...old, trustRemembered: old.trustRemembered && kept } : old))}
        onFork={(id) => void forkFrom(id)}
        onRewind={askRewind}
        onOpenParent={openParent}
        onFocused={clearFocus}
        onDecide={answer}
        onAnswer={answerQuestions}
        botView={botPage}
        onOpenBotConversation={(bot, summary) => void showSession(summary, undefined, { slug: bot.slug, model: bot.model })}
        onStartBotChat={startBotChat}
        onBotModel={(bot, model) => { void window.bravebot.writeBotModel(bot.slug, model).then(() => readBots()) }}
        footer={footer}
        noProject={noProject}
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
      <Context live={live} audit={auditPanel} details={details} />
      </div>
      </div>
      {settings && (
        <SettingsView
          page={settingsPage}
          onPage={setSettingsPage}
          onBack={() => {
            setSettings(false)
            // Back to the control that opened the page, once the chat view is no longer inert.
            requestAnimationFrame(() => document.querySelector('.agent-settings-open')?.shadowRoot?.querySelector<HTMLElement>('button')?.focus())
          }}
          session={live?.handle}
          chosen={chosen}
          onAppearance={writeAppearance}
          onChanged={() => { void checkBackend() }}
        />
      )}
      {aboutInfo && <About info={aboutInfo} onClose={() => setAboutInfo(null)} />}
      {notice && (
        <Notice title={notice.title} body={notice.body} onClose={() => setNotice(null)} />
      )}
      {unconfigured && <Unconfigured detail={unconfigured} onClose={() => setUnconfigured(null)} />}
      {rewinding && live?.handle === rewinding.handle && (() => {
        const plan = planRewind(live.rewind ?? [], rewinding.steps)
        return plan && <RewindConfirm plan={plan} bot={live.bot !== null} running={live.running} busy={rewinding.busy}
          onCancel={cancelRewind} onConfirm={() => void confirmRewind()} />
      })()}
      {live?.askingTrust && (
        <TrustPrompt directory={live.askingTrust} keeping={live.keepingTrust} onAnswer={answerTrust} />
      )}
      <TooltipLayer />
    </div>
  )
}
