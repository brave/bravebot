import { Watches } from './Watches'
import type { FileAttachment } from '../../shared/files'
import { Permissions } from './Permissions'
import { memo, useLayoutEffect, useEffect, useMemo, useRef, useState } from 'react'
import { useEvent } from '../hooks'
import { IconButton } from './IconButton'
import { IconMenu } from './IconMenu'
import { CopyButton } from './CopyButton'
import { isConfined, type Ambient, type ManifestError, type RunRecord as SavedRun, type PermissionMode, type SettingsRules, type AskAnswer, type AskPrompt, type Checking, type Hook, type KeptTrust, type RewindPoint, type Waiting, type Shown, type TodoRow } from '../../shared/protocol'
import * as t from '../transcript'
import { drawCommand } from '../../shared/connectors'
import type { Side } from '../columns'
import type { Asked } from '../App'
import type { ExportFormat } from '../../shared/export'
import { Diff } from './Diff'
import { BackendTray, Composer, ProjectMenu, type ComposerFooterProps } from './Composer'
import { BotView } from './BotView'
import type { BotConversation } from '../../shared/bot-history'
import type { SessionSummary } from '../../shared/protocol'
import { Toasts } from './Toasts'
import { ForkIcon } from './ForkIcon'
import { ago, contextMenu } from './Sessions'
import { Markdown } from './Markdown'
import { BotAvatar, type Doing } from './BotAvatar'
import type { Bot } from '../../shared/bots'
import { projectLabel } from '../../shared/recents'
import { conversationPreferences, setConversation } from '../experience'
import { ErrorCard } from './ErrorCard'
import { RunRecord } from './RunRecord'
import { failureSummary } from '../failure'
import { FilePreview } from './FilePreview'
import { TurnFooter, TurnNotices, type OpenAudit } from './TurnDetails'
import type { Turns, TurnDisclosure } from '../turn-details'
import { Alert, Button, Collapse, Icon, Input, Label, ProgressRing, type IconName } from '../nala'
import { middleTruncate } from '../truncate'
import { pointForPrompt, undoRow } from '../rewind'

interface Live {
  model: string | null
  handle: string
  summary: { title: string; project: string; branch: string | null; directory: string }
  entries: t.Entry[]
  turns: Turns
  todos: TodoRow[]
  quarantine: Shown[]
  phase: Waiting | null
  checking: Checking | null
  hook: Hook | null
  composing: string | null
  contextTokens?: number
  archived?: number
  tokens: number
  running: boolean
  askingTrust: string | null
  forkedFrom: { directory: string; id: string; title: string; prompt: number } | null
  focus: number | null
  autoVetting?: boolean
  permissionMode: PermissionMode
  trustRemembered?: KeptTrust | null
  rules?: SettingsRules | null
  /** The points this session can be put back to, newest first. */
  rewind?: RewindPoint[]
  sendRefused?: string | null
}

/**
 * How an answer travels back up.
 *
 * The kind rides along with the id: the reply goes to a different method per question, and
 * the card that drew the question is the only thing that knows which one it was.
 */
export type Answer = (
  kind: Asked,
  request: number,
  approve: boolean,
  remember?: boolean,
) => void

/** How a series of answers travels back up. */
export type AnswerQuestions = (request: number, answers: AskAnswer[]) => void

interface Props {
  onAudit: OpenAudit
  onTurnDisclosure: (turn: number, field: TurnDisclosure, open: boolean) => void
  attachments: FileAttachment[]
  onAttach: () => void
  onRemoveAttachment: (id: string) => void
  backendReady: boolean | null
  onCheckBackend: () => void
  onDiagnostics: () => void
  onSetup: () => void
  storageKey: string
  onNew: (directory?: string) => void
  onQueue: () => void
  queued: string[]
  queuePaused: boolean
  onResumeQueued: () => void
  onRemoveQueued: (index: number) => void
  onDismissRefusal: () => void
  live: Live | null
  /** A saved manifest run being read. Drawn in place of a session, which it is not. */
  reading?: SavedRun | null
  /** The bot whose session this is, if one is. Its name is what the header says instead of a title. */
  bot: Bot | null
  /** What that bot is doing, for its face in the header. Derived in `App`, where the turn is known. */
  doing: Doing
  pending: t.Asking | null
  problem: string | null
  collapsed: Record<Side, boolean>
  onToggle: (side: Side) => void
  /** Background sessions waiting on the reader, for the folded session list's toggle. */
  attention: number
  /** The composer's text, owned by `App` so the Send menu item can be grey when it is empty. */
  draft: string
  onDraft: (draft: string) => void
  onModel: (model: string) => void
  onMode: (mode: PermissionMode) => void
  onSubmit: () => void
  /** Start a manifest run from the draft. Absent where the window cannot start one. */
  onPlan?: () => void
  onCancel: () => void
  onDecide: Answer
  onAnswer: AnswerQuestions
  /** Begin a session from what was said before a named prompt. */
  onFork: (id: string) => void
  /** Ask to put the session back `steps` turns. */
  onRewind: (steps: number) => void
  /** Show the session this one was forked out of, at the prompt it was cut in front of. */
  onOpenParent: () => void
  /** Said once the marked prompt has been scrolled to, so the mark can be let go of. */
  onFocused: () => void
  /** Whether there is any conversation to write down. */
  canExport: boolean
  /** Whether an export will carry the tool calls as well as the conversation. */
  includeTools: boolean
  onToggleTools: () => void
  onExport: (format: ExportFormat) => void
  /** The kept yes about this directory as Permissions last read it, which another session may have changed. */
  onTrustRemembered: (session: string, kept: KeptTrust | null) => void
  /** A bot's own page, shown when no conversation is open. */
  botView?: { bot: Bot; history: BotConversation[] } | null
  onOpenBotConversation: (bot: Bot, summary: SessionSummary) => void
  onStartBotChat: (bot: Bot, prompt: string, directory: string | null) => Promise<boolean>
  onBotModel: (bot: Bot, model: string) => void
  /** The composer's footer for the open conversation: its project and branch. */
  footer?: ComposerFooterProps
  /** Whether the open conversation runs in a bot's home folder, which has no project to show. */
  noProject: boolean
}

/** Dispatched on `document` by the View menu's Find item. */
export const FIND_EVENT = 'bravebot:find'
/** Dispatched on `document` by the View menu's Focus Composer item. */
export const FOCUS_COMPOSER_EVENT = 'bravebot:focus-composer'

/**
 * Run something once a menu has finished closing.
 *
 * A dialog opened from a menu item remembers what had focus to give it back on close. Opened in
 * the click, that is the item, which is gone by then; a tick later it is the menu's trigger.
 */
const afterMenu = (open: () => void): void => { setTimeout(open, 0) }

/**
 * The find bar's field.
 *
 * Keys are read by a listener on the host rather than through Leo's `onKeyDown`, whose handler is
 * the one from the first render and would step through the matches of the first query typed. Leo
 * re-dispatches the key on the host as an event that carries the keyboard event in `innerEvent`.
 */
function FindInput({ query, onQuery, onClose, onStep }: {
  query: string
  onQuery: (query: string) => void
  onClose: () => void
  onStep: (back: boolean) => void
}): React.JSX.Element {
  const host = useRef<HTMLElement>(null)
  const latest = useRef({ onClose, onStep })
  latest.current = { onClose, onStep }
  useEffect(() => {
    const element = host.current
    if (!element) return
    const key = (heard: Event) => {
      const event = ((heard as Event & { innerEvent?: KeyboardEvent }).innerEvent ?? heard) as KeyboardEvent
      if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); latest.current.onClose() }
      if (event.key === 'Enter') { event.preventDefault(); latest.current.onStep(event.shiftKey) }
    }
    element.addEventListener('keydown', key)
    return () => element.removeEventListener('keydown', key)
  }, [])
  return (
    <Input ref={host} autofocus type="search" size="small" className="find-input" aria-label="Find in conversation"
      placeholder="Find in conversation" value={query} onInput={({ value }) => onQuery(value)}>
      <Icon name="search" slot="left-icon" />
    </Input>
  )
}

/**
 * The control that folds one side column away.
 *
 * Both toggles live here, in the middle column's header, rather than each sitting in the
 * column it controls. A button that moved when its column folded would be unmounted and
 * mounted somewhere else — which drops keyboard focus to nothing, with no shortcut to get
 * back — and would read to a screen reader as a new control rather than as the same one
 * changing state.
 *
 * The name stays put and `aria-expanded` carries the state, which is the disclosure
 * pattern: a label that flipped between "Show" and "Hide" would say the state twice and
 * rename a button the moment it was pressed. The verb goes in the tooltip, which is for the
 * pointer. The mark swaps with that state: an open column shows the split it belongs to,
 * and a folded column shows the panel coming back.
 *
 * With the session list folded, the left toggle is the only place a background session can
 * ask for attention, so it carries a count of the ones waiting on an answer or an approval.
 */
function ColumnToggle({
  side,
  collapsed,
  onToggle,
  attention = 0,
}: {
  side: Side
  collapsed: boolean
  onToggle: (side: Side) => void
  attention?: number
}): React.JSX.Element {
  const what = side === 'left' ? 'the chat list' : 'the context panel'
  const label = side === 'left' ? 'Chat list' : 'Context panel'
  const controls = side === 'left' ? 'sessions-column' : 'context-column'
  const icon: IconName =
    side === 'left'
      ? collapsed
        ? 'browser-sidebar-off'
        : 'browser-sidebar'
      : collapsed
        ? 'browser-sidebar-right-off'
        : 'browser-sidebar-right'
  const waiting = collapsed && attention > 0
  const need = `${attention} ${attention === 1 ? 'chat needs' : 'chats need'} you`

  return (
    <span className={`fold-toggle-slot ${side}`}>
      <IconButton
        icon={icon}
        label={label}
        tooltip={`${collapsed ? 'Show' : 'Hide'} ${what}${waiting ? ` · ${need}` : ''}`}
        shortcut={side === 'left' ? '⌘⌥←' : '⌘⌥→'}
        description={waiting ? need : undefined}
        className={`fold-toggle ${side}`}
        expanded={!collapsed}
        controls={controls}
        onClick={() => onToggle(side)}
      />
      {waiting && <span className="attention-badge num" aria-hidden="true">{attention}</span>}
    </span>
  )
}

