import { SettingsView, type SettingsPage } from './components/SettingsView'
import { BotDetails } from './components/BotDetails'
import type { ComposerFooterProps, ProjectChoice } from './components/Composer'
import { botHistory, homeConversation } from '../shared/bot-history'
import type { Tab } from '../shared/view'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type {
  BridgeEvent,
  ForkedSession,
  OpenedSession,
  PermissionMode,
  SandboxMode,
  RunRecord,
  SessionSettled,
  SessionSummary,
} from '../shared/protocol'
import { nextPermissionMode } from '../shared/protocol'
import { Sidebar } from './components/Sidebar'
import { Agents } from './components/Agents'
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
import { BugReport } from './components/BugReport'
import { UpdateBanner } from './components/UpdateBanner'
import { bannerShown, parseUpdateStatus, type UpdateStatus } from '../shared/update'
import { conversationModel, rememberModel } from './models'
import { canSend } from './can-send'
import { useCommandRouter, usePublishedState } from './commands'
import { type Fork, forkedSessions } from '../shared/forks'
import { activeBots, type Bot } from '../shared/bots'
import type { Summon } from '../shared/summons'
import { projectLabel } from '../shared/recents'
import type { Doing } from './components/BotAvatar'
import * as t from './transcript'
import { useAudit } from './use-audit'
import { useExport } from './use-export'
import { useRewind } from './use-rewind'
import { useBots } from './use-bots'
import { useTurn } from './use-turn'
import { useEnhance } from './use-enhance'
import { useAppearance } from './use-appearance'
import { conversationKey } from '../shared/experience'
import { useExperience, conversationPreferences, setConversation, setExperience, dropConversation, experienceError } from './experience'
import { showToast } from './toasts'
import { planRewind } from './rewind'
import { call } from './rpc'
import { RewindConfirm } from './components/Rewind'
import { PurgeProject } from './components/PurgeProject'
import { ImportSessions } from './components/ImportSessions'
import { type Purged, purgeReport, turnRunningIn } from './purge'
import { apply } from './apply'
import { hadConversation, listSessions } from './session-list'
import { type Live, NEW_CHAT, cameFrom, liveKey, newLive, preIdKey, promoteDraft, settledBy } from './live'