/** The middle column: the conversation, and everything the turn did inside it. */
export function Transcript({
  onAudit,
  onTurnDisclosure,
  attachments,
  onAttach,
  onRemoveAttachment,
  backendReady,
  onCheckBackend,
  onDiagnostics,
  onSetup,
  storageKey,
  onNew,
  onQueue,
  queued, queuePaused, onResumeQueued, onDismissRefusal,
  onRemoveQueued,
  live,
  bot,
  doing,
  pending,
  problem,
  collapsed,
  onToggle,
  attention,
  draft,
  onDraft,
  onModel,
  onMode,
  onSubmit,
  onPlan,
  reading,
  onCancel,
  onDecide,
  onAnswer,
  onFork,
  onRewind,
  onOpenParent,
  onFocused,
  canExport,
  includeTools,
  onToggleTools,
  onExport,
  onTrustRemembered,
  botView,
  onOpenBotConversation,
  onStartBotChat,
  onBotModel,
  footer,
  noProject,
}: Props): React.JSX.Element {
  const bottom = useRef<HTMLDivElement>(null)
  const marked = useRef<HTMLDivElement>(null)
  const scroller = useRef<HTMLDivElement>(null)
  const input = useRef<HTMLElement>(null)
  const following = useRef(true)
  const lastScroll = useRef(0)
  const scrollSave = useRef<ReturnType<typeof setTimeout> | null>(null)
  const [watches, setWatches] = useState(false)
  const [permissions, setPermissions] = useState(false)
  const [previewPath, setPreviewPath] = useState<string | null>(null)
  useEffect(() => {
    const preview = (event: Event) => setPreviewPath((event as CustomEvent<string>).detail)
    document.addEventListener('bravebot:preview-file', preview)
    return () => document.removeEventListener('bravebot:preview-file', preview)
  }, [])
  const [unseen, setUnseen] = useState(false)
  const [searching, setSearching] = useState(false)
  const [query, setQuery] = useState('')
  const [match, setMatch] = useState(0)
  const [recents, setRecents] = useState<string[]>([])
  const [dismissed, setDismissed] = useState<string | null>(null)
  const closeFind = (): void => {
    setSearching(false)
    document.querySelector<HTMLElement>('.transcript-head .find-open')?.focus()
  }
  const jump = (element: HTMLElement | null) => element?.scrollIntoView({ block: 'nearest',
    behavior: window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth' })
  const latest = () => { following.current = true; setUnseen(false); jump(bottom.current) }
  useEffect(() => { void window.bravebot.readRecents().then(setRecents).catch(() => {}) }, [live?.handle])
  useLayoutEffect(() => {
    const element = scroller.current
    if (!element) return
    const stored = conversationPreferences(storageKey).scroll
    element.scrollTop = stored ?? element.scrollHeight
    lastScroll.current = element.scrollTop
    following.current = element.scrollHeight - element.scrollTop - element.clientHeight < 80
    setUnseen(false)
    setQuery('')
    return () => {
      if (scrollSave.current) clearTimeout(scrollSave.current)
      if (storageKey) setConversation(storageKey, { scroll: lastScroll.current })
    }
  }, [storageKey, live?.handle])
  const matches = useMemo(() => {
    if (!query.trim()) return []
    return live?.entries.filter((entry) => t.searchableText(entry).toLowerCase().includes(query.toLowerCase())).map((entry) => entry.id) ?? []
  }, [query, live?.entries])
  useEffect(() => {
    const id = matches[match % (matches.length || 1)]
    if (id) document.dispatchEvent(new CustomEvent('bravebot:reveal-entry', { detail: id }))
    if (id) jump(scroller.current?.querySelector<HTMLElement>(`[data-entry-id="${id}"]`)?.firstElementChild as HTMLElement | null)
  }, [match, matches])
  useEffect(() => {
    // The menu's ⌘F arrives as FIND_EVENT. The key is still heard here for a window whose menu
    // bar never sees it, which is every key a test types over CDP.
    const find = () => {
      setSearching(true)
      requestAnimationFrame(() => document.querySelector<HTMLElement>('.find-bar .find-input')?.focus())
    }
    const key = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && !event.altKey && !event.shiftKey && event.key.toLowerCase() === 'f') {
        event.preventDefault(); find()
      }
    }
    const composer = () => input.current?.focus()
    document.addEventListener('keydown', key)
    document.addEventListener(FIND_EVENT, find)
    document.addEventListener(FOCUS_COMPOSER_EVENT, composer)
    return () => {
      document.removeEventListener('keydown', key)
      document.removeEventListener(FIND_EVENT, find)
      document.removeEventListener(FOCUS_COMPOSER_EVENT, composer)
    }
  }, [])

  /**
   * Which row a fork link is pointing at, resolved from an ordinal over the prompts.
   *
   * The ordinal is the coordinate, not the id: ids are minted fresh each time a session is
   * opened, so nothing durable can name a row. Counting prompts is what both sides of a fork
   * agree on, and it is the same count the cut was made on.
   */
  useEffect(() => {
    const reveal = (event: Event) => {
      const id = (event as CustomEvent<string>).detail
      following.current = false
      requestAnimationFrame(() => {
        const element = scroller.current?.querySelector<HTMLElement>(`[data-entry-id="${id}"]`)
        if (element) jump(element.firstElementChild as HTMLElement ?? element)
      })
    }
    document.addEventListener('bravebot:reveal-entry', reveal)
    return () => document.removeEventListener('bravebot:reveal-entry', reveal)
  }, [])

  const focused = useMemo(() => {
    if (!live || live.focus === null) return null
    // Matched against the ordinal each prompt arrived with, not counted over the rows drawn: the
    // ordinal was minted by the agent over its own messages, and a count here would be a second
    // copy of that rule. A parent with fewer prompts than it had matches nothing, and the link
    // still opened the right session, which is the honest half of what it promised.
    return live.entries.find((entry) => entry.kind === 'user' && entry.prompt === live.focus)?.id ?? null
  }, [live])

  // Read inside the effect below rather than depended on, and that is the whole point: a
  // transcript follows the conversation, but one that has been *sent* somewhere must stay
  // where it was sent. As a dependency, letting go of the mark would count as a change and
  // drag the view to the bottom a second and a half after the link landed on the row.
  const focusRef = useRef<number | null>(null)
  focusRef.current = live?.focus ?? null

  // Stable, so the memoised list below is not re-drawn by a keystroke in the composer.
  const stableDisclosure = useEvent(onTurnDisclosure)
  const stableAudit = useEvent(onAudit)
  // A decision is the reader's turn done; the next thing they do is write, so the field is
  // where focus goes back to.
  const toComposer = (): void => { requestAnimationFrame(() => input.current?.focus()) }
  const stableDecide = useEvent((...args: Parameters<Answer>) => { onDecide(...args); toComposer() })
  const stableAnswer = useEvent((...args: Parameters<AnswerQuestions>) => { onAnswer(...args); toComposer() })
  const send = useEvent(() => { latest(); onSubmit(); toComposer() })
  const queue = useEvent(onQueue)
  const cancel = useEvent(onCancel)
  const plan = useEvent(() => { latest(); onPlan?.() })
  const draftChanged = useEvent(onDraft)
  const chooseModelFor = useEvent(onModel)
  const chooseMode = useEvent(onMode)
  const attach = useEvent(onAttach)
  const removeAttachment = useEvent(onRemoveAttachment)
  const preview = useEvent((path: string) => setPreviewPath(path))
  const resumeQueued = useEvent(onResumeQueued)
  const removeQueued = useEvent(onRemoveQueued)
  const dismissRefusal = useEvent(onDismissRefusal)
  const setup = useEvent(onSetup)
  const checkBackend = useEvent(onCheckBackend)
  const diagnostics = useEvent(onDiagnostics)
  const stableFork = useEvent(onFork)
  const stableRewind = useEvent(onRewind)
  const recover = useEvent(() => {
    onDraft((draft.trim() ? `${draft}\n\n` : '') + 'Continue the previous task from the current project state. First check which actions already completed; do not repeat successful commands or writes. Resolve the last error before proceeding.')
    input.current?.focus()
  })
  const chooseModel = useEvent(() => {
    (document.querySelector('.composer .model-trigger') as HTMLButtonElement | null)?.click()
  })

  const activitySnapshot = useRef<{ handle?: string; entries?: t.Entry[]; phase?: Waiting | null }>({})
  useEffect(() => {
    const previous = activitySnapshot.current
    activitySnapshot.current = { handle: live?.handle, entries: live?.entries, phase: live?.phase }
    if (previous.handle !== live?.handle || focusRef.current !== null) return
    if (previous.entries === live?.entries && previous.phase === live?.phase) return
    if (following.current) jump(bottom.current)
    else setUnseen(true)
  }, [live?.handle, live?.entries, live?.phase])

  useEffect(() => {
    if (!focused) return
    // Instant, unlike the bottom-follow above. A smooth scroll is right for a transcript
    // growing under you and wrong for a link: it can be hundreds of lines to travel, and a
    // reader watching the intervening session fly past has been shown the journey rather than
    // the place they asked for.
    // The row inside the wrapper, not the wrapper: `.entry-hit` is `display: contents`, so it
    // has no box of its own — and an element with no box cannot be scrolled to. The same
    // reason `.entry-hit.focused` paints its child rather than itself.
    marked.current?.firstElementChild?.scrollIntoView({ behavior: 'auto', block: 'center' })
    // Let go of the mark rather than leaving it: it says "this is the row you asked for", and
    // a row that keeps saying that is a row that looks selected.
    const timer = setTimeout(onFocused, 1600)
    return () => clearTimeout(timer)
  }, [focused, onFocused])

  // The header is outside this branch on purpose. It carries the fold toggles and the
  // window's drag strip, and with no session open there would otherwise be neither: a
  // sessions column folded shut here could not be brought back, and the state outlives the
  // launch that caused it.
  const viewing = !live && !reading ? botView ?? null : null
  // The search belongs to what is on screen: a bot's page filters its list, a conversation finds
  // in its transcript. Moving between them closes it.
  useEffect(() => { setSearching(false) }, [viewing?.bot.slug, live?.handle])
  // The right toggle goes with the column: a bot's conversation with no project has none worth
  // drawing (`.app.no-context`), and neither does a window with nothing open, which is not a bot's
  // page either (`.app.no-session`).
  const contextless = live ? noProject : !botView
  const head = (
    <header className="transcript-head">
      <div className="drag" />
      <div className="head-row">
        <ColumnToggle side="left" collapsed={collapsed.left} onToggle={onToggle} attention={attention} />
        {/* Rendered even with nothing to name: it is what holds the two toggles at
            opposite ends of the header, and without it they collect in the corner. */}
        <div className="head-titles">
          {!live && reading && (
            <>
              <span className="where" data-tooltip={reading.record.directory}>
                <Label color="neutral"><Icon name="folder-open" slot="icon-before" />{projectLabel(reading.record.directory)}</Label>
              </span>
              <h1>{reading.record.title}</h1>
              <span className="plan-run-chip">Plan run</span>
            </>
          )}
          {viewing && (
            <>
              <BotAvatar seed={viewing.bot.avatar} size={16} doing="open" />
              <h1>{viewing.bot.name}</h1>
            </>
          )}
          {live && (
            <>
              {/* A bot's conversation carries its face, so whose it is reads before what it is
                  about. The project chip names the folder; its tooltip is the whole path, the half
                  that says which copy of a project this is. */}
              {bot && <BotAvatar seed={bot.avatar} size={16} doing={doing} />}
              {!noProject && (
                <span className="where" data-tooltip={live.summary.directory}>
                  <Label color="neutral"><Icon name="folder-open" slot="icon-before" />{projectLabel(live.summary.directory)}</Label>
                </span>
              )}
              <h1>{live.summary.title}</h1>
              {live.autoVetting && !noProject && (
                <span className="vetting-chip" tabIndex={0}
                  data-tooltip="Auto-vetting is on: a check that finds nothing reads content to the model without asking you.">
                  <Label color="green" mode="outline"><Icon name="shield-done" slot="icon-before" />Vetting on</Label>
                </span>
              )}
            </>
          )}
        </div>
        {viewing && <div className="conversation-toolbar">
          <IconButton icon="search" label="Search conversations" tooltip="Search conversations" pressed={searching}
            className="find-open" onClick={() => setSearching((value) => !value)} />
        </div>}
        {live && <div className="conversation-toolbar">
          <IconButton icon="search" label="Find" tooltip="Find in conversation" shortcut="⌘F" pressed={searching}
            className="find-open" onClick={() => setSearching((value) => !value)} />
          <ExportMenu canExport={canExport} includeTools={includeTools} onToggleTools={onToggleTools} onExport={onExport} />
          <IconMenu icon="more-vertical" label="More" tooltip={false} className="conversation-more" data-test="conversation-more">
            <leo-menu-item onClick={() => afterMenu(() => setPermissions(true))}>
              <span className="menu-icon-row"><Icon name="shield-done" />Permissions…</span>
            </leo-menu-item>
            <leo-menu-item onClick={() => afterMenu(() => setWatches(true))}>
              <span className="menu-icon-row"><Icon name="eye-on" />File watches…</span>
            </leo-menu-item>
          </IconMenu>
        </div>}
        {!contextless && <ColumnToggle side="right" collapsed={collapsed.right} onToggle={onToggle} />}
      </div>
    </header>
  )

  const findBar = searching && live && (
    <div className="find-bar" role="search" data-test="find-bar">
      <FindInput query={query} onQuery={(value) => { setQuery(value); setMatch(0) }}
        onClose={closeFind}
        onStep={(back) => setMatch((n) => n + (back ? matches.length - 1 : 1))} />
      <span className="find-count num" role="status">{matches.length ? `${match % matches.length + 1} of ${matches.length}` : query ? 'No matches' : ''}</span>
      <IconButton icon="carat-up" label="Previous match" shortcut="⇧↩" size="tiny" disabled={!matches.length} onClick={() => setMatch((n) => n + matches.length - 1)} />
      <IconButton icon="carat-down" label="Next match" shortcut="↩" size="tiny" disabled={!matches.length} onClick={() => setMatch((n) => n + 1)} />
      <IconButton icon="close" label="Close search" shortcut="⎋" size="tiny" onClick={closeFind} />
    </div>
  )

  const toast = problem && problem !== dismissed && (
    <Alert type="error" isToast className="problem-toast" role="alert" data-test="problem-toast">
      <Icon name="warning-circle-filled" slot="icon" />
      <span slot="title">Something went wrong</span>
      <span className="problem-text">{problem}</span>
      <Button slot="content-after" kind="plain-faint" fab aria-label="Dismiss" onClick={() => setDismissed(problem)}>
        <Icon name="close" />
      </Button>
    </Alert>
  )

  if (!live && reading) {
    return (
      <main className="transcript run-record">
        {head}
        <div className="toast-stack">{toast}<Toasts /></div>
        <div className="entries">
          <RunRecord run={reading} onNew={onNew} />
        </div>
      </main>
    )
  }


  if (!live && viewing) {
    return (
      <main className="transcript bot-view">
        {head}
        <BotView
          notices={<>{toast}<Toasts /></>}
          bot={viewing.bot}
          history={viewing.history}
          filtering={searching}
          backendReady={backendReady}
          onOpen={(summary) => onOpenBotConversation(viewing.bot, summary)}
          onStart={(prompt, directory) => onStartBotChat(viewing.bot, prompt, directory)}
          onModel={(model) => onBotModel(viewing.bot, model)}
          onSetup={setup}
          onCheckBackend={checkBackend}
          onDiagnostics={diagnostics}
        />
      </main>
    )
  }

  if (!live) {
    return (
      <main className="transcript empty-state">
        {head}
        <div className="toast-stack">{toast}<Toasts /></div>
        <div className="empty-body">
          <div className="welcome">
            <div className="welcome-mark" aria-hidden="true"><Icon name="product-brave-leo" /></div>
            <h1>What should we build?</h1>
            <p className="welcome-lede">Open a project to work with an agent in it. Changes and approvals stay in the conversation.</p>
            <div className="welcome-actions">
              <Button kind="filled" size="medium" className="welcome-open" onClick={() => onNew()} data-test="open-project">
                <Icon name="folder-open" slot="icon-before" />Open project
              </Button>
              <kbd className="welcome-shortcut">⌘N</kbd>
            </div>
            {backendReady === false && <BackendTray onSetup={setup} onCheckBackend={checkBackend} onDiagnostics={diagnostics} />}
            {!!recents.length && (
              <section className="welcome-recents" aria-labelledby="welcome-recents-title">
                <h2 id="welcome-recents-title" className="bot-view-title">Recent projects</h2>
                <div className="bot-conversations">
                  {recents.slice(0, 5).map((directory) => (
                    <button key={directory} type="button" className="bot-history-row" onClick={() => onNew(directory)}>
                      <span className="session-title"><span className="session-name">{projectLabel(directory)}</span></span>
                      <span className="session-where" data-tooltip={directory}>{directory}</span>
                    </button>
                  ))}
                </div>
              </section>
            )}
          </div>
        </div>
      </main>
    )
  }

  // Nothing said yet and nothing running: the composer is the whole of the page.
  const fresh = live.entries.length === 0 && !live.running
  return (
    <main className={`transcript${fresh ? ' fresh' : ''}`}>
      {head}
      {findBar}

      <div className="entries" ref={scroller} onScroll={(event) => {
        const element = event.currentTarget
        lastScroll.current = element.scrollTop
        if (scrollSave.current) clearTimeout(scrollSave.current)
        scrollSave.current = setTimeout(() => { if (storageKey) setConversation(storageKey, { scroll: lastScroll.current }) }, 180)
        following.current = element.scrollHeight - element.scrollTop - element.clientHeight < 80
        if (following.current) setUnseen(false)
      }}>
        {fresh && (
          <div className="fresh-greeting">
            {noProject ? <>
              <h1>{bot ? `Talk to ${bot.name}` : 'Ready'}</h1>
              <p>This conversation has no project. Pick one below to work in a folder.</p>
            </> : <>
              <h1>Ready in {footer?.choices
                ? <ProjectMenu directory={footer.directory} choices={footer.choices} onChoose={footer.onChoose} disabled={live.running} variant="title" />
                : <span className="fresh-project">{projectLabel(live.summary.directory)}</span>}</h1>
              <p>Ask for a change, a review or an explanation. Nothing is written without your approval.</p>
            </>}
          </div>
        )}
        <SessionNotices
          forkedFrom={live.forkedFrom}
          kept={live.trustRemembered ?? null}
          vetting={live.autoVetting === true}
          rules={live.rules ?? null}
          onOpenParent={onOpenParent}
          onManage={() => setPermissions(true)}
        />
        <EntryList
          entries={live.entries}
          turns={live.turns}
          running={live.running}
          focused={focused}
          matched={matches[match % (matches.length || 1)] ?? null}
          marked={marked}
          onTurnDisclosure={stableDisclosure}
          onAudit={stableAudit}
          onRecover={recover}
          onChooseModel={chooseModel}
          onDecide={stableDecide}
          onAnswer={stableAnswer}
          onFork={stableFork}
          rewind={live.rewind ?? NO_POINTS}
          onRewind={stableRewind}
        />

        {live.running && (
          <WorkingRow
            key={live.handle}
            bot={bot}
            word={workingWord(live.phase, live.checking, live.composing, live.hook)}
            tokens={live.tokens}
            turn={Object.values(live.turns).filter((turn) => turn.status === 'running').at(-1)?.turn ?? null}
            onAudit={onAudit}
          />
        )}
        <div ref={bottom} />
      </div>

      <div className="composer-dock">
        <div className="dock-float">
          {/* Kept in the tree while empty, so what arrives in it is announced. */}
          <div className="attention-pills" aria-live="polite">
            {pending && (
              <button type="button" className="attention-pill warning pending-jump" data-tooltip={waitingOn(pending.kind)} onClick={() => {
                document.dispatchEvent(new CustomEvent('bravebot:reveal-entry', { detail: pending.id }))
                const element = scroller.current?.querySelector<HTMLElement>(`[data-entry-id="${pending.id}"]`)
                jump(element ?? bottom.current)
              }}>
                {pending.kind === 'ask' ? 'Answer needed' : 'Approval needed'}<Icon name="arrow-up" />
              </button>
            )}
            {unseen && (
              <button type="button" className="attention-pill" onClick={latest}>
                Jump to latest<Icon name="arrow-down" />
              </button>
            )}
          </div>
          {toast}<Toasts />
        </div>
        <Composer
          input={input}
          session={live.handle}
          model={live.model}
          running={live.running}
          askingTrust={!!live.askingTrust}
          compacting={live.phase === 'compacting'}
          contextTokens={live.contextTokens}
          archived={live.archived}
          pending={!!pending}
          scope={bot ? 'bot' : 'conversation'}
          draft={draft}
          onDraft={draftChanged}
          onSend={send}
          onQueue={queue}
          onCancel={cancel}
          onPlan={onPlan ? plan : undefined}
          onModel={chooseModelFor}
          permissionMode={live.permissionMode}
          onMode={chooseMode}
          attachments={attachments}
          onAttach={attach}
          onRemoveAttachment={removeAttachment}
          onPreview={preview}
          queued={queued}
          queuePaused={queuePaused}
          onResumeQueued={resumeQueued}
          refusal={live.sendRefused ?? null}
          onDismissRefusal={dismissRefusal}
          onRemoveQueued={removeQueued}
          backendReady={backendReady}
          onSetup={setup}
          onCheckBackend={checkBackend}
          onDiagnostics={diagnostics}
          footer={footer}
        />
      </div>
      {/* Takes the space under a fresh session's composer, which holds it in the middle of the
          column; it gives the space up when the first message is sent. */}
      <div className="composer-spacer" aria-hidden="true" />
      {watches && <Watches session={live.handle} onClose={() => setWatches(false)} />}
      {permissions && <Permissions session={live.handle} onClose={() => setPermissions(false)} onRemembered={(kept) => onTrustRemembered(live.handle, kept)} />}
      {previewPath && <FilePreview session={live.handle} path={previewPath} onClose={() => setPreviewPath(null)} />}
    </main>
  )
}