export function App(): React.JSX.Element {
  /** Settings take the place of the chat view while open; which page was last open is kept. */
  const [settings, setSettings] = useState(false)
  /** The list of agents the open conversation has started, over the chat view. */
  const [agentsOpen, setAgentsOpen] = useState(false)
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
  /** The bots whose home conversation is being made, so opening one twice makes one. */
  const startingHome = useRef(new Set<string>())
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
  const [notice, setNotice] = useState<{ title: string; subtitle?: string; body: string } | null>(null)
  const [aboutInfo, setAboutInfo] = useState<AboutInfo | null>(null)
  const [bugReportOpen, setBugReportOpen] = useState(false)
  /** What the last update check found (UPDATE-12). Asked once at start and again when About opens. */
  const [updateStatus, setUpdateStatus] = useState<UpdateStatus | null>(null)
  const readUpdateStatus = useCallback(async () => {
    try { setUpdateStatus(parseUpdateStatus(await call('update.status'))) } catch { setUpdateStatus(null) }
  }, [])
  useEffect(() => { void readUpdateStatus() }, [readUpdateStatus])
  const dismissedUpdate = preferences.dismissedUpdate
  const download = useCallback(() => { void window.bravebot.openPage('releases') }, [])
  const updateNotice = useMemo(() => {
    const running = updateStatus?.running ?? null
    return running !== null && bannerShown(updateStatus, dismissedUpdate)
      ? <UpdateBanner running={running} onDownload={download}
        onDismiss={() => setExperience('dismissedUpdate', running)} />
      : null
  }, [updateStatus, dismissedUpdate, download])
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
  const { answerTrust, submit, submitPlan, queueDraft, cancel, answer, answerQuestions, interjectDraft } = useTurn({
    liveRef, handleRef, openedLives, updateSession, setLive, setProblem, setUnconfigured, refreshLives, live, livesRevision, backendReady, draft, setDraft,
  })
  const enhancement = useEnhance({ openedLives, updateSession, setProblem })
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

  // Which bot answered which summoned turn. The main process writes it when such a turn ends.
  const [summons, setSummons] = useState<Summon[]>([])
  const readSummons = useCallback(async () => {
    setSummons(await window.bravebot.readSummons().catch(() => []))
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
    void readSummons()
    const stop = window.bravebot.onEvent((message: BridgeEvent) => {
      if (message.event === 'enhance.done' && message.session) enhancement.receive(message.session, message.data)
      else if (message.event === 'agent.ready') apply(message, setLive, setBuild, refresh)
      else if (message.session) apply(message, (action) => updateSession(message.session!, action), setBuild, refresh)
      // The list holds two things this window did not write — the session behind a bot and how
      // much compaction has taken from it — and a turn is when either can have changed. Read back
      // rather than assumed, since the main process is the one that saw the agent's answer. The
      // summons are written as the turn ends, so they are read back then too.
      if (message.event === 'turn.done' || message.event === 'turn.error') {
        void readBots()
        void readSummons()
      }
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
              consolidating: running,
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
  }, [refresh, readForks, readBots, readSummons])

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
        askingTrust: opened.trust.known ? null : opened.shownDirectory,
        ...settledBy(opened),
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
        context: opened.context,
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
      const made = await call<SessionSettled & {
        session: string
        branch: string | null
        model: string | null
        shownDirectory: string
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
        askingTrust: made.remembered ? null : made.shownDirectory,
        ...settledBy(made),
        forkedFrom: null,
        focus: null,
        bot: bot ? { slug: bot.slug, grounded: false } : null,
        archived: 0,
      }))
      setProblem(null)
      return made.session
    } catch (error) {
      setProblem(String(error))
      return null
    }
  }, [])

  const pending = useMemo(
    () => (live ? t.outstanding(live.entries) : null),
    [live],
  )

  const { widths, collapsed, dragging, folding, start, reset, nudge, toggle } = useColumns()
  const { openAudit, discloseTurn, auditPanel } = useAudit(live, collapsed, toggle, updateSession)

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
  const preferencesRef = useRef(preferences)
  preferencesRef.current = preferences
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
      await Promise.all([refresh(), readForks(), readBots(), readSummons()])
    } catch (error) { setProblem(String(error)) }
  }, [closeSession, refresh, readForks, readBots, readSummons])

  /** The project whose group menu asked to delete all its chats or to import some, while it asks. */
  const [projectAction, setProjectAction] = useState<{ kind: 'purge' | 'import'; directory: string; opened: ReadonlySet<string> } | null>(null)
  const askPurge = useCallback((directory: string) => {
    const opened = new Set([...openedLives.current.values()].flatMap((item) => item.summary.directory === directory && item.summary.id ? [item.summary.id] : []))
    setProjectAction({ kind: 'purge', directory, opened })
  }, [])
  const askImport = useCallback((directory: string) => setProjectAction({ kind: 'import', directory, opened: new Set() }), [])
  const leaveProjectAction = useCallback(() => setProjectAction(null), [])

  /**
   * Delete the named conversations of one project, the way [`deleteConversation`] deletes one:
   * every handle this window holds on them is closed first, since the agent keeps what is open.
   */
  const purgeProject = useCallback(async (directory: string, ids: string[]) => {
    setProjectAction(null)
    if (turnRunningIn(openedLives.current.values(), directory)) {
      setProblem('A chat in this project is still answering. Wait for it to finish, then delete the project’s chats.')
      return
    }
    try {
      const chosen = new Set(ids)
      const held = [...openedLives.current.values()].filter((item) => item.summary.directory === directory && item.summary.id !== null && chosen.has(item.summary.id))
      for (const item of held) {
        if (item.handle === handleRef.current) await closeSession()
        else {
          await call('session.close', { session: item.handle }).catch(() => undefined)
          openedLives.current.delete(item.handle)
        }
      }
      const answer = await call<Purged>('session.purge', { directory, ids })
      for (const id of answer.deleted) dropConversation(conversationKey(directory, id))
      const report = purgeReport(answer)
      if ('problem' in report) setProblem(report.problem)
      else showToast(report.done, projectLabel(directory))
      await Promise.all([refresh(), readForks(), readBots()])
    } catch (error) { setProblem(String(error)) }
  }, [closeSession, refresh, readForks, readBots])

  const about = useCallback(async () => {
    try {
      const info = await call<AboutInfo>('agent.info')
      await readUpdateStatus()
      setAboutInfo(info)
    } catch (error) {
      setProblem(String(error))
    }
  }, [readUpdateStatus])

  const doctor = useCallback(async () => {
    try {
      const report = await call<{ found: boolean; text: string }>('doctor')
      setNotice({ title: 'Diagnostics', body: report.text.trim() || 'It said nothing at all.' })
    } catch (error) {
      setProblem(String(error))
    }
  }, [])

  // Plain text, not Markdown: the notes link every issue, and the agent has already written each
  // link as its label, so nothing in this dialog opens anything.
  const changelog = useCallback(async () => {
    try {
      const read = await call<{ version: string; running: boolean; notes: string | null }>('changelog.read')
      if (read.notes === null) {
        setNotice({ title: 'Release notes', body: 'No release notes in this build.' })
        return
      }
      setNotice({
        title: `Release notes for ${read.version}`,
        subtitle: read.running ? undefined : 'This build has no notes of its own. These are the newest release’s.',
        body: read.notes,
      })
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
          askingTrust: forked.trust.known ? null : forked.shownDirectory,
          ...settledBy(forked),
          // A fork is a session and not a bot, even when it was cut out of a bot's. A bot is one
          // conversation resumed forever; a second one carrying its name would be a second bot
          // wearing it, with the same memory file and no way to tell them apart in the list.
          bot: null,
          // A child begins with the archive its parent had at the cut, which the fork answer does
          // not carry. Zero is the safe way to be wrong: it can only ask for a briefing that is not
          // needed, where too high a figure would miss the one that is — and a fork is not a bot, so
          // in this build it asks for nothing at all.
          archived: 0,
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

  /**
   * Open a bot's conversation: the newest one in its home folder, or a new one there when it has
   * none (MEMORY-14). A bot works in a project by being summoned in that project's chat, so there
   * is no project to choose here.
   */
  const openBot = useCallback((bot: Bot) => {
    const owner = { slug: bot.slug, model: bot.model }
    const newest = homeConversation(bot, botHistory(bot, ownSessionsRef.current, preferencesRef.current))
    if (newest?.session) { void showSession(newest.session, undefined, owner); return }
    // One already open and not archived: an archived one is left where it is, and a new one is made.
    const waiting = [...openedLives.current.values()].find((item) => item.bot?.slug === bot.slug && item.summary.directory === bot.home
      && !preferencesRef.current.conversations[liveKey(item)]?.archived)
    if (waiting) { setLive(waiting); return }
    // A second open while the first is still being made would make a second home conversation.
    if (startingHome.current.has(bot.slug)) return
    startingHome.current.add(bot.slug)
    void create(bot.home, owner).finally(() => startingHome.current.delete(bot.slug))
  }, [setLive, showSession, create])

  /** Show a bot's earlier conversations in place of the one open. The conversation stays open. */
  const openEarlier = useCallback((bot: Bot) => {
    setLive(null)
    setReading(null)
    setBotView(bot.slug)
  }, [setLive])

  /** Open the conversation a "Replied to" card names, at the prompt that summoned the bot. */
  const openSummon = useCallback((summon: Summon) => {
    const found = sessionsRef.current.find((each) => each.directory === summon.session.directory && each.id === summon.session.id)
    if (found) void showSession(found, summon.prompt)
    else setProblem('That conversation is no longer in the chat list.')
  }, [showSession])

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
    changelog: () => void changelog(),
    doctor: () => void doctor(),
    bugReport: () => setBugReportOpen(true),
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
  const stableOpenEarlier = useEvent(openEarlier)
  const stableOpenSummon = useEvent(openSummon)
  const openSettings = useEvent(() => openSettingsPage())
  const openAgents = useEvent(() => setAgentsOpen(true))
  const stableSwitchProject = useEvent((choice: ProjectChoice) => { void switchProject(choice) })

  // A bot's own page, while no conversation is open over it.
  const viewedBot = !live && !reading && botView ? bots.find((each) => each.slug === botView && each.retired === 0) ?? null : null
  const viewedHistory = useMemo(() => {
    if (!viewedBot) return []
    const rows = botHistory(viewedBot, ownSessions, preferences)
    const current = homeConversation(viewedBot, rows)
    return rows.filter((row) => row !== current)
  }, [viewedBot, ownSessions, preferences])
  const botPage = useMemo(() => viewedBot ? { bot: viewedBot, history: viewedHistory } : null, [viewedBot, viewedHistory])
  // A conversation in a bot's home folder has no project, so no branch and no files to list.
  const noProject = !!live && homes.has(live.summary.directory)
  // The right column of a bot's own page and of its home conversation holds the bot, since a home
  // folder has no project to inspect.
  const detailed = viewedBot ?? (noProject ? openBotRecord : null)
  const details = useMemo(() => detailed
    ? <BotDetails bot={detailed} onSave={saveBot} onArchive={() => void retireBot(detailed.slug, true)} />
    : null, [detailed, saveBot, retireBot])
  const [recents, setRecents] = useState<string[]>([])
  useEffect(() => { void window.bravebot.readRecents().then(setRecents).catch(() => undefined) }, [live?.handle])
  const fresh = !!live && live.entries.length === 0 && !live.running
  const footer = useMemo((): ComposerFooterProps | undefined => {
    if (!live) return undefined
    return {
      directory: noProject ? null : live.summary.directory,
      branch: noProject ? null : live.summary.branch,
      // Only a bot is offered no project, and only the folders it may work in; see `worksIn`.
      // A bot's conversation stays where it is; a bot is summoned into a project instead.
      choices: !fresh || openBotRecord ? null : { folders: recents.filter((each) => each !== live.summary.directory), noProject: false },
      onChoose: stableSwitchProject,
    }
  }, [live, noProject, fresh, openBotRecord, recents, stableSwitchProject])

  return (
    <div
      className={[
        'app',
        !live && !viewedBot ? 'no-session' : '',
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
        onPurge={askPurge}
        onImport={askImport}
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
        agentsStarted={live?.agentCounts.started ?? 0}
        agentsWorking={live?.agentCounts.working ?? 0}
        onAgents={openAgents}
        notice={updateNotice}
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
        onThinking={(shown) => setLive((old) => old ? { ...old, showThinking: shown } : old)}
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
        onStaging={(handle, staging) => updateSession(handle, (old) => old ? { ...old, staging } : old)}
        queuePaused={live?.queuePaused ?? false}
        onResumeQueued={() => setLive((old) => old ? { ...old, queuePaused: false } : old)}
        queued={live?.queued?.map((message) => message.prompt) ?? []}
        onInterject={interjectDraft}
        onEnhance={() => { const handle = handleRef.current; if (handle) void enhancement.enhance(handle) }}
        onStopEnhance={() => { const handle = handleRef.current; if (handle) enhancement.stop(handle) }}
        onRestoreEnhanced={() => { const handle = handleRef.current; return !!handle && enhancement.restore(handle) }}
        onDismissEnhancement={() => { const handle = handleRef.current; if (handle) enhancement.dismiss(handle) }}
        onQueue={queueDraft}
        interjected={live?.interjected?.map((line) => line.prompt) ?? []}
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
        onEarlier={stableOpenEarlier}
        onBackToBot={stableOpenBot}
        bots={bots}
        summons={summons}
        onOpenSummon={stableOpenSummon}
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
      {agentsOpen && (
        <Agents
          agents={live?.agents ?? []}
          onClose={() => {
            setAgentsOpen(false)
            // Back to the entry that opened the list, once the chat view is no longer inert.
            requestAnimationFrame(() => document.querySelector('.agents-open')?.shadowRoot?.querySelector<HTMLElement>('button')?.focus())
          }}
        />
      )}
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
          running={live?.running ?? false}
          onAppearance={writeAppearance}
          onChanged={() => { void checkBackend() }}
        />
      )}
      {aboutInfo && <About info={aboutInfo} update={updateStatus} onDownload={download}
        onReportBug={() => { setAboutInfo(null); setBugReportOpen(true) }} onClose={() => setAboutInfo(null)} />}
      {bugReportOpen && <BugReport onClose={() => setBugReportOpen(false)} />}
      {notice && (
        <Notice title={notice.title} subtitle={notice.subtitle} body={notice.body} onClose={() => setNotice(null)} />
      )}
      {projectAction?.kind === 'purge' && (
        <PurgeProject directory={projectAction.directory} opened={projectAction.opened} onClose={leaveProjectAction}
          onConfirm={(ids) => void purgeProject(projectAction.directory, ids)} />
      )}
      {projectAction?.kind === 'import' && (
        <ImportSessions directory={projectAction.directory} onClose={leaveProjectAction} onImported={() => void refresh()} />
      )}
      {unconfigured && <Unconfigured detail={unconfigured} onClose={() => setUnconfigured(null)} />}
      {rewinding && live?.handle === rewinding.handle && (() => {
        const plan = planRewind(live.rewind ?? [], rewinding.steps)
        return plan && <RewindConfirm plan={plan} bot={live.bot !== null} running={live.running} busy={rewinding.busy}
          onCancel={cancelRewind} onConfirm={() => void confirmRewind()} />
      })()}
      {live?.askingTrust && (
        <TrustPrompt directory={live.askingTrust} keeping={live.shownKeepingTrust} confinedLines={live.confinedLinesNotice} onAnswer={answerTrust} />
      )}
      <TooltipLayer />
    </div>
  )
}