/** The three files a conversation can become. Ordered plainest first. */
const FORMATS: readonly { id: ExportFormat; label: string; detail: string; icon: IconName }[] = [
  { id: 'txt', label: 'Plain Text', detail: '.txt', icon: 'file-text' },
  { id: 'md', label: 'Markdown', detail: '.md', icon: 'file-code' },
  { id: 'pdf', label: 'PDF', detail: '.pdf', icon: 'file-text' },
]

/**
 * The control that writes the conversation to a file.
 *
 * The whole button opens the menu rather than a split control like `NewSession`'s. That one
 * splits because opening a folder picker is overwhelmingly the common case and the recents
 * list is the exception; here there is no format worth guessing at, and a button that
 * exported a `.txt` because somebody clicked slightly to the left would be worse than one
 * that always asks.
 */
function ExportMenu({
  canExport,
  includeTools,
  onToggleTools,
  onExport,
}: {
  canExport: boolean
  includeTools: boolean
  onToggleTools: () => void
  onExport: (format: ExportFormat) => void
}): React.JSX.Element {
  return (
    <IconMenu icon="download" label="Export" tooltip={canExport ? 'Export conversation' : 'Nothing has been said yet'}
      disabled={!canExport} className="export-menu" triggerClassName="export-open">
      {/* What the file will contain, asked above what it will be called. A native save
          panel takes no controls of ours, so the question lives here, ticked or not, and
          the File menu carries the same row: see `session.export-tools` in
          `shared/commands.ts`. The tick is the state; a second line would say it twice.
          Choosing it closes the menu, the way a checkable menu item does. */}
      <leo-menu-item className="export-tools" data-role="menuitemcheckbox" aria-checked={includeTools ? 'true' : 'false'} onClick={() => onToggleTools()}>
        <span className="menu-icon-row">
          Include Tool Calls
          <span className="menu-check" aria-hidden="true">{includeTools && <Icon name="check-normal" />}</span>
        </span>
      </leo-menu-item>
      <hr />
      {FORMATS.map((format) => (
        <leo-menu-item key={format.id} onClick={() => onExport(format.id)}>
          <span className="menu-icon-row">
            <Icon name={format.icon} />
            <span className="export-format">{format.label} <span className="menu-detail">({format.detail})</span></span>
          </span>
        </leo-menu-item>
      ))}
    </IconMenu>
  )
}

function phaseWord(phase: Waiting): string {
  // The agent's own words, so the two interfaces say the same thing about the same wait.
  if (phase === 'starting-servers') return 'Starting MCP servers'
  return phase === 'planning'
    ? 'Planning'
    : phase === 'thinking'
      ? 'Thinking'
      : phase === 'compacting'
        ? 'Compacting'
        : 'Reconnecting'
}

/**
 * What a finished call spent at a model of its own, written as the terminal writes it.
 *
 * Empty for a call that asked none and for one the agent rounded to `0`: `0s at the model` answers
 * nothing.
 */
function waitedWord(waited: number | null): string {
  if (!waited) return ''
  const elapsed = waited < 60 ? `${waited}s` : `${Math.floor(waited / 60)}m ${String(waited % 60).padStart(2, '0')}s`
  return ` · ${elapsed} at the model`
}

/**
 * What the session is waiting on. A running hook wins over everything, since it holds the turn and
 * says what it is; a running check wins over the phase, which a check does not
 * change, and one function serves both places the word is drawn so they cannot disagree. A call
 * being written follows the check: the phase is the same for the whole wait, which can be minutes.
 */
export function workingWord(phase: Waiting | null, checking: Checking | null, composing: string | null = null, hook: Hook | null = null): string {
  if (hook !== null) return `Running hook: ${hook.program} (${hook.moment})`
  if (checking !== null && 'file' in checking) return checking.file === 'pdf' ? 'Checking a PDF' : 'Checking a picture'
  if (checking !== null) return `Checking ${checking.lines} ${checking.lines === 1 ? 'line' : 'lines'}`
  if (composing !== null) return `Preparing a call: ${composing}`
  return phase ? phaseWord(phase) : 'Working'
}

/**
 * The line that says a turn is under way: what it is doing, for how long, and how much it has
 * written. Stopping it is the composer's Stop, which is where the eye already is while waiting;
 * a second Cancel here was one more thing to aim at for the same act.
 */
function WorkingRow({ bot, word, tokens, turn, onAudit }: {
  bot: Bot | null
  word: string
  tokens: number
  turn: number | null
  onAudit: OpenAudit
}): React.JSX.Element {
  const [since] = useState(() => Date.now())
  const [now, setNow] = useState(since)
  useEffect(() => {
    const tick = setInterval(() => setNow(Date.now()), 1000)
    return () => clearInterval(tick)
  }, [])
  const seconds = Math.floor((now - since) / 1000)
  const elapsed = seconds < 60 ? `${seconds}s` : `${Math.floor(seconds / 60)}m ${String(seconds % 60).padStart(2, '0')}s`
  return (
    <div className={`working${bot ? ' working-bot' : ''}`}>
      {/* For a bot, the bot itself, looking down at the page — the one place in the transcript
          its face carries something the header does not: it is *here*, at the point of
          attention, only while something is happening, and its posture is the indicator. It
          mounts already working, since the row exists only while a turn runs. A plain session
          has no face and keeps the spinner. */}
      {bot ? (
        <BotAvatar seed={bot.avatar} size={20} doing="working" />
      ) : (
        <ProgressRing mode="indeterminate" className="working-ring" data-test="working-ring" />
      )}
      <span className="working-word">{word}…</span>
      <span className="count">
        {seconds > 0 && elapsed}
        {tokens > 0 && `${seconds > 0 ? ' · ' : ''}${tokens.toLocaleString()} tokens written`}
      </span>
      {turn !== null && <IconButton icon="shield-done" label="Audit" tooltip="Audit this turn" size="tiny" className="turn-audit-link"
        controls="turn-audit-inspector" onClick={(event) => onAudit(turn, event.currentTarget as HTMLButtonElement)} />}
    </div>
  )
}

/**
 * Consecutive tool calls, gathered into one run.
 *
 * A turn that reads five files and writes five more puts ten lines between the question
 * and the answer, and they are the least interesting thing on screen once it is over. A
 * run of them is one thing that happened, so it is drawn as one thing that can be put away.
 *
 * Only calls are gathered. A confirmation is waiting on an answer and confined content is
 * the point of the tool, so neither is ever swept into a fold with a lid on it.
 */
type Run = { kind: 'one'; entry: t.Entry } | { kind: 'run'; id: string; entries: t.Entry[] }

const isCall = (entry: t.Entry): boolean =>
  entry.kind === 'tool' || entry.kind === 'replayed-tool'

export function runs(entries: t.Entry[]): Run[] {
  const out: Run[] = []
  for (const entry of entries) {
    const last = out[out.length - 1]
    if (isCall(entry) && last?.kind === 'run') last.entries.push(entry)
    else if (isCall(entry)) out.push({ kind: 'run', id: entry.id, entries: [entry] })
    else out.push({ kind: 'one', entry })
  }
  // A run of one is just a line. Giving it a header and a chevron would be more furniture
  // than the thing it contains.
  return out.map((run) =>
    run.kind === 'run' && run.entries.length === 1 && run.entries[0]
      ? { kind: 'one', entry: run.entries[0] }
      : run,
  )
}

/**
 * Every entry, drawn. Memoised on its props, which are the entries and stable callbacks, so
 * nothing but a change to the conversation re-draws it.
 */
const EntryList = memo(function EntryList({
  entries, turns, running, focused, matched, marked,
  onTurnDisclosure, onAudit, onRecover, onChooseModel, onDecide, onAnswer, onFork, rewind, onRewind,
}: {
  entries: t.Entry[]
  turns: Turns
  running: boolean
  focused: string | null
  matched: string | null
  marked: React.RefObject<HTMLDivElement | null>
  onTurnDisclosure: (turn: number, field: TurnDisclosure, open: boolean) => void
  onAudit: OpenAudit
  onRecover: () => void
  onChooseModel: () => void
  onDecide: Answer
  onAnswer: AnswerQuestions
  onFork: (id: string) => void
  rewind: RewindPoint[]
  onRewind: (steps: number) => void
}): React.JSX.Element {
  const grouped = useMemo(() => runs(entries), [entries])
  // The current turn is everything after its `turn-start`; its runs are the ones still open.
  const current = useMemo(() => {
    if (!running) return null
    let from = entries.length - 1
    while (from >= 0 && entries[from]!.kind !== 'turn-start') from--
    return new Set(entries.slice(from + 1).map((entry) => entry.id))
  }, [entries, running])
  const lastReply = useMemo(() => [...entries].reverse().find((entry) => entry.kind === 'assistant')?.id, [entries])
  const undoing = useMemo(() => undoRow(entries, rewind), [entries, rewind])
  const undo = useMemo(() => rewind[0] && { running, onUndo: () => onRewind(rewind[0]!.steps) }, [rewind, running, onRewind])
  return <>
    {grouped.map((run) =>
      run.kind === 'run' ? (
        <ToolRun key={run.id} entries={run.entries} active={current?.has(run.id) ?? false} />
      ) : (
        // Wrapped only to catch the right-click. `display: contents` keeps the wrapper
        // out of the layout entirely, so the bubbles flow exactly as they did — the
        // alternative was an `onContextMenu` on each of the eleven shapes `Row` returns.
        <div
          key={run.entry.id}
          data-entry-id={run.entry.id}
          className={`entry-hit${run.entry.id === focused || run.entry.id === matched ? ' focused' : ''}${run.entry.id === lastReply ? ' last-reply' : ''}`}
          ref={run.entry.id === focused ? marked : undefined}
          // A prompt is the one row that came from the person reading it, and the only one
          // a fork can be cut in front of, so it is a different kind of thing to
          // right-click. The menu it gets is still decided in the main process.
          onContextMenu={contextMenu(
            run.entry.kind !== 'user' ? 'entry'
              : pointForPrompt(rewind, run.entry.prompt) ? 'entry-user-rewindable' : 'entry-user',
            run.entry.id,
          )}
        >
          {run.entry.kind === 'turn-start' ? <TurnNotices details={turns[run.entry.number]} onDisclosure={onTurnDisclosure} /> : <Row
            entry={run.entry}
            onRecover={onRecover}
            onChooseModel={onChooseModel}
            onDecide={onDecide}
            onAnswer={onAnswer}
            onFork={onFork}
            // Greyed rather than gone while a turn runs, the way the menu item is: a
            // control that disappears is one the reader has to go looking for again. Greyed
            // too for a prompt this window has sent and not yet been told the ordinal of:
            // that prompt is in the conversation, but nothing here knows where.
            forkable={!running && (run.entry.kind !== 'user' || run.entry.prompt !== undefined)}
          />}
          {(run.entry.kind === 'assistant' || (run.entry.kind === 'error' && run.entry.turn !== undefined)) &&
            <TurnFooter details={run.entry.turn === undefined ? undefined : turns[run.entry.turn]} onDisclosure={onTurnDisclosure} onAudit={onAudit}
              copy={run.entry.kind === 'assistant' ? run.entry.text : undefined}
              undo={run.entry.id === undoing ? undo : undefined} />}
        </div>
      ),
    )}
  </>
})

const nothing = (): undefined => undefined
const NO_POINTS: RewindPoint[] = []

/**
 * What a run did, in the agent's own verbs: "Read 3, Edit 1". Counted as sent rather than sorted
 * into kinds, because the verbs arrive in the reader's language and a list of English words to
 * match them against would miscount every other one.
 */
export function runSummary(entries: t.Entry[]): string {
  const counts = new Map<string, number>()
  for (const entry of entries) {
    const verb = entry.kind === 'tool' ? entry.activity.verb : entry.kind === 'replayed-tool' ? 'Replayed' : null
    if (verb) counts.set(verb, (counts.get(verb) ?? 0) + 1)
  }
  const verbs = [...counts].map(([verb, count]) => `${verb} ${count}`).join(', ')
  return `Worked through ${entries.length} step${entries.length === 1 ? '' : 's'}${verbs ? ` · ${verbs}` : ''}`
}

/**
 * A run of calls, folded under one line.
 *
 * Open while its turn is running, so the calls can be watched as they land, and folded when the
 * turn is over, when they are the least interesting thing between the question and the answer.
 * A saved conversation opens with every run folded for the same reason.
 */
const ToolRun = memo(function ToolRun({ entries, active }: { entries: t.Entry[]; active: boolean }): React.JSX.Element {
  const [open, setOpen] = useState(active)
  useEffect(() => { setOpen(active) }, [active])
  useEffect(() => {
    const reveal = (event: Event) => { if (entries.some((entry) => entry.id === (event as CustomEvent<string>).detail)) setOpen(true) }
    document.addEventListener('bravebot:reveal-entry', reveal)
    return () => document.removeEventListener('bravebot:reveal-entry', reveal)
  }, [entries])
  const working = active && entries.some((entry) => entry.kind === 'tool' && entry.activity.note === null)

  return (
    <section className={`tool-run ${open ? 'open' : ''}`}>
      <Collapse className="flat-collapse tool-run-collapse" isOpen={open} onToggle={({ open: next }) => setOpen(next)}
        title={runSummary(entries)} data-test="tool-run">
        <span slot="icon" className="tool-run-status" aria-hidden="true">
          {working ? <ProgressRing className="tool-ring" /> : <Icon name="window-console" />}
        </span>
        <div className="tool-run-lines">
          {entries.map((entry) => (
            <div key={entry.id} data-entry-id={entry.id}><Row
              key={entry.id}
              entry={entry}
              onDecide={nothing}
              onAnswer={nothing}
              // A run holds tool lines and nothing else, so neither of these can be reached
              // from in here — and a folded row carries no right-click either.
              onFork={nothing}
              forkable={false}
            /></div>
          ))}
        </div>
      </Collapse>
    </section>
  )
}, (before, after) => before.active === after.active && before.entries.length === after.entries.length
  && before.entries.every((entry, index) => entry === after.entries[index]))

/**
 * A series of questions the planner is putting to the person.
 *
 * The one question in the interface that is not a yes or a no, so it holds its own state
 * until it is sent: several questions arrive together and are answered together, in one
 * reply, because the turn is blocked on the series rather than on any one of them.
 *
 * A question with no rows is not a mistake — it can only be answered in the person's own
 * words — and every question keeps a free-text box for the same reason: the model's options
 * may all be wrong, and forcing a choice between them would put words in somebody's mouth.
 */
function Questions({
  request,
  answers,
  onAnswer,
}: {
  request: t.Entry & { kind: 'ask' }
  answers: AskAnswer[] | null
  onAnswer: AnswerQuestions
}): React.JSX.Element {
  const prompts = request.request.prompts
  const [picked, setPicked] = useState<number[][]>(() => prompts.map(() => []))
  const [typed, setTyped] = useState<string[]>(() => prompts.map(() => ''))

  const choose = (question: number, index: number, multiple: boolean): void => {
    setPicked((old) =>
      old.map((chosen, at) => {
        if (at !== question) return chosen
        if (!multiple) return chosen.includes(index) ? [] : [index]
        return chosen.includes(index)
          ? chosen.filter((one) => one !== index)
          : [...chosen, index].sort((a, b) => a - b)
      }),
    )
  }

  /**
   * What each question would be answered with.
   *
   * Typed words win over a selection, matching what the agent does with a reply that
   * carries both: they are the more specific thing to have done. An empty answer is sent as
   * an empty object, which is how declining is said.
   */
  const collected = (): AskAnswer[] =>
    prompts.map((_, at) => {
      const words = typed[at]?.trim() ?? ''
      if (words) return { typed: words }
      const chosen = picked[at] ?? []
      return chosen.length > 0 ? { chosen } : {}
    })

  const blank = collected().filter((answer) => !answer.typed && !answer.chosen).length

  // Answered, or ended before it could be: either way the questions are a record rather than a
  // form, so they are drawn as text with no choices to press.
  if (answers || request.interrupted) {
    return (
      <div className="confirm ask">
        <CardHead icon="message-bubble" intent="Asked" subject={<span className="path">{prompts.length} question{prompts.length === 1 ? '' : 's'}</span>} />
        {prompts.map((prompt, at) => (
          // Keyed by position, not by `prompt.key`: that key is canonical *content*, and a
          // series may legitimately contain the same question twice. The order never
          // changes — the agent emits one prompt per question, in order — so the index is
          // both stable and unique where the content is only stable.
          <div className="asked-answer" key={at}>
            <div className="question">{prompt.question}</div>
            {answers && <div className="given"><Icon name="arrow-small-right" />{describe(prompt, answers[at])}</div>}
          </div>
        ))}
        {!answers && <Unanswered />}
      </div>
    )
  }

  return (
    <div className="confirm ask">
      <CardHead icon="message-bubble" intent="Asked" subject={<span className="path">{prompts.length} question{prompts.length === 1 ? '' : 's'}</span>} />

      {prompts.map((prompt, at) => (
        <fieldset className="ask-question" key={at}>
          <legend>
            <span className="header">{prompt.header}</span>
            {prompt.multiple && <span className="any">pick any</span>}
          </legend>
          <div className="question">{prompt.question}</div>

          <ul className="choices">
            {prompt.rows.map((row) => {
              const on = (picked[at] ?? []).includes(row.index)
              return (
                <li key={row.index}>
                  <button
                    className={`choice ${on ? 'picked' : ''}`}
                    aria-pressed={on}
                    onClick={() => choose(at, row.index, prompt.multiple)}
                  >
                    <Icon className="choice-mark" name={prompt.multiple ? (on ? 'checkbox-checked' : 'checkbox-unchecked') : (on ? 'radio-checked' : 'radio-unchecked')} />
                    <span className="choice-text">
                      <span className="label">{row.label}</span>
                      {row.detail && <span className="detail">{row.detail}</span>}
                    </span>
                  </button>
                </li>
              )
            })}
          </ul>

          <Input
            className="typed"
            size="small"
            value={typed[at] ?? ''}
            aria-label={`Your own answer to: ${prompt.question}`}
            placeholder={prompt.rows.length > 0 ? 'Or say something else…' : 'Your answer…'}
            onInput={({ value }) => setTyped((old) => old.map((text, index) => (index === at ? value : text)))}
          />
        </fieldset>
      ))}

      <div className="confirm-actions">
        {/* Declining every question is a real answer and the turn continues, so it is a
            button here rather than something a person has to leave blank and guess at. */}
        <Button kind="plain-faint" size="small" className="reject" onClick={() => onAnswer(request.request.request, prompts.map(() => ({})))}>
          Decline
        </Button>
        <Button kind="filled" size="small" className="approve" onClick={() => onAnswer(request.request.request, collected())}>
          Answer
          {/* Leaving a question blank declines it, which is legitimate but should not be a
              surprise — with several questions on screen it is easy to answer two of three
              and not notice. Said on the button rather than after the fact. */}
          {blank > 0 && prompts.length > 1 && (
            <span className="aside"> · {blank} declined</span>
          )}
        </Button>
      </div>
    </div>
  )
}

/**
 * What the window says about a session rather than in it, at the top of the conversation.
 *
 * Three notes, each one sentence and at most one link: that this session was cut out of another
 * (history, so it is the first thing in the transcript), that a kept yes about this directory is
 * why nothing asked (TRUST-23), and that auto-vetting was on when it opened (CHECK-11, which the
 * header chip keeps saying for as long as the session is open). A fourth, folded, lists what a
 * settings file wrote that is not in force, since its list can run long.
 *
 * None of them is a `t.Entry`, which is what keeps them out of `t.conversation` and therefore out
 * of every export: a note this window wrote about a session is not something the session said.
 * The fork link is a button and not a link, because nothing in this window navigates.
 */
function SessionNotices({ forkedFrom, kept, vetting, rules, onOpenParent, onManage }: {
  forkedFrom: { title: string; prompt: number } | null
  kept: KeptTrust | null
  vetting: boolean
  rules: SettingsRules | null
  onOpenParent: () => void
  onManage: () => void
}): React.JSX.Element | null {
  const unheld = t.notInForce(rules)
  if (!forkedFrom && !kept && !vetting && !unheld) return null
  return (
    <div className="session-notices">
      {forkedFrom && (
        <p className="fork-banner session-notice divider">
          <span className="fork-mark"><ForkIcon /></span>
          <span className="notice-text">
            Forked from <strong>{forkedFrom.title}</strong>, before prompt {forkedFrom.prompt + 1}.
          </span>
          <Button kind="plain" size="tiny" className="link" onClick={onOpenParent} data-tooltip="Show the chat this was forked from">
            View original
          </Button>
        </p>
      )}
      {kept && (
        <p className="session-banner session-notice" role="note" data-tooltip={`Remembered ${ago(kept.at)} · kept in ${kept.path}`}>
          <Icon name="shield-done" />
          <span className="notice-text">Trust is remembered for this directory, so chats started here are not asked.</span>
          <Button kind="plain" size="tiny" className="link" onClick={onManage}>Manage</Button>
        </p>
      )}
      {vetting && (
        <p className="session-banner session-notice" role="note">
          <Icon name="shield-done" />
          <span className="notice-text">
            Auto-vetting is on. A check that finds nothing reads content to the model without asking you.
            Kept in <code>~/.bravebot/vetting</code>.
          </span>
        </p>
      )}
      {unheld && rules && <RulesBanner rules={rules} />}
    </div>
  )
}

/**
 * What a settings file wrote that is not in force in this conversation.
 *
 * Drawn only where there is something to say. A rule that reads as protection and is not in
 * force is worth telling the person about, and so is an allow rule that will not stop a question
 * it was written to stop.
 */
export function RulesBanner({ rules }: { rules: SettingsRules }): React.JSX.Element {
  const refused = t.refusedPaths(rules)
  const ignored = rules.filesystemIgnored ?? []
  const count = rules.unreadable.length + rules.proposed.length + rules.directories.length + refused.length + ignored.length
  return (
    <details className="rules-banner" role="note">
      <summary className="session-notice">
        <Icon name="warning-triangle-outline" />
        <span className="notice-text">
          <strong>
            {count} permission {count === 1 ? 'setting is' : 'settings are'} not in force.
          </strong>{' '}
          Details
        </span>
      </summary>
      {rules.unreadable.length > 0 && (
        <>
          <p>These entries could not be read as rules, so they decide nothing:</p>
          <ul>
            {rules.unreadable.map((entry, index) => (
              <li key={index}>{entry.said}</li>
            ))}
          </ul>
        </>
      )}
      {rules.proposed.length > 0 && (
        <>
          <p>
            These allow rules were written by this project’s settings file. An allow rule
            answers a question for you, so only your own settings file may write one. You will
            still be asked:
          </p>
          <ul>
            {rules.proposed.map((entry, index) => (
              <li key={index}>
                <code>{entry.rule}</code> from <code>{entry.file}</code>
              </li>
            ))}
          </ul>
        </>
      )}
      {rules.directories.length > 0 && (
        <>
          <p>A settings file asked for these directories to be opened. This app opens none:</p>
          <ul>
            {rules.directories.map((directory, index) => (
              <li key={index}>
                <code>{directory}</code>
              </li>
            ))}
          </ul>
        </>
      )}
      {refused.length > 0 && (
        <>
          <p>
            These paths for the programs the agent runs are not in force. A program that
            would have been held back by a refused one is not started:
          </p>
          <ul>
            {refused.map((entry, index) => (
              <li key={index}>
                <code>{entry.key}</code> <code>{entry.path}</code>: {entry.refused}
              </li>
            ))}
          </ul>
        </>
      )}
      {ignored.length > 0 && (
        <>
          <p>A project’s settings file can refuse a path to a program and never add one, so these are not obeyed:</p>
          <ul>
            {ignored.map((entry, index) => (
              <li key={index}>
                <code>{entry.key}</code> in <code>{entry.file}</code>
              </li>
            ))}
          </ul>
        </>
      )}
    </details>
  )
}

/**
 * Where the controls were on a question the turn ended before anybody answered.
 *
 * The same slot as the `decided` line, because it is the same kind of statement: what became of
 * this question. There is nothing to press because the channel that would have carried the
 * answer is gone, and a button that silently did nothing would be worse than none.
 */
function Unanswered(): React.JSX.Element {
  return <div className="decided unanswered">Nobody answered this</div>
}

/**
 * The foot of a question answered with a yes or a no: the two buttons while it waits, what was
 * answered once it has been, and that nobody answered where its turn ended first.
 *
 * One component so that the three states are the same three on every card that uses it. The
 * state that matters is the last: a card whose turn has ended draws no button, because the
 * question it would answer is no longer waiting and a press would be an approval of nothing.
 *
 * The words are the card's own and are passed in whole. A button here says what pressing it
 * does, in the words of the thing being decided, so there is no generic yes to press from habit.
 *
 * `kind` is a parameter and never worked out from anything drawn: it chooses the method the
 * answer is sent through, and the agent checks that against the question actually waiting.
 */
function YesOrNo({
  kind,
  request,
  answerable,
  decision,
  onDecide,
  reject,
  approve,
  approved,
  rejected,
}: {
  kind: Asked
  request: number
  answerable: boolean
  decision: 'approve' | 'reject' | null
  onDecide: Answer
  reject: string
  approve: string
  approved: string
  rejected: string
}): React.JSX.Element {
  if (!answerable) return <Unanswered />
  if (decision !== null) {
    return <div className={`decided ${decision}`}>{decision === 'approve' ? approved : rejected}</div>
  }
  return (
    <div className="confirm-actions">
      <button className="reject" onClick={() => onDecide(kind, request, false)}>
        {reject}
      </button>
      <button className="approve" onClick={() => onDecide(kind, request, true)}>
        {approve}
      </button>
    </div>
  )
}

/**
 * The answers to a question with a standing third one: no, yes, and yes without asking again.
 *
 * The standing answer is its own button rather than a checkbox beside yes, as on a run, because
 * it answers questions not yet asked and should take its own deliberate press. `always` is null
 * where it cannot be given.
 */
function ThreeAnswers({
  kind,
  request,
  answerable,
  decision,
  remember,
  onDecide,
  reject,
  approve,
  always,
  alwaysTooltip,
  approved,
  approvedAlways,
  rejected,
}: {
  kind: Asked
  request: number
  answerable: boolean
  decision: 'approve' | 'reject' | null
  remember: boolean
  onDecide: Answer
  reject: string
  approve: string
  always: string | null
  alwaysTooltip: string
  approved: string
  approvedAlways: string
  rejected: string
}): React.JSX.Element {
  if (!answerable) return <Unanswered />
  if (decision !== null) {
    return (
      <div className={`decided ${decision}`}>
        {decision === 'reject' ? rejected : remember ? approvedAlways : approved}
      </div>
    )
  }
  return (
    <div className="confirm-actions">
      <button className="reject" onClick={() => onDecide(kind, request, false)}>
        {reject}
      </button>
      {always && (
        <button className="approve always" data-tooltip={alwaysTooltip} onClick={() => onDecide(kind, request, true, true)}>
          {always}
        </button>
      )}
      <button className="approve" onClick={() => onDecide(kind, request, true)}>
        {approve}
      </button>
    </div>
  )
}

/**
 * A manifest run that stopped, with what it produced.
 *
 * Three cases read differently. The person stopped it, the agent refused or failed it and said
 * why, or the model service failed. The plan and the steps that ran are shown in every case
 * where they exist, because the run somebody needs to read is the one that stopped.
 */
function PlanEnded({ ended }: { ended: ManifestError }): React.JSX.Element {
  const declined = ended.declined === true
  const title = ended.stopped
    ? 'You stopped this run'
    : declined
      ? 'The plan was declined, so nothing ran'
      : ended.problem
        ? 'The run stopped'
        : failureSummary(ended.category ?? ended.kind).title
  const detail = ended.stopped
    ? 'Steps that finished before the stop remain in the project. The run was not saved.'
    : declined
      ? 'Nothing was read or written.'
      : ended.problem ?? failureSummary(ended.category ?? ended.kind).description
  const attempt = ended.attempt
  return (
    <div className={`plan-ended ${ended.stopped || declined ? 'quiet' : 'failed'}`}>
      <strong>{title}</strong>
      <p>{detail}</p>
      {attempt?.plan && (
        <details>
          <summary>The plan</summary>
          <pre className="plan-text">{attempt.plan}</pre>
        </details>
      )}
      {!attempt?.plan && attempt?.proposed && (
        <details>
          <summary>What the planner proposed, which could not be used</summary>
          <pre className="plan-text">{attempt.proposed}</pre>
        </details>
      )}
      {!!attempt?.steps.length && (
        <details open>
          <summary>Steps that ran</summary>
          <pre className="plan-text">{attempt.steps.join('\n')}</pre>
        </details>
      )}
      {ended.record && <p className="plan-record">Saved as run <code>{ended.record}</code>.</p>}
    </div>
  )
}

/**
 * What a request reaches that nothing in the agent holds, where it reaches anything.
 *
 * Drawn by every card whose request can spend such access, in the same words, so that a
 * metadata service reached by a command and one reached by a fetch read as the same grant.
 * Nothing is drawn for a request that reaches none, which is nearly every one.
 */
function AmbientNotice({ ambient }: { ambient?: Ambient[] }): React.JSX.Element | null {
  if (!ambient?.length) return null
  return (
    <Alert type="warning" className="card-alert warn">
      <Icon name="shield-alert" slot="icon" />
      This spends access that is yours elsewhere. Nobody is asked for it at the moment it
      is used, and nothing here takes it back afterwards.
      <ul>
        {ambient.map((spent) => (
          <li key={`${spent.authority}:${spent.named}`}>
            <code>{spent.named}</code>: {t.ambientSentence(spent.authority)}
          </li>
        ))}
      </ul>
    </Alert>
  )
}

/** What somebody answered, in words, for the record left in the transcript. */
function describe(prompt: AskPrompt, answer: AskAnswer | undefined): string {
  if (!answer) return 'Declined'
  if (answer.typed) return answer.typed
  const chosen = answer.chosen ?? []
  if (chosen.length === 0) return 'Declined'
  return chosen
    .map((index) => prompt.rows.find((row) => row.index === index)?.label ?? `#${index}`)
    .join(', ')
}

type RowProps = {
  entry: t.Entry
  onRecover?: () => void
  onChooseModel?: () => void
  onDecide: Answer
  onAnswer: AnswerQuestions
  onFork: (id: string) => void
  /** Whether a fork can be taken at all right now: false while a turn is running. */
  forkable: boolean
}

/**
 * One transcript entry, drawn.
 *
 * Exported because this is where the marking rule lands: which entries are formatted and which
 * are shown inside a container of their own is decided here rather than in the components below,
 * so `scripts/marking.test.mjs` renders this to assert it.
 */
export const Row = memo(function Row({
  entry,
  onRecover,
  onChooseModel,
  onDecide,
  onAnswer,
  onFork,
  forkable,
}: RowProps): React.JSX.Element {
  const card = (
    <Card
      entry={entry}
      onRecover={onRecover}
      onChooseModel={onChooseModel}
      onDecide={onDecide}
      onAnswer={onAnswer}
      onFork={onFork}
      forkable={forkable}
    />
  )
  if (!entry.interrupted) return card
  // A question the turn ended before anybody answered. It keeps the card it was, with its head,
  // its label, its reach and its warnings, inside a fold saying what became of it. Summarising
  // the entry into a bare `<pre>` instead is the cheap thing to draw and releases the bytes with
  // none of the marking LAYER-5 owes them: no container of their own, no origin, no label, no
  // reach line, and for an untrusted write neither the border nor the sentence saying nobody
  // vouched for it. The turn ending declassifies nothing.
  return (
    <div className="interrupted-request">
      <strong>Request cancelled when the turn ended</strong>
      <Collapse title="Request details" isOpen={undefined} data-test="interrupted-details">
        {card}
      </Collapse>
    </div>
  )
})

/**
 * The card itself, which is the same card whether or not the turn it belongs to ended.
 *
 * What ending it changes is only whether the question can still be answered: `answerable` is
 * false for an interrupted entry, and every arm that would draw controls draws
 * `<Unanswered />` instead. Nothing about how the content is marked depends on it.
 */
function Card({
  entry,
  onRecover,
  onChooseModel,
  onDecide,
  onAnswer,
  onFork,
  forkable,
}: RowProps): React.JSX.Element {
  const answerable = entry.interrupted !== true
  switch (entry.kind) {
    case 'turn-start': return <></>
    case 'user':
      return (
        <div className="bubble user">
          <div className="bubble-text">{entry.text}</div>
          {/* Inside the row rather than beside it. The wrapper this row sits in is
              `display: contents` and has no box to hang anything off, and the bubble is the
              only thing here that knows where the row actually is on screen. */}
          <div className="message-actions">
            <CopyButton text={() => entry.text} label="Copy message" />
            <Button
              kind="plain-faint"
              size="tiny"
              fab
              className="fork-here icon-button plain-faint tiny"
              aria-label="Fork from here"
              data-tooltip={
                forkable
                  ? 'Fork from here · Start a session from what was said before this'
                  : 'Wait for the turn to finish'
              }
              isDisabled={!forkable}
              onClick={() => onFork(entry.id)}
            >
              <span slot="icon-before"><ForkIcon /></span>
            </Button>
          </div>
        </div>
      )

    case 'assistant':
      // The only formatted surface in the app. See Markdown.tsx for why it is the only one.
      return (
        <div className="bubble assistant">
          <Markdown text={entry.text} />
        </div>
      )

    case 'narration':
      return <div className="narration"><Icon name="file-text" />{entry.text}</div>

    case 'attached':
      // A line rather than a bubble, and the path rather than the contents. This is the same
      // register the tool lines are in — something that happened on the way to the reply — which
      // is what it is: a file somebody named, read at the top of a turn.
      return (
        <div className="attached" data-tooltip={entry.path}>
          <Icon name="file-text" />Read {entry.path}
        </div>
      )

    case 'consolidation':
      // The same line register as the attachment above, and deliberately not a bubble. This is
      // something that happened on the way to a reply rather than something anybody said, and the
      // whole reason it has an entry of its own is that drawing it as a prompt would claim
      // otherwise. Reached only from the record's own tag, never from a message's wording, so
      // nothing anybody types earns this row.
      return <div className="attached consolidation"><Icon name="refresh" />Asked to bring its memory up to date</div>

    case 'watch':
      return <div className="watch-turn"><Icon name="eye-on" /><strong>{entry.text}</strong><span>Automatic turn · file contents still follow normal read permissions</span></div>
    case 'error':
      return <ErrorCard category={entry.category} attempts={entry.attempts} status={entry.status} cutOff={entry.cutOff} detail={entry.text} onRetry={onRecover} onModel={onChooseModel} />

    case 'replayed-tool':
      // No outcome, because the record does not keep one. Drawn quietly for the same
      // reason: a call the agent could not even name reads as "Tool", and giving that
      // the prominence of a real line would be worse than the gap.
      return (
        <div className={`tool replayed ${entry.why ? 'has-why' : ''}`}>
          <span className="tool-status" aria-hidden="true"><Icon name="check-normal" /></span>
          <span className="tool-body">
            {entry.why && <span className="why" data-tooltip={entry.why.length > WHY_WIDTH ? entry.why : undefined}>{entry.why}</span>}
            <span className="call"><span className="verb">{entry.text}</span></span>
          </span>
        </div>
      )

    case 'tool': {
      const { activity, landing } = entry
      const running = activity.note === null
      return (
        <div className={`tool ${activity.failed ? 'failed' : ''} ${running ? 'running' : ''} ${activity.why ? 'has-why' : ''}`}>
          <span className="tool-status" role="img" aria-label={running ? 'Running' : activity.failed ? 'Failed' : 'Done'}>
            {running ? <ProgressRing className="tool-ring" /> : <Icon name={activity.failed ? 'close' : 'check-normal'} />}
          </span>
          <span className="tool-body">
            {activity.why && <span className="why" data-tooltip={activity.why.length > WHY_WIDTH ? activity.why : undefined}>{activity.why}</span>}
            <span className="call">
              <span className="verb">{activity.verb}</span>
              {activity.target && <span className="target" data-tooltip={activity.target.length > TARGET_WIDTH ? activity.target : undefined}>{middleTruncate(activity.target, TARGET_WIDTH)}</span>}
            </span>
          </span>
          {!running && (
            <span className="note">
              {activity.note}
              {waitedWord(activity.waitedSeconds)}
            </span>
          )}
          {landing && isConfined(landing) && (
            <Label mode="outline" color="blue" className="confined" data-tooltip={landingHint(landing)}>
              {landing === 'quarantined' ? 'quarantined' : 'name only'}
            </Label>
          )}
        </div>
      )
    }

    case 'quarantined': {
      const { shown } = entry
      return (
        <div className="quarantine">
          <div className="quarantine-head">
            <Icon name="shield-done" className="card-kind" />
            <span className="mark">confined</span>
            {/* Ellipsised, and it is a path: what gets cut is the part that identifies it. */}
            <span className="origin" data-tooltip={shown.origin}>
              {shown.origin}
            </span>
            <span className="label">{shown.label}</span>
          </div>
          <pre className="preview">{shown.preview.join('\n')}</pre>
          <div className="quarantine-foot">
            {shown.lines} line{shown.lines === 1 ? '' : 's'} total ·{' '}
            {shown.reach === 'no_model'
              ? 'in no model’s context'
              : 'not in the planner’s context'}
            <Collapse className="card-details" title="Details" isOpen={undefined}>
              {shown.reach === 'no_model'
                ? 'Nothing can be sent to read this. Only its line count and origin are known to the conversation.'
                : 'A processor can be sent to read it in isolation. What it writes back is marked untrusted in turn.'}
            </Collapse>
          </div>
        </div>
      )
    }

    case 'confirm': {
      const { request, decision } = entry
      const stats = t.diffStats(request.changes)
      return (
        <DecisionCard
          className={`confirm ${request.untrusted ? 'untrusted' : ''}`}
          answerable={answerable}
          subject={request.path}
          decided={decision === null ? null : { tone: decision, text: decision === 'approve' ? 'You approved this write' : 'You refused this write' }}
          head={<CardHead icon="edit-box" intent={INTENT_WORD[request.intent]}
            subject={<code className="path" data-tooltip={request.path}>{request.path}</code>}
            counts={<span className="counts" aria-label={`${stats.added} added, ${stats.removed} removed`}><span className="stat added">+{stats.added}</span><span className="stat removed">−{stats.removed}</span></span>}
            trust={request.untrusted && <Label mode="loud" color="yellow" className="trust-label"><Icon name="warning-triangle-filled" slot="icon-before" />Untrusted source</Label>} />}
          actions={<>
            <Button kind="plain-faint" size="small" className="reject" onClick={() => onDecide('confirm', request.request, false)}>
              Don’t write
            </Button>
            <Button kind="filled" size="small" className="approve" onClick={() => onDecide('confirm', request.request, true)}>
              {request.existing ? 'Apply this change' : 'Create this file'}
            </Button>
          </>}
        >
          {request.untrusted && (
            <p className="warn">
              <Icon name="warning-triangle-filled" />
              <span>This came from somewhere nobody vouched for. The agent never read it — an
              isolated processor wrote it. Read it as you would a stranger’s patch.</span>
            </p>
          )}
          {!request.exact && (
            <p className="warn">
              <Icon name="warning-triangle-filled" />
              <span>The files were too dissimilar to diff exactly. This is an approximation of
              the change.</span>
            </p>
          )}
          {request.writtenSinceCheckout && (
            <p className="warn">
              <Icon name="warning-triangle-filled" />
              <span>This session wrote to this file in the working directory after the checkout was
              made, so it may hold changes the checkout’s copy does not. Read the difference before
              approving.</span>
            </p>
          )}
          {request.lineEndings && <p className="permission-scope" data-test="line-endings">{request.lineEndings}</p>}
          {request.credentials && request.credentials.length > 0 && <Alert type="warning" className="card-alert credential-finding">
            <Icon name="shield-alert" slot="icon" />
            <span slot="title">This looks like it would put a secret in the tree</span>
            <ul>{request.credentials.map((found) => <li key={found}>{found}</li>)}</ul>
            <small>Going by the name beside the value and how the value reads. Nothing recognised it as a particular provider’s key, so it is a guess and yours to settle.</small>
          </Alert>}
          <Diff changes={request.changes} path={request.path} untrusted={request.untrusted} />
          <Collapse className="card-details" title="Details" isOpen={undefined}>
            <p className="permission-scope">{request.existing ? 'Update an existing project file.' : 'Create a new project file.'} This decision applies to the change shown above.</p>
            {request.remark && <div className="processor-remark"><strong>Processor’s remark · untrusted</strong>
              <pre>{request.remark.preview.join('\n')}</pre>
              <small>{request.remark.label}{request.remark.lines > request.remark.preview.length ? ` · ${request.remark.lines - request.remark.preview.length} more lines not shown` : ''}. Review the diff before approving.</small>
            </div>}
          </Collapse>
        </DecisionCard>
      )
    }

    case 'run': {
      const { request, decision, remember } = entry
      const commands = request.stages.length
      const lent = request.requestedScopes ?? []
      return (
        <DecisionCard
          className={`confirm run ${request.releasesPrivate ? 'releases' : ''}`}
          answerable={answerable}
          subject={request.stages.map((stage) => stage.display).join(' | ')}
          decided={decision === null ? null : {
            tone: decision,
            text: decision === 'reject' ? 'You refused this command' : remember ? 'You ran this and vouched for the programs' : 'You ran this once',
          }}
          head={<CardHead icon="window-console" intent={commands === 1 ? 'Run a command' : `Run ${commands} commands`}
            subject={<code className="path" data-tooltip={request.directory}>{request.directory}</code>} />}
          actions={<>
            <Button kind="plain-faint" size="small" className="reject" onClick={() => onDecide('run', request.request, false)}>
              Don’t run
            </Button>
            {/* Separate from "Run once" rather than a checkbox beside it: remembering
                answers every later question about these programs, so it should take its
                own deliberate press. The tooltip says exactly what it would cover. */}
            {!lent.length && (
              <Button
                kind="outline" size="small" className="approve always"
                data-tooltip={`Stop asking about: ${request.vouches.map((v) => v.display).join(', ')}`}
                onClick={() => onDecide('run', request.request, true, true)}
              >
                Trust command and output
              </Button>
            )}
            <Button kind="filled" size="small" className="approve" onClick={() => onDecide('run', request.request, true)}>
              Run once
            </Button>
          </>}
        >
          {/* The line the planner wrote, above the plan and as context. It is not what the
              answer binds to: two spellings that compile alike are one thing to agree to, and
              the plan below is the one being agreed to. Drawn all the same, because a reader
              comparing the two is what would catch a compiler that got the line wrong. */}
          {request.line && <p className="permission-scope"><strong>The model wrote:</strong> <code>{request.line}</code></p>}

          {/* The argv, one stage per line, with what each name resolved to underneath.
              Both are shown because they are two different claims: $PATH decides what
              `grep` means, and a person vouching for a program should be looking at the
              binary rather than the word. */}
          {request.plan && <p className="permission-scope"><strong>Execution plan:</strong> <code>{request.plan}</code></p>}
          {request.stdin && <p className="permission-scope"><strong>Standard input:</strong> <code>{request.stdin}</code></p>}
          {!!request.writes?.length && <div className="permission-scope"><strong>Files created or modified:</strong><ul>{request.writes.map(path => <li key={path}><code>{path}</code></li>)}</ul></div>}
          <ol className="stages">
            {request.stages.map((stage, index) => (
              <li key={index}>
                <code className="argv">{stage.display}</code>
                <span className="resolved">
                  {stage.startedAs && stage.startedAs !== stage.resolved && `${stage.startedAs} -> `}
                  {stage.resolved ?? 'not found on PATH'}
                </span>
              </li>
            ))}
          </ol>

          <AmbientNotice ambient={request.ambient} />
          {!!lent.length && (
            <Alert type="warning" className="card-alert warn">
              <Icon name="shield-alert" slot="icon" />
              The model asked for <strong>{lent.join(', ')}</strong> to be added to every
              command in this line. Each lets a program use credentials or caches it would
              otherwise be refused. You are asked every time, and no answer is remembered.
            </Alert>
          )}
          {request.releasesPrivate && (
            <Alert type="warning" className="card-alert warn">
              <Icon name="shield-alert" slot="icon" />
              This hands your own data to the program. Whatever it does with those bytes
              happens somewhere the agent stops governing them.
            </Alert>
          )}
          {/* Outside the fold: "Trust command and output" is a standing grant, and what it covers
              has to be readable at the moment it can be pressed, not only on hover or after a click. */}
          {answerable && decision === null && !lent.length && <p className="permission-scope"><strong>Remembered approval:</strong> {request.vouches.map((v) => v.display).join('; ')}. Covers these exact commands and trusts their output for this conversation, including after reopening it. Revoke through Permissions.</p>}
          <Collapse className="card-details" title="Details" isOpen={undefined}>
            <p className="permission-scope">Run this command in the project folder shown above. “Run once” approves only this execution.</p>
          </Collapse>
        </DecisionCard>
      )
    }

    case 'output': {
      const { request, decision } = entry
      return (
        <DecisionCard
          className="confirm output"
          answerable={answerable}
          subject={request.command}
          decided={decision === null ? null : { tone: decision, text: decision === 'approve' ? 'You let the planner read this' : 'You kept this out of the planner’s context' }}
          head={<CardHead icon="eye-on" intent="Read output"
            subject={<code className="path" data-tooltip={request.command}>{request.command}</code>}
            counts={<span className="counts">{request.lines} line{request.lines === 1 ? '' : 's'}</span>}
            trust={<VettingVerdict vetting={request.vetting} />} />}
          actions={<>
            <Button
              kind="plain-faint" size="small" className="reject"
              onClick={() => onDecide('output', request.request, false)}
            >
              Keep it out
            </Button>
            <Button
              kind="filled" size="small" className="approve"
              onClick={() => onDecide('output', request.request, true)}
            >
              Let the planner read it
            </Button>
          </>}
        >
          <VettingReason vetting={request.vetting} />
          <p className="warn">
            <Icon name="warning-triangle-filled" />
            <span>The planner has not seen this. Read it yourself before deciding: approving is
            what puts it into the model’s context, and anything in here that reads like an
            instruction will be read there as one.</span>
          </p>

          {/* In full, never truncated. The answer to this question rests on the bytes, so
              a preview would be asking for an approval of what nobody saw. */}
          <pre className="preview">{request.output}</pre>
          <Collapse className="card-details" title="Details" isOpen={undefined}>
            <VettingNotice vetting={request.vetting} />
          </Collapse>
        </DecisionCard>
      )
    }

    case 'vet': {
      const { request, decision } = entry
      if (request.picture) {
        const picture = request.picture
        return <DecisionCard className="confirm vetted-read" answerable={answerable} subject={request.origin}
          decided={decision === null ? null : { tone: decision, text: decision === 'approve' ? 'You allowed this file once' : 'You kept this file out' }}
          head={<CardHead icon="eye-on" intent="See once" subject={<code className="path" data-tooltip={request.origin}>{request.origin}</code>}
            counts={<span className="counts">{picture.media}, {picture.bytes} bytes</span>} trust={<VettingVerdict vetting={request.vetting} />} />}
          actions={<>
            <Button kind="plain-faint" size="small" className="reject" onClick={() => onDecide('vet', request.request, false)}>Keep it out</Button>
            <Button kind="filled" size="small" className="approve" onClick={() => onDecide('vet', request.request, true)}>Let the planner see it once</Button>
          </>}>
          <VettingReason vetting={request.vetting} />
          <p className="permission-scope">Expected contents: {request.expects}</p>
          <p className="warn"><Icon name="warning-triangle-filled" /><span>A model reads words in a picture that a person can miss: small, faint, or nearly the colour of what is behind them. Look for writing before letting it through.</span></p>
          {picture.media === 'application/pdf' && <p className="warn"><Icon name="warning-triangle-filled" /><span>A PDF can also hold text that no page draws, and the planner is given that text too.</span></p>}
          <p>Open this copy to see what the planner would be shown. It is deleted when you answer:</p>
          <pre className="preview">{picture.path}</pre>
          <Collapse className="card-details" title="Details" isOpen={undefined}>
            <p className="permission-scope">Approval shows the planner only this file. It does not trust this file for future reads.</p>
            <VettingNotice vetting={request.vetting} />
          </Collapse>
        </DecisionCard>
      }
      return <DecisionCard className="confirm vetted-read" answerable={answerable} subject={request.origin}
        decided={decision === null ? null : { tone: decision, text: decision === 'approve' ? 'You allowed this content once' : 'You kept this content out' }}
        head={<CardHead icon="eye-on" intent="Read once" subject={<code className="path" data-tooltip={request.origin}>{request.origin}</code>}
          counts={<span className="counts">{request.lines} lines</span>} trust={<VettingVerdict vetting={request.vetting} />} />}
        actions={<>
          <Button kind="plain-faint" size="small" className="reject" onClick={() => onDecide('vet', request.request, false)}>Keep it out</Button>
          <Button kind="filled" size="small" className="approve" onClick={() => onDecide('vet', request.request, true)}>Let the planner read once</Button>
        </>}>
        <VettingReason vetting={request.vetting} />
        <p className="permission-scope">Expected contents: {request.expects}</p>
        <pre className="preview">{request.content}</pre>
        <Collapse className="card-details" title="Details" isOpen={undefined}>
          <p className="permission-scope">Approval lets the planner read only this content. It does not trust this file for future reads.</p>
          <VettingNotice vetting={request.vetting} />
        </Collapse>
      </DecisionCard>
    }
    case 'fetch': {
      const { request, decision } = entry
      return (
        <div className={`confirm fetch ${request.ambient?.length ? 'spends' : ''}`}>
          {/* The address as the planner wrote it, drawn as text and never as a link: nothing
              in a question may be something a click follows. */}
          <div className="confirm-head">
            <span className="intent">fetch</span>
            <code className="path">{request.url}</code>
          </div>

          {/* The host on a line of its own, as the agent read it out of the address. An
              address can be written so that a reader takes one name from it and the request
              goes to another, and the host is what a yes agrees to talk to. */}
          <p className="permission-scope fetch-host">
            <strong>Talking to:</strong> <code>{request.host}</code>
          </p>

          <AmbientNotice ambient={request.ambient} />

          <p className="permission-scope">
            What comes back stays confined however you answer: the model can pass it to a
            processor or write it to a file, and cannot read it or be told what it says.
            “Fetch once” covers this address only. Nothing is remembered, so the next fetch
            asks again.
          </p>

          <YesOrNo
            kind="fetch"
            request={request.request}
            answerable={answerable}
            decision={decision}
            onDecide={onDecide}
            reject="Don’t fetch"
            approve="Fetch once"
            approved="You allowed this fetch"
            rejected="You refused this fetch"
          />
        </div>
      )
    }

    case 'server': {
      const { request, decision } = entry
      return (
        <div className={`confirm server ${request.runsBuildTooling ? 'builds' : ''}`}>
          <div className="confirm-head">
            <span className="intent">start server</span>
            <span className="path server-language">{request.language} language server</span>
          </div>

          {/* The resolved path is shown because `$PATH` decides what the name runs. */}
          <p className="permission-scope">
            <strong>Program:</strong> <code>{request.program}</code>
          </p>
          <p className="permission-scope">
            <strong>Indexes:</strong> <code>{request.workspace}</code>
          </p>

          {/* A server that runs build tooling executes code from dependencies (LSP-5). */}
          {request.runsBuildTooling ? (
            <p className="warn">
              Starting it runs the build tooling of its ecosystem, so code from your
              dependencies runs with your own access, the way a build or a test run does. It
              is not confined. Files it writes are not tracked, so undoing a turn in the
              terminal may not put them back.
            </p>
          ) : (
            <p className="permission-scope">
              It reads the project with your own access. Nothing is written to your project.
            </p>
          )}

          <p className="permission-scope">
            It stays running for this conversation and stops when the conversation closes.
            What it reports stays on the same footing however you answer: a place in a file
            is shown to the model, and the text at that place stays confined unless you
            vouched for the file.
          </p>

          <YesOrNo
            kind="server"
            request={request.request}
            answerable={answerable}
            decision={decision}
            onDecide={onDecide}
            reject="Don’t start"
            approve="Start for this conversation"
            approved="You started this server for the conversation"
            rejected="You refused this server"
          />
        </div>
      )
    }

    case 'manifest': {
      const { request, decision } = entry
      return (
        <div className="confirm manifest">
          <div className="confirm-head">
            <span className="intent">run plan</span>
            <span className="path manifest-count">
              {request.steps.length} step{request.steps.length === 1 ? '' : 's'}
            </span>
          </div>

          <p className="permission-scope">
            <strong>You asked:</strong> {request.task}
          </p>

          {/* Every step, in order and unshortened. The answer covers the whole plan. The agent
              numbers each line, so the list draws no numbers of its own. */}
          <ol className="manifest-steps">
            {request.steps.map((step, index) => (
              <li key={index}>
                <code>{step}</code>
              </li>
            ))}
          </ol>

          <p className="permission-scope">
            These steps run in this order, and nothing re-plans once the run starts. Approving
            the plan does not approve its writes: each write is still put to you when its step
            is reached. This answer covers this plan only.
          </p>

          {!!entry.unheld?.length && (
            <div className="warn">
              Permission rules are not applied to a plan run. Check the steps against the
              rules this conversation refuses or asks about:
              <ul>
                {entry.unheld.map((rule, index) => (
                  <li key={index}>
                    <code>{rule}</code>
                  </li>
                ))}
              </ul>
            </div>
          )}

          <YesOrNo
            kind="manifest"
            request={request.request}
            answerable={answerable}
            decision={decision}
            onDecide={onDecide}
            reject="Don’t run"
            approve="Run this plan"
            approved="You approved this plan"
            rejected="You declined this plan"
          />
        </div>
      )
    }

    case 'exposure': {
      const { request, decision } = entry
      const found = request.credentials.length
      return (
        <div className="confirm exposure">
          <div className="confirm-head">
            <span className="intent">send file</span>
            <code className="path">{request.path}</code>
            <span className="counts">
              {found} finding{found === 1 ? '' : 's'}
            </span>
          </div>

          <p className="warn">
            The model asked to read this file, and what it reads goes to whoever performs
            inference. The scan found something in it that looks like a credential. Sending
            the file discloses that value.
          </p>

          {/* Each finding as the agent wrote it: a kind, a place and a mask. The value is
              not sent to this window, so there is none here to draw. */}
          <div className="permission-scope">
            <strong>What the scan found, without any of the value:</strong>
            <ul className="exposure-findings">
              {request.credentials.map((finding, index) => (
                <li key={index}>
                  <code>{finding}</code>
                </li>
              ))}
            </ul>
          </div>

          <p className="permission-scope">
            Keeping it back keeps this file’s text from the model and changes nothing else.
            Your answer covers this file until this conversation closes. It is not saved, and
            it does not change whether the file is trusted.
          </p>

          <YesOrNo
            kind="exposure"
            request={request.request}
            answerable={answerable}
            decision={decision}
            onDecide={onDecide}
            reject="Keep it back"
            approve="Send it anyway"
            approved="You sent this file to the model"
            rejected="You kept this file back"
          />
        </div>
      )
    }

    case 'mcp-server': {
      const { request, decision, remember } = entry
      const local = request.transport === 'stdio'
      return (
        <div className={`confirm mcp-server ${request.fetching.length || request.changed ? 'fetches' : ''}`}>
          <div className="confirm-head">
            <span className="intent">use MCP server</span>
            <code className="path">{request.alias}</code>
            <span className="counts">{local ? 'runs on this computer' : 'remote'}</span>
          </div>

          <p className="permission-scope">
            <strong>Requested by:</strong> <code>{request.requestedBy}</code>
          </p>

          {/* What the declaration runs or reaches, word by word as it was declared. A word is
              drawn as itself, so two that differ only in a space are told apart. */}
          {local ? (
            <p className="permission-scope">
              <strong>Runs:</strong>{' '}
              <code className="mcp-command">{drawCommand(request.command)}</code>
            </p>
          ) : (
            <p className="permission-scope">
              <strong>Reaches:</strong> <code className="mcp-url">{request.url}</code>
            </p>
          )}
          {request.program && (
            <p className="permission-scope">
              <strong>Program:</strong> <code>{request.program}</code>
            </p>
          )}
          {request.variables.length > 0 && (
            <p className="permission-scope">
              <strong>Receives:</strong>{' '}
              {request.variables.map((variable, index) => (
                <span key={variable.name}>
                  {index > 0 && ', '}
                  <code>{variable.name}</code>
                  {variable.stored && ' (stored)'}
                </span>
              ))}
            </p>
          )}
          {request.reads.length > 0 && (
            <div className="permission-scope">
              <strong>May read:</strong>
              <ul className="mcp-list">
                {request.reads.map((path) => (
                  <li key={path}>
                    <code>{path}</code>
                  </li>
                ))}
              </ul>
            </div>
          )}
          {request.directory && (
            <p className="permission-scope">
              <strong>Runs in, and may write:</strong> <code>{request.directory}</code>
            </p>
          )}
          <p className="permission-scope mcp-digest">
            <strong>Declaration:</strong> <code>{request.digest}</code>
          </p>

          {request.changed && (
            <p className="warn">
              This server’s declaration changed since you approved it. What it runs or reaches
              may not be what you said yes to.
            </p>
          )}
          {request.fetching.length > 0 && (
            <div className="warn">
              <ul className="mcp-list">
                {request.fetching.map((line, index) => (
                  <li key={index}>{line}</li>
                ))}
              </ul>
              Approving a server that fetches what it runs approves whatever it fetches each time
              it starts.
            </div>
          )}

          <p className="permission-scope">
            {local
              ? 'It runs confined: it reads and writes its own directory, receives only the variables named above, and starts with none of this app’s environment. '
              : 'Every request to it goes through the same network checks as any other. '}
            What it returns is kept from the model until you approve its list of tools, and each
            call to one of its tools is put to you.
          </p>

          <ThreeAnswers
            kind="mcp-server"
            request={request.request}
            answerable={answerable}
            decision={decision}
            remember={remember}
            onDecide={onDecide}
            reject="Continue without it"
            approve="Use this server"
            always="Use it and all future servers in this project"
            alwaysTooltip="Servers this project requests from now on start without asking, until you run bravebot mcp forget here"
            approved="You approved this server"
            approvedAlways="You approved this server and every server this project requests"
            rejected="You left this server out of the conversation"
          />
        </div>
      )
    }

    case 'mcp-tools': {
      const { request, decision } = entry
      const count = request.tools.length
      return (
        <div className={`confirm mcp-tools ${request.vetting.verdict === 'unsafe' ? 'unsafe' : ''}`}>
          <div className="confirm-head">
            <span className="intent">offer tools</span>
            <code className="path">{request.alias}</code>
            <span className="counts">
              {count} tool{count === 1 ? '' : 's'}
            </span>
            <VettingVerdict vetting={request.vetting} />
          </div>

          {request.changed && (
            <p className="warn">This is not the list you approved before: the tools it offers have changed.</p>
          )}

          <p className="permission-scope">
            The model will read each tool’s name, its arguments and what the server says about
            it, exactly as shown here. The descriptions are the server’s own text. Say no if one
            gives instructions.
          </p>

          {/* Each description is the server's text, drawn as text in a marked block, so it never
              reads as this app's own words (SERVERS-8). */}
          <ul className="mcp-tools-list">
            {request.tools.map((tool) => (
              <li key={tool.name}>
                <code className="mcp-tool-name">{tool.name}</code>
                {tool.arguments.length > 0 && (
                  <ul className="mcp-tool-arguments">
                    {tool.arguments.map((argument, index) => (
                      <li key={index}>
                        <code>{argument}</code>
                      </li>
                    ))}
                  </ul>
                )}
                {tool.description && (
                  <blockquote className="mcp-tool-description" data-tooltip="Written by the server">
                    {tool.description}
                  </blockquote>
                )}
              </li>
            ))}
          </ul>
          {request.refused > 0 && (
            <p className="permission-scope">
              {request.refused} more tool{request.refused === 1 ? ' is' : 's are'} not listed:
              {request.refused === 1 ? ' its' : ' their'} name or arguments cannot be offered.
            </p>
          )}

          <p className="permission-scope">
            Every call to one of these tools is still put to you. Your answer is remembered for
            this exact list, so the same list is not asked about again.
          </p>

          <YesOrNo
            kind="mcp-tools"
            request={request.request}
            answerable={answerable}
            decision={decision}
            onDecide={onDecide}
            reject="Continue without them"
            approve="Offer these tools"
            approved="You offered these tools to the model"
            rejected="You kept these tools from the model"
          />
        </div>
      )
    }

    case 'mcp-call': {
      const { request, decision, remember } = entry
      return (
        <div className="confirm mcp-call">
          <div className="confirm-head">
            <span className="intent">call tool</span>
            <code className="path">{request.name}</code>
            <span className="counts">MCP</span>
          </div>

          {request.description && (
            <blockquote className="mcp-tool-description" data-tooltip="Written by the server">
              {request.description}
            </blockquote>
          )}

          {/* What the model wrote, argument by argument, as JSON. The call sends exactly this. */}
          {request.arguments.length > 0 ? (
            <dl className="mcp-arguments">
              {request.arguments.map((argument) => (
                <div key={argument.name}>
                  <dt>
                    <code>{argument.name}</code>
                  </dt>
                  <dd>
                    <pre className="mcp-argument">{argument.value}</pre>
                  </dd>
                </div>
              ))}
            </dl>
          ) : (
            <p className="permission-scope">No arguments.</p>
          )}

          <p className="permission-scope">
            The call sends these arguments to the server. What it returns is confined: it is not
            put in the model’s context.
            {!request.mayStand && ' Nothing answered in this conversation can be recorded, so it cannot stop asking.'}
          </p>

          <ThreeAnswers
            kind="mcp-call"
            request={request.request}
            answerable={answerable}
            decision={decision}
            remember={remember}
            onDecide={onDecide}
            reject="Don’t call"
            approve="Call once"
            always={request.mayStand ? `Call, and stop asking for ${request.tool} here` : null}
            alwaysTooltip="Later calls to this tool in this project are made without asking, until you run bravebot mcp forget here"
            approved="You allowed this call"
            approvedAlways="You allowed this call and later ones to this tool in this project"
            rejected="You refused this call"
          />
        </div>
      )
    }

    case 'mcp-move': {
      const { request, decision } = entry
      return (
        <div className="confirm mcp-move">
          <div className="confirm-head">
            <span className="intent">move server</span>
            <code className="path">{request.alias}</code>
          </div>

          <p className="permission-scope">
            <strong>Declared at:</strong> <code>{request.declared}</code>
          </p>
          {/* Where the reply pointed is the server's own bytes, released for this screen, so it is
              drawn in a container of its own and decides nothing until the answer. */}
          <p className="permission-scope">
            <strong>Its reply points to:</strong> <code className="mcp-destination">{request.destination}</code>
          </p>
          {/* The host and port, taken out of the address by the agent, as a fetch's host is. */}
          <p className="permission-scope fetch-host">
            <strong>Reaching:</strong> <code>{request.authority}</code>
          </p>

          <p className="warn">
            Nothing was sent there. A yes declares the server at that address and sends it what
            was being sent, and every later request to the server goes there too
            {request.mayRecord ? ', in this conversation and the next' : ' until this conversation ends'}.
            Say no unless you know the server moved.
          </p>

          <YesOrNo
            kind="mcp-move"
            request={request.request}
            answerable={answerable}
            decision={decision}
            onDecide={onDecide}
            reject="Don’t move it"
            approve="It moved there"
            approved="You moved this server"
            rejected="You did not move this server"
          />
        </div>
      )
    }

    case 'mcp-started': {
      const { servers, notes } = entry.started
      return (
        <div className="mcp-started">
          <div className="mcp-started-head">
            <Icon name="plug" />
            <strong>
              {servers.length
                ? `MCP server${servers.length === 1 ? '' : 's'} started: ${servers.join(', ')}`
                : 'No MCP server started'}
            </strong>
          </div>
          {notes.length > 0 && (
            <ul className="mcp-list">
              {notes.map((note, index) => (
                <li key={index}>{note}</li>
              ))}
            </ul>
          )}
        </div>
      )
    }

    case 'plan-task':
      return (
        <div className="bubble user plan-task">
          <span className="plan-task-mark">Plan</span>
          {entry.text}
        </div>
      )

    case 'plan-reply':
      // Plain text in a marked container, and never formatted. What a run releases can be the
      // text of a file nobody vouched for, so it is not drawn as the agent's own words.
      return (
        <div className="plan-reply">
          <div className="plan-reply-head">
            <span className="mark">run result</span>
            <span>what the plan’s last step released</span>
          </div>
          <pre className="preview">{entry.text}</pre>
          <div className="plan-reply-foot">
            Shown to you and to no model. It is not part of this conversation.
            {entry.record && <> Saved as run <code>{entry.record}</code>.</>}
          </div>
        </div>
      )

    case 'plan-ended':
      return <PlanEnded ended={entry.ended} />

    case 'ask':
      return <Questions request={entry} answers={entry.answers} onAnswer={onAnswer} />

    case 'vouch': {
      const { request, decision } = entry
      return (
        <DecisionCard
          className="confirm vouch"
          answerable={answerable}
          subject={request.path}
          decided={decision === null ? null : { tone: decision, text: decision === 'approve' ? 'You vouched for this path' : 'You left it confined' }}
          head={<CardHead icon="shield-done" intent="Vouch for"
            subject={<code className="path" data-tooltip={request.path}>{request.path}</code>}
            trust={<VettingVerdict vetting={request.vetting} />} />}
          actions={<>
            <Button
              kind="plain-faint" size="small" className="reject"
              onClick={() => onDecide('vouch', request.request, false)}
            >
              Leave it confined
            </Button>
            <Button
              kind="filled" size="small" className="approve"
              onClick={() => onDecide('vouch', request.request, true)}
            >
              Vouch for this path
            </Button>
          </>}
        >
          <VettingReason vetting={request.vetting} />
          <p className="warn">
            <Icon name="warning-triangle-filled" />
            <span>Vouching records a standing rule for this path, so it applies to later reads as
            well as this one. Only do it for content you know the origin of.</span>
          </p>

          <pre className="preview">{request.preview}</pre>
          {request.truncated && (
            <div className="quarantine-foot">
              This is the beginning of the file, not all of it.
            </div>
          )}
          <Collapse className="card-details" title="Details" isOpen={undefined}>
            <VettingNotice vetting={request.vetting} />
          </Collapse>
        </DecisionCard>
      )
    }
  }
}

const INTENT_WORD: Record<import('../../shared/protocol').Intent, string> = { create: 'Create', overwrite: 'Overwrite', edit: 'Edit' }

/** How much of a tool's target a line shows before it cuts from the middle. */
const TARGET_WIDTH = 72
const WHY_WIDTH = 80

/**
 * The head every decision card shares: what kind of question it is, what it is about, how big it
 * is, and how far to trust it. The kind is an icon and a verb so the five read apart at a glance.
 */
function CardHead({ icon, intent, subject, counts, trust }: {
  icon: IconName
  intent: string
  subject?: React.ReactNode
  counts?: React.ReactNode
  trust?: React.ReactNode
}): React.JSX.Element {
  return <div className="confirm-head">
    <Icon name={icon} className="card-kind" />
    <span className="intent">{intent}</span>
    {subject}
    {counts}
    {trust}
  </div>
}

/**
 * The frame of a decision card.
 *
 * Undecided, it shows the whole question and its answers. Decided, it folds to the one line that
 * says what became of it, and can be opened again to see what was agreed to. A question whose
 * turn ended keeps the whole card with `Unanswered` in the answers' place, because a fold would
 * put away evidence that was never settled.
 */
function DecisionCard({ className, head, decided, subject, answerable, actions, children }: {
  className: string
  head: React.ReactNode
  decided: { tone: 'approve' | 'reject'; text: string } | null
  subject?: string
  answerable: boolean
  actions: React.ReactNode
  children: React.ReactNode
}): React.JSX.Element {
  const [shown, setShown] = useState(false)
  const settled = answerable && decided !== null
  const line = settled && decided && (
    <div className={`decided ${decided.tone}`}>
      <Icon name={decided.tone === 'approve' ? 'check-normal' : 'close'} />
      <span className="decided-text">{decided.text}</span>
      {subject && !shown && <code className="decided-subject" data-tooltip={subject}>{middleTruncate(subject, 56)}</code>}
      <Button kind="plain-faint" size="tiny" className="decided-toggle" aria-expanded={shown} onClick={() => setShown((open) => !open)}>
        {shown ? 'Hide' : 'Show'}
      </Button>
    </div>
  )
  if (settled && !shown) return <div className={className} data-folded="true">{line}</div>
  return <div className={className}>
    {head}
    <div className="card-body">{children}</div>
    {!answerable ? <Unanswered /> : decided === null ? <div className="confirm-actions">{actions}</div> : line}
  </div>
}

/**
 * What the composer says while a question is outstanding.
 *
 * Named for the question rather than a generic "answer the prompt", because the five are
 * not interchangeable and somebody who has scrolled away needs to know what they are
 * being asked before they scroll back.
 */
function waitingOn(kind: t.Asking['kind']): string {
  switch (kind) {
    case 'confirm':
      return 'Answer the write'
    case 'run':
      return 'Answer the command'
    case 'output':
      return 'Answer the output'
    case 'vet':
      return 'Review the checked content'
    case 'fetch':
      return 'Answer the fetch'
    case 'server':
      return 'Answer the language server'
    case 'manifest':
      return 'Answer the plan'
    case 'exposure':
      return 'Answer the file with a credential'
    case 'mcp-server':
      return 'Answer the MCP server'
    case 'mcp-tools':
      return 'Answer the MCP tools'
    case 'mcp-call':
      return 'Answer the MCP call'
    case 'mcp-move':
      return 'Answer the moved MCP server'
    case 'vouch':
      return 'Answer the vouch'
    case 'ask':
      return 'Answer the questions'
  }
}

function landingHint(landing: string): string {
  return landing === 'quarantined'
    ? 'not in the planner’s context; only an isolated processor can be sent to read it'
    : 'read by nothing: only its name is known'
}

/** The checker's verdict, as the label in a card's head, where it is read before the content is. */
function VettingVerdict({ vetting }: { vetting?: import('../../shared/protocol').Vetting }): React.JSX.Element {
  const verdict = vetting?.verdict
  return verdict === 'safe'
    ? <Label mode="outline" color="green" className="vetting-verdict safe">No instructions detected</Label>
    : verdict === 'unsafe'
      ? <Label mode="loud" color="red" className="vetting-verdict unsafe"><Icon name="warning-triangle-filled" slot="icon-before" />Possible instructions detected</Label>
      : <Label mode="outline" color="yellow" className="vetting-verdict">Check inconclusive</Label>
}

/** The checker's one-sentence reason, beside the content it is about: it is the evidence for the verdict. */
function VettingReason({ vetting }: { vetting?: import('../../shared/protocol').Vetting }): React.JSX.Element | null {
  return vetting?.reason ? <p className="vetting-reason"><span className="vetting-by">Checker</span>{vetting.reason}</p> : null
}

/** How far to lean on the checker, under a card's Details. */
function VettingNotice({ vetting }: { vetting?: import('../../shared/protocol').Vetting }): React.JSX.Element {
  const verdict = vetting?.verdict
  return <div className={`vetting-notice ${verdict === 'safe' ? 'safe' : 'caution'}`}>
    {vetting?.detail && <p>{vetting.detail}</p>}
    <small>{!vetting ? 'No checker assessment was recorded. Review the content before deciding.' : verdict === 'safe' || verdict === 'unsafe' ? 'The checker received this content at the backend before this question. Its assessment can be wrong; you decide whether the planner may read it.' : 'The check did not complete. Content may already have reached the backend. You still decide whether the planner may read it.'}</small>
  </div>
}
