import { Watches } from './Watches'
import type { FileAttachment } from '../../shared/files'
import { Permissions } from './Permissions'
import { useLayoutEffect, useEffect, useMemo, useRef, useState } from 'react'
import type { AskAnswer, AskPrompt, Phase, Shown, TodoRow } from '../../shared/protocol'
import * as t from '../transcript'
import type { Side } from '../columns'
import type { Asked } from '../App'
import type { ExportFormat } from '../../shared/export'
import { Diff } from './Diff'
import { Fold } from './Fold'
import { ModelPicker } from './ModelPicker'
import { ForkIcon } from './ForkIcon'
import { contextMenu } from './Sessions'
import { Markdown } from './Markdown'
import { PopMenu, type PopItem } from './PopMenu'
import { BotAvatar, type Doing } from './BotAvatar'
import type { Bot } from '../../shared/bots'
import { conversationPreferences, setConversation, setExperience, useExperience } from '../experience'
import { ErrorCard } from './ErrorCard'
import { FilePreview } from './FilePreview'
import { TurnFooter, TurnNotices, type OpenAudit } from './TurnDetails'
import type { Turns, TurnDisclosure } from '../turn-details'

interface Live {
  model: string | null
  handle: string
  summary: { title: string; project: string; branch: string | null; directory: string }
  entries: t.Entry[]
  turns: Turns
  todos: TodoRow[]
  quarantine: Shown[]
  phase: Phase | null
  contextTokens?: number
  archived?: number
  tokens: number
  running: boolean
  askingTrust: string | null
  forkedFrom: { directory: string; id: string; title: string; prompt: number } | null
  focus: number | null
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
  live: Live | null
  /** The bot whose session this is, if one is. Its name is what the header says instead of a title. */
  bot: Bot | null
  /** What that bot is doing, for its face in the header. Derived in `App`, where the turn is known. */
  doing: Doing
  pending: t.Asking | null
  problem: string | null
  collapsed: Record<Side, boolean>
  onToggle: (side: Side) => void
  /** The composer's text, owned by `App` so the Send menu item can be grey when it is empty. */
  draft: string
  onDraft: (draft: string) => void
  onModel: (model: string) => void
  onSubmit: () => void
  onCancel: () => void
  onDecide: Answer
  onAnswer: AnswerQuestions
  /** Begin a session from what was said before a named prompt. */
  onFork: (id: string) => void
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
 * rename a button the moment it was pressed. The verb goes in `title`, which is for the
 * pointer.
 */
function ColumnToggle({
  side,
  collapsed,
  onToggle,
}: {
  side: Side
  collapsed: boolean
  onToggle: (side: Side) => void
}): React.JSX.Element {
  const what = side === 'left' ? 'the session list' : 'the context panel'
  return (
    <button
      className={`fold-toggle ${side}`}
      aria-expanded={!collapsed}
      aria-controls={side === 'left' ? 'sessions-column' : 'context-column'}
      aria-label={side === 'left' ? 'Session list' : 'Context panel'}
      title={`${collapsed ? 'Show' : 'Hide'} ${what}`}
      onClick={() => onToggle(side)}
    >
      {/* Pointing outward when folded — the way the column will come back — and inward
          when open. Decorative: the button is already named and its state announced. */}
      <span className={`fold-chevron ${collapsed ? '' : 'open'}`} aria-hidden="true">
        {side === 'left' ? '›' : '‹'}
      </span>
    </button>
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
  queued, queuePaused, onResumeQueued,
  onRemoveQueued,
  live,
  bot,
  doing,
  pending,
  problem,
  collapsed,
  onToggle,
  draft,
  onDraft,
  onModel,
  onSubmit,
  onCancel,
  onDecide,
  onAnswer,
  onFork,
  onOpenParent,
  onFocused,
  canExport,
  includeTools,
  onToggleTools,
  onExport,
}: Props): React.JSX.Element {
  const bottom = useRef<HTMLDivElement>(null)
  const marked = useRef<HTMLDivElement>(null)
  const scroller = useRef<HTMLDivElement>(null)
  const input = useRef<HTMLTextAreaElement>(null)
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
  const preferences = useExperience()
  const [focusedLayout, setFocusedLayout] = useState<Record<Side, boolean> | null>(null)
  const jump = (element: HTMLElement | null) => element?.scrollIntoView({ block: 'nearest',
    behavior: window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth' })
  const latest = () => { following.current = true; setUnseen(false); jump(bottom.current) }
  useEffect(() => { void window.bravebot.readRecents().then(setRecents).catch(() => {}) }, [live?.handle])
  useLayoutEffect(() => {
    if (!input.current) return
    input.current.style.height = 'auto'
    input.current.style.height = `${Math.min(210, Math.max(70, input.current.scrollHeight))}px`
  }, [draft])
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
    const key = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'f') {
        event.preventDefault(); setSearching(true)
      }
    }
    document.addEventListener('keydown', key)
    return () => document.removeEventListener('keydown', key)
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
    let seen = 0
    for (const entry of live.entries) {
      if (entry.kind !== 'user') continue
      if (seen === live.focus) return entry.id
      seen += 1
    }
    // A parent with fewer prompts than it had. The link still opened the right session, which
    // is the honest half of what it promised.
    return null
  }, [live])

  // Read inside the effect below rather than depended on, and that is the whole point: a
  // transcript follows the conversation, but one that has been *sent* somewhere must stay
  // where it was sent. As a dependency, letting go of the mark would count as a change and
  // drag the view to the bottom a second and a half after the link landed on the row.
  const focusRef = useRef<number | null>(null)
  focusRef.current = live?.focus ?? null

  const activitySnapshot = useRef<{ handle?: string; entries?: t.Entry[]; phase?: Phase | null }>({})
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
  const head = (
    <header className="transcript-head">
      <div className="drag" />
      <div className="head-row">
        <ColumnToggle side="left" collapsed={collapsed.left} onToggle={onToggle} />
        {/* Rendered even with nothing to name: it is what holds the two toggles at
            opposite ends of the header, and without it they collect in the corner. */}
        <div className="head-titles">
          {live && (
            <>
              {/* A bot's session is titled by whatever was asked first, like every session — but a
                  bot is not an occasion, it is somebody, and a header saying "Hello." over a
                  conversation with the Custodian names the wrong thing. So a bot's own name and
                  face stand where the title would, and the title becomes the second line beside
                  the checkout: still there, no longer pretending to say whose this is. */}
              <h1>
                {bot && <BotAvatar seed={bot.avatar} size={30} doing={doing} />}
                {bot ? bot.name : live.summary.title}
              </h1>
              {/* The same string in the tooltip, because this line ellipsises and the
                  half it drops is the end of the path — which is the half that says which
                  checkout of a project this is. */}
              <span
                className="where"
                title={`${live.summary.directory}${live.summary.branch ? ` · ${live.summary.branch}` : ''}`}
              >
                {bot && `${live.summary.title} · `}
                {live.summary.directory}
                {live.summary.branch && ` · ${live.summary.branch}`}
              </span>
            </>
          )}
        </div>
        <ColumnToggle side="right" collapsed={collapsed.right} onToggle={onToggle} />
      </div>
      {live && <div className="conversation-toolbar">
        <button onClick={() => setSearching((value) => !value)} aria-expanded={searching}>Find</button>
        <button onClick={() => setPermissions(true)}>Permissions</button>
        <button onClick={() => setWatches(true)}>Watches</button>
        <button onClick={() => {
          if (focusedLayout) {
            for (const side of ['left', 'right'] as const) if (collapsed[side] !== focusedLayout[side]) onToggle(side)
            setFocusedLayout(null)
          } else {
            setFocusedLayout({ ...collapsed })
            for (const side of ['left', 'right'] as const) if (!collapsed[side]) onToggle(side)
          }
        }}>{focusedLayout ? 'Exit focus' : 'Focus'}</button>
        <button onClick={() => setExperience('density', preferences.density === 'compact' ? 'comfortable' : 'compact')}>
          {preferences.density === 'compact' ? 'Comfortable view' : 'Compact view'}
        </button>
        <ExportMenu canExport={canExport} includeTools={includeTools} onToggleTools={onToggleTools} onExport={onExport} />
      </div>}
      {backendReady === false && <div className="backend-status" role="status"><strong>Backend setup needed</strong><span>You can browse conversations and prepare drafts.</span><div><button onClick={onSetup}>Setup help</button><button onClick={onCheckBackend}>Check again</button><button onClick={onDiagnostics}>Diagnostics</button></div></div>}
      {live && <div className="context-status" title="The model’s last request size, not accumulated token usage. New messages may change the next request.">
        {live.phase === 'compacting' ? 'Summarising context…' : live.contextTokens === undefined ? 'Context measurement unavailable' : live.contextTokens === 0 ? 'Context not yet measured' : `${live.contextTokens.toLocaleString()} context tokens at last request`}
        {!!live.archived && <span> · Earlier context summarised</span>}
      </div>}
      {problem && <ErrorCard detail={problem} />}
      {searching && <div className="conversation-search">
        <input autoFocus type="search" aria-label="Find in conversation" placeholder="Find in conversation…" value={query}
          onChange={(event) => { setQuery(event.target.value); setMatch(0) }}
          onKeyDown={(event) => { if (event.key === 'Escape') setSearching(false); if (event.key === 'Enter') setMatch((n) => n + (event.shiftKey ? -1 + matches.length : 1)) }} />
        <span role="status">{matches.length ? `${match % matches.length + 1} of ${matches.length}` : query ? 'No matches' : ''}</span>
        <button disabled={!matches.length} onClick={() => setMatch((n) => n + matches.length - 1)} aria-label="Previous match">↑</button>
        <button disabled={!matches.length} onClick={() => setMatch((n) => n + 1)} aria-label="Next match">↓</button>
        <button onClick={() => setSearching(false)} aria-label="Close search">×</button>
      </div>}
      {live?.forkedFrom && <ForkBanner from={live.forkedFrom} onOpen={onOpenParent} />}
    </header>
  )

  if (!live) {
    return (
      <main className="transcript empty-state">
        {head}
        <div className="empty-body">
          <div>
            <div className="welcome-mark">B</div>
            <h1>What would you like to build?</h1>
            <p>Work with an agent in your project. Track changes and review approval requests as you work.</p>
            <button className="primary" onClick={() => onNew()}>Open project</button>
            {!!recents.length && <div className="welcome-recents"><h2>Recent projects</h2>{recents.slice(0, 5).map((directory) =>
              <button key={directory} onClick={() => onNew(directory)}><strong>{directory.split('/').pop()}</strong><span>{directory}</span></button>)}</div>}
            <p className="welcome-hint">Choose a conversation to resume work, or create a bot with a purpose and persistent memory.</p>
          </div>
        </div>
      </main>
    )
  }

  return (
    <main className="transcript">
      {head}

      <div className="entries" ref={scroller} onScroll={(event) => {
        const element = event.currentTarget
        lastScroll.current = element.scrollTop
        if (scrollSave.current) clearTimeout(scrollSave.current)
        scrollSave.current = setTimeout(() => { if (storageKey) setConversation(storageKey, { scroll: lastScroll.current }) }, 180)
        following.current = element.scrollHeight - element.scrollTop - element.clientHeight < 80
        if (following.current) setUnseen(false)
      }}>
        {runs(live.entries).map((run) =>
          run.kind === 'run' ? (
            <ToolRun key={run.id} entries={run.entries} />
          ) : (
            // Wrapped only to catch the right-click. `display: contents` keeps the wrapper
            // out of the layout entirely, so the bubbles flow exactly as they did — the
            // alternative was an `onContextMenu` on each of the eleven shapes `Row` returns.
            <div
              key={run.entry.id}
              data-entry-id={run.entry.id}
              className={`entry-hit ${run.entry.id === focused || run.entry.id === matches[match % (matches.length || 1)] ? 'focused' : ''}`}
              ref={run.entry.id === focused ? marked : undefined}
              // A prompt is the one row that came from the person reading it, and the only one
              // a fork can be cut in front of, so it is a different kind of thing to
              // right-click. The menu it gets is still decided in the main process.
              onContextMenu={contextMenu(
                run.entry.kind === 'user' ? 'entry-user' : 'entry',
                run.entry.id,
              )}
            >
              {run.entry.kind === 'turn-start' ? <TurnNotices details={live.turns[run.entry.number]} onDisclosure={onTurnDisclosure} /> : <Row
                entry={run.entry}
                onRecover={() => { onDraft((draft.trim() ? `${draft}\n\n` : '') + 'Continue the previous task from the current project state. First check which actions already completed; do not repeat successful commands or writes. Resolve the last error before proceeding.'); input.current?.focus() }}
                onChooseModel={() => { (document.querySelector('.composer .model-trigger') as HTMLButtonElement | null)?.click() }}
                onDecide={onDecide}
                onAnswer={onAnswer}
                onFork={onFork}
                // Greyed rather than gone while a turn runs, the way the menu item is: a
                // control that disappears is one the reader has to go looking for again.
                forkable={!live.running}
              />}
              {(run.entry.kind === 'assistant' || (run.entry.kind === 'error' && run.entry.turn !== undefined)) &&
                <TurnFooter details={run.entry.turn === undefined ? undefined : live.turns[run.entry.turn]} onDisclosure={onTurnDisclosure} onAudit={onAudit} />}
            </div>
          ),
        )}

        {live.running && (
          <div className={`working${bot ? ' working-bot' : ''}`}>
            {/* For a bot, the bot itself, looking down at the page — the one place in the transcript
                its face carries something the header does not: it is *here*, at the point of
                attention, only while something is happening, and its posture is the indicator. It
                mounts already working, since the row exists only while a turn runs. A plain session
                has no face and keeps the spinner. */}
            {bot ? (
              <BotAvatar seed={bot.avatar} size={22} doing="working" />
            ) : (
              <span className="spinner" />
            )}
            {live.phase ? phaseWord(live.phase) : 'Working'}
            {live.tokens > 0 && <span className="count"> · {live.tokens} tokens written</span>}
            {Object.values(live.turns).filter((turn) => turn.status === 'running').slice(-1).map((turn) =>
              <button className="turn-audit-link" key={turn.turn} aria-controls="turn-audit-inspector" onClick={(event) => onAudit(turn.turn, event.currentTarget)}>Audit</button>)}
            <button className="cancel" onClick={onCancel}>
              Cancel
            </button>
          </div>
        )}
        <div ref={bottom} />
      </div>

      <div className="attention-bar" aria-live="polite">
        {pending ? <button className="pending-jump" onClick={() => {
          document.dispatchEvent(new CustomEvent('bravebot:reveal-entry', { detail: pending.id }))
          const element = scroller.current?.querySelector<HTMLElement>(`[data-entry-id="${pending.id}"]`)
          jump(element ?? bottom.current)
        }}>{pending.kind === 'ask' ? 'Your answer is needed' : 'Approval needed'} · {waitingOn(pending.kind)} — Review ↑</button> :
          live.running ? <span>{live.phase ? phaseWord(live.phase) : 'Working'} · You can draft your next message</span> :
          <span>{live.entries.at(-1)?.kind === 'error' ? 'Needs attention' : live.entries.length ? 'Ready for your next message' : 'Ready to begin'}</span>}
        {unseen && <button onClick={latest}>New activity ↓</button>}
      </div>
      <footer className="composer">
        {queued.length > 0 && <div className="queued-messages"><strong>{queuePaused ? 'Queue paused' : 'Queued after this turn'}</strong>{queuePaused && <button disabled={live.running || backendReady === false} onClick={onResumeQueued}>Resume queue</button>}{queued.map((text, index) => <div key={index}><span>{text}</span><button aria-label={`Remove queued message ${index + 1}`} onClick={() => onRemoveQueued(index)}>×</button></div>)}</div>}
        {attachments.length > 0 && <div className="attachment-chips"><p>These files will be sent as trusted context with your message.</p>{attachments.map((file) => <span key={file.id}><button onClick={() => setPreviewPath(file.path)}>{file.path}</button><button aria-label={`Remove attachment ${file.path}`} onClick={() => onRemoveAttachment(file.id)}>×</button></span>)}</div>}
        <textarea ref={input} rows={2} value={draft} aria-label="Message the agent" title="Unsent drafts are saved locally on this device. Clear the message to remove its saved draft."
          placeholder={pending ? 'Draft your next message while you review…' : 'Describe a task, ask a question, or paste code…'}
          onChange={(event) => onDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter' && !event.shiftKey && !event.metaKey && !event.ctrlKey && !event.altKey
              && !event.nativeEvent.isComposing && event.keyCode !== 229) {
              event.preventDefault()
              if (!event.repeat && !live.running && backendReady !== false && draft.trim()) { latest(); onSubmit() }
            }
          }} />
        <div className="composer-toolbar">
          <ModelPicker session={live.handle} scope={bot ? 'bot' : 'conversation'} key={live.handle} model={live.model} disabled={live.running} onChoose={onModel} />
          <button className="attach-files" onClick={onAttach} disabled={attachments.length >= 5} title="Choose project files to share as trusted context">Attach files</button>
          <span className="composer-hint">Enter to send · Shift+Enter for newline</span>
          {live.running && <button className="stop" onClick={onCancel}>Stop</button>}
          <button className="send" onClick={() => { latest(); live.running ? onQueue() : onSubmit() }} disabled={!draft.trim() || !!live.askingTrust || backendReady === false}>
            {live.running ? 'Queue message' : 'Send'}
          </button>
        </div>
      </footer>
      {watches && <Watches session={live.handle} onClose={() => setWatches(false)} />}
      {permissions && <Permissions session={live.handle} onClose={() => setPermissions(false)} />}
      {previewPath && <FilePreview session={live.handle} path={previewPath} onClose={() => setPreviewPath(null)} />}
    </main>
  )
}

/** The three files a conversation can become. Ordered plainest first. */
const FORMATS: readonly PopItem[] = [
  { id: 'txt', label: 'Plain Text', detail: '.txt' },
  { id: 'md', label: 'Markdown', detail: '.md' },
  { id: 'pdf', label: 'PDF', detail: '.pdf' },
]

/**
 * What the file will contain, asked above what it will be called.
 *
 * The question really belongs in the save sheet, next to the filename — but a native save
 * panel takes no controls of ours, and a second dialog in front of it would put a question
 * between somebody and the thing they asked for every time they exported. So it is a row in
 * this menu, ticked or not, and the File menu carries the same one: see the note beside
 * `session.export-tools` in `shared/commands.ts`.
 */
const TOOLS = 'tools'

/**
 * The control that writes the conversation to a file.
 *
 * The whole button opens the menu rather than a split control like `NewSession`'s. That one
 * splits because opening a folder picker is overwhelmingly the common case and the recents
 * list is the exception; here there is no format worth guessing at, and a button that
 * exported a `.txt` because somebody clicked slightly to the left would be worse than one
 * that always asks.
 *
 * Placed before Send rather than after it, so the accent-coloured primary action stays in
 * the corner it has always been in.
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
  const [open, setOpen] = useState(false)
  const trigger = useRef<HTMLButtonElement>(null)

  // Rebuilt with the tick rather than held in state: the setting lives in `App`, which is
  // also what the File menu's copy of this row is drawn from, and a second copy here could
  // disagree with the one in the menu bar.
  const items: readonly PopItem[] = [
    {
      id: TOOLS,
      label: 'Include Tool Calls',
      // No second line saying "on" or "off": the tick is the state, and a row that said it
      // twice would be the only one in the app that did.
      checked: includeTools,
      // Never greyed, unlike the formats below and like its File-menu twin: it is a setting
      // rather than an action, and it is worth being able to set it in a session with
      // nothing said in it yet.
    },
    ...FORMATS.map((format, index) => (index === 0 ? { ...format, separated: true } : format)),
  ]

  return (
    <div className="export-split">
      <button
        ref={trigger}
        className="export-open"
        aria-haspopup="menu"
        aria-expanded={open}
        disabled={!canExport}
        title={canExport ? 'Export this conversation' : 'Nothing has been said yet'}
        onClick={() => setOpen(!open)}
      >
        Export
        <span className="export-chevron" aria-hidden="true">
          ⌄
        </span>
      </button>
      {/* `PopMenu` already flips above its anchor when there is no room below, which is the
          whole reason a menu can hang off a control at the bottom of the window. */}
      <PopMenu
        open={open}
        anchor={trigger}
        items={items}
        label="Export the conversation as"
        onChoose={(id) => (id === TOOLS ? onToggleTools() : onExport(id as ExportFormat))}
        onClose={() => setOpen(false)}
      />
    </div>
  )
}

function phaseWord(phase: Phase): string {
  // The agent's own words, so the two interfaces say the same thing about the same wait.
  return phase === 'planning'
    ? 'Planning'
    : phase === 'thinking'
      ? 'Thinking'
      : phase === 'compacting'
        ? 'Compacting'
        : 'Reconnecting'
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

function ToolRun({ entries }: { entries: t.Entry[] }): React.JSX.Element {
  const [open, setOpen] = useState(true)
  useEffect(() => {
    const reveal = (event: Event) => { if (entries.some((entry) => entry.id === (event as CustomEvent<string>).detail)) setOpen(true) }
    document.addEventListener('bravebot:reveal-entry', reveal)
    return () => document.removeEventListener('bravebot:reveal-entry', reveal)
  }, [entries])

  return (
    <section className={`tool-run ${open ? 'open' : ''}`}>
      <button
        className="tool-run-head"
        aria-expanded={open}
        // The verb in the title, the name staying put — the rule `ColumnToggle` states above.
        title={open ? 'Hide these steps' : 'Show these steps'}
        onClick={() => setOpen(!open)}
      >
        <span className={`chevron ${open ? 'open' : ''}`} aria-hidden="true">
          ›
        </span>
        {entries.length} step{entries.length === 1 ? '' : 's'}
      </button>
      <Fold open={open}>
        {entries.map((entry) => (
          <div key={entry.id} data-entry-id={entry.id}><Row
            key={entry.id}
            entry={entry}
            onDecide={() => undefined}
            onAnswer={() => undefined}
            // A run holds tool lines and nothing else, so neither of these can be reached
            // from in here — and a folded row carries no right-click either.
            onFork={() => undefined}
            forkable={false}
          /></div>
        ))}
      </Fold>
    </section>
  )
}

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

  if (answers) {
    return (
      <div className="confirm ask">
        <div className="confirm-head">
          <span className="intent">asked</span>
          <span className="path">
            {prompts.length} question{prompts.length === 1 ? '' : 's'}
          </span>
        </div>
        {prompts.map((prompt, at) => (
          // Keyed by position, not by `prompt.key`: that key is canonical *content*, and a
          // series may legitimately contain the same question twice. The order never
          // changes — the agent emits one prompt per question, in order — so the index is
          // both stable and unique where the content is only stable.
          <div className="asked-answer" key={at}>
            <div className="question">{prompt.question}</div>
            <div className="given">{describe(prompt, answers[at])}</div>
          </div>
        ))}
      </div>
    )
  }

  return (
    <div className="confirm ask">
      <div className="confirm-head">
        <span className="intent">asked</span>
        <span className="path">
          {prompts.length} question{prompts.length === 1 ? '' : 's'}
        </span>
      </div>

      {prompts.map((prompt, at) => (
        <fieldset className="ask-question" key={at}>
          <legend>
            <span className="header">{prompt.header}</span>
            {prompt.multiple && <span className="any">pick any</span>}
          </legend>
          <div className="question">{prompt.question}</div>

          <ul className="choices">
            {prompt.rows.map((row) => (
              <li key={row.index}>
                <button
                  className={`choice ${(picked[at] ?? []).includes(row.index) ? 'picked' : ''}`}
                  aria-pressed={(picked[at] ?? []).includes(row.index)}
                  onClick={() => choose(at, row.index, prompt.multiple)}
                >
                  <span className="label">{row.label}</span>
                  {row.detail && <span className="detail">{row.detail}</span>}
                </button>
              </li>
            ))}
          </ul>

          <input
            className="typed"
            value={typed[at] ?? ''}
            placeholder={prompt.rows.length > 0 ? 'or say something else…' : 'your answer…'}
            onChange={(event) =>
              setTyped((old) => old.map((text, index) => (index === at ? event.target.value : text)))
            }
          />
        </fieldset>
      ))}

      <div className="confirm-actions">
        {/* Declining every question is a real answer and the turn continues, so it is a
            button here rather than something a person has to leave blank and guess at. */}
        <button className="reject" onClick={() => onAnswer(request.request.request, prompts.map(() => ({})))}>
          Decline
        </button>
        <button className="approve" onClick={() => onAnswer(request.request.request, collected())}>
          Answer
          {/* Leaving a question blank declines it, which is legitimate but should not be a
              surprise — with several questions on screen it is easy to answer two of three
              and not notice. Said on the button rather than after the fact. */}
          {blank > 0 && prompts.length > 1 && (
            <span className="aside"> · {blank} declined</span>
          )}
        </button>
      </div>
    </div>
  )
}

/**
 * The line at the top of a session that was cut out of another one.
 *
 * In the header, beside the notes about the branch and the build, rather than in the scroller.
 * Those are the other two things the window says *about* a session rather than in it, and a
 * transcript is long: an indicator that has to be scrolled back to is one nobody reads.
 *
 * A button and not a link: nothing in this window navigates, and an `<a href>` here would be
 * the one thing on screen that could. It is also not a `t.Entry`, which is what keeps it out of
 * `t.conversation` and therefore out of every export — a note this window wrote about a session
 * is not something the session said.
 */
function ForkBanner({
  from,
  onOpen,
}: {
  from: { title: string; prompt: number }
  onOpen: () => void
}): React.JSX.Element {
  return (
    <p className="fork-banner">
      <span className="fork-mark">
        <ForkIcon />
      </span>{' '}
      Forked from{' '}
      <button className="link" onClick={onOpen} title="Show the session this was forked from">
        {from.title}
      </button>
      , before prompt {from.prompt + 1}.
    </p>
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

function Row({
  entry,
  onRecover,
  onChooseModel,
  onDecide,
  onAnswer,
  onFork,
  forkable,
}: {
  entry: t.Entry
  onRecover?: () => void
  onChooseModel?: () => void
  onDecide: Answer
  onAnswer: AnswerQuestions
  onFork: (id: string) => void
  /** Whether a fork can be taken at all right now — false while a turn is running. */
  forkable: boolean
}): React.JSX.Element {
  if (entry.interrupted) return <div className="interrupted-request"><strong>Request cancelled when the turn ended</strong><details><summary>Request details</summary><pre>{t.searchableText(entry)}</pre></details></div>
  switch (entry.kind) {
    case 'turn-start': return <></>
    case 'user':
      return (
        <div className="bubble user">
          {entry.text}
          {/* Inside the bubble, and positioned out of it. The wrapper this row sits in is
              `display: contents` and has no box to hang anything off, and the bubble is the
              only thing here that knows where the row actually is on screen. */}
          <button
            className="fork-here"
            aria-label="Fork from here"
            title={
              forkable
                ? 'Start a session from what was said before this'
                : 'Wait for the turn to finish'
            }
            disabled={!forkable}
            onClick={() => onFork(entry.id)}
          >
            <ForkIcon />
          </button>
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
      return <div className="narration">{entry.text}</div>

    case 'attached':
      // A line rather than a bubble, and the path rather than the contents. This is the same
      // register the tool lines are in — something that happened on the way to the reply — which
      // is what it is: a file somebody named, read at the top of a turn.
      return (
        <div className="attached" title={entry.path}>
          Read {entry.path}
        </div>
      )

    case 'consolidation':
      // The same line register as the attachment above, and deliberately not a bubble. This is
      // something that happened on the way to a reply rather than something anybody said, and the
      // whole reason it has an entry of its own is that drawing it as a prompt would claim
      // otherwise. See `CONSOLIDATION_MARK`.
      return <div className="attached consolidation">Asked to bring its memory up to date</div>

    case 'watch':
      return <div className="watch-turn"><strong>{entry.text}</strong><span>Automatic turn · file contents still follow normal read permissions</span></div>
    case 'error':
      return <ErrorCard category={entry.category} attempts={entry.attempts} status={entry.status} detail={entry.text} onRetry={onRecover} onModel={onChooseModel} />

    case 'replayed-tool':
      // No outcome, because the record does not keep one. Drawn quietly for the same
      // reason: a call the agent could not even name reads as "Tool", and giving that
      // the prominence of a real line would be worse than the gap.
      return <div className="tool replayed">{entry.text}</div>

    case 'tool': {
      const { activity, landing } = entry
      const running = activity.note === null
      return (
        <div className={`tool ${activity.failed ? 'failed' : ''} ${running ? 'running' : ''}`}>
          <span className="verb">{activity.verb}</span>
          {activity.target && <span className="target">({activity.target})</span>}
          {running ? (
            <span className="ellipsis">…</span>
          ) : (
            <span className="note">{activity.note}</span>
          )}
          {landing && landing !== 'context' && (
            <span className="confined" title={landingHint(landing)}>
              {landing === 'quarantined' ? 'quarantined' : 'name only'}
            </span>
          )}
        </div>
      )
    }

    case 'quarantined': {
      const { shown } = entry
      return (
        <div className="quarantine">
          <div className="quarantine-head">
            <span className="mark">confined</span>
            {/* Ellipsised, and it is a path: what gets cut is the part that identifies it. */}
            <span className="origin" title={shown.origin}>
              {shown.origin}
            </span>
            <span className="label">{shown.label}</span>
          </div>
          <pre className="preview">{shown.preview.join('\n')}</pre>
          <div className="quarantine-foot">
            {shown.lines} line{shown.lines === 1 ? '' : 's'} total ·{' '}
            {shown.reach === 'no_model'
              ? 'in no model’s context: nothing can be sent to read this'
              : 'not in the planner’s context; a processor can be sent to read it'}
          </div>
        </div>
      )
    }

    case 'confirm': {
      const { request, decision } = entry
      return (
        <div className={`confirm ${request.untrusted ? 'untrusted' : ''}`}>
          <div className="confirm-head">
            <span className="intent">{request.intent}</span>
            <code className="path">{request.path}</code>
            <span className="counts">
              +{request.added} −{request.removed}
            </span>
          </div>

          {request.untrusted && (
            <p className="warn">
              This came from somewhere nobody vouched for. The agent never read it — an
              isolated processor wrote it. Read it as you would a stranger’s patch.
            </p>
          )}
          {!request.exact && (
            <p className="warn">
              The files were too dissimilar to diff exactly. This is an approximation of
              the change.
            </p>
          )}

<p className="permission-scope">{request.existing ? 'Update an existing project file.' : 'Create a new project file.'} This decision applies to the change shown below.</p>
          {request.remark && <div className="processor-remark"><strong>Processor’s remark · untrusted</strong>
            <pre>{request.remark.preview.join('\n')}</pre>
            <small>{request.remark.label}{request.remark.lines > request.remark.preview.length ? ` · ${request.remark.lines - request.remark.preview.length} more lines not shown` : ''}. Review the diff before approving.</small>
          </div>}
          <Diff changes={request.changes} />

          {decision === null ? (
            <div className="confirm-actions">
              <button className="reject" onClick={() => onDecide('confirm', request.request, false)}>
                Don’t write
              </button>
              <button className="approve" onClick={() => onDecide('confirm', request.request, true)}>
                {request.existing ? 'Apply this change' : 'Create this file'}
              </button>
            </div>
          ) : (
            <div className={`decided ${decision}`}>
              {decision === 'approve' ? 'You approved this write' : 'You refused this write'}
            </div>
          )}
        </div>
      )
    }

    case 'run': {
      const { request, decision, remember } = entry
      return (
        <div className={`confirm run ${request.releasesPrivate ? 'releases' : ''}`}>
          <div className="confirm-head">
            <span className="intent">run</span>
            <code className="path">{request.directory}</code>
          </div>

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
                  {stage.resolved ?? 'not found on PATH'}
                </span>
              </li>
            ))}
          </ol>

          <p className="permission-scope">Run this command in the project folder shown above. “Run once” approves only this execution.</p>
          {decision === null && <p className="permission-scope"><strong>Remembered approval:</strong> {request.vouches.map((v) => v.display).join('; ')}. Covers these exact commands and trusts their output for this conversation, including after reopening it. Revoke through Permissions.</p>}
          {request.releasesPrivate && (
            <p className="warn">
              This hands your own data to the program. Whatever it does with those bytes
              happens somewhere the agent stops governing them.
            </p>
          )}

          {decision === null ? (
            <div className="confirm-actions">
              <button className="reject" onClick={() => onDecide('run', request.request, false)}>
                Don’t run
              </button>
              <button className="approve" onClick={() => onDecide('run', request.request, true)}>
                Run once
              </button>
              {/* Separate from "Run once" rather than a checkbox beside it: remembering
                  answers every later question about these programs, so it should take its
                  own deliberate press. The title says exactly what it would cover. */}
              <button
                className="approve always"
                title={`Stop asking about: ${request.vouches.map((v) => v.display).join(', ')}`}
                onClick={() => onDecide('run', request.request, true, true)}
              >
                Trust command and output
              </button>
            </div>
          ) : (
            <div className={`decided ${decision}`}>
              {decision === 'reject'
                ? 'You refused this command'
                : remember
                  ? 'You ran this and vouched for the programs'
                  : 'You ran this once'}
            </div>
          )}
        </div>
      )
    }

    case 'output': {
      const { request, decision } = entry
      return (
        <div className="confirm output">
          <div className="confirm-head">
            <span className="intent">read output</span>
            <code className="path">{request.command}</code>
            <span className="counts">
              {request.lines} line{request.lines === 1 ? '' : 's'}
            </span>
          </div>

          <p className="warn">
            The planner has not seen this. Read it yourself before deciding: approving is
            what puts it into the model’s context, and anything in here that reads like an
            instruction will be read there as one.
          </p>

          {/* In full, never truncated. The answer to this question rests on the bytes, so
              a preview would be asking for an approval of what nobody saw. */}
          <VettingNotice vetting={request.vetting} />
          <pre className="preview">{request.output}</pre>

          {decision === null ? (
            <div className="confirm-actions">
              <button
                className="reject"
                onClick={() => onDecide('output', request.request, false)}
              >
                Keep it out
              </button>
              <button
                className="approve"
                onClick={() => onDecide('output', request.request, true)}
              >
                Let the planner read it
              </button>
            </div>
          ) : (
            <div className={`decided ${decision}`}>
              {decision === 'approve'
                ? 'You let the planner read this'
                : 'You kept this out of the planner’s context'}
            </div>
          )}
        </div>
      )
    }

    case 'vet': {
      const { request, decision } = entry
      return <div className="confirm vetted-read">
        <div className="confirm-head"><span className="intent">read once</span><code className="path">{request.origin}</code><span>{request.lines} lines</span></div>
        <p className="permission-scope">Expected contents: {request.expects}</p>
        <VettingNotice vetting={request.vetting} />
        <p className="warn">Approval lets the planner read only this content. It does not trust this file for future reads.</p>
        <pre className="preview">{request.content}</pre>
        {decision === null ? <div className="confirm-actions">
          <button className="reject" onClick={() => onDecide('vet', request.request, false)}>Keep it out</button>
          <button className="approve" onClick={() => onDecide('vet', request.request, true)}>Let the planner read once</button>
        </div> : <div className={`decided ${decision}`}>{decision === 'approve' ? 'You allowed this content once' : 'You kept this content out'}</div>}
      </div>
    }
    case 'ask':
      return <Questions request={entry} answers={entry.answers} onAnswer={onAnswer} />

    case 'vouch': {
      const { request, decision } = entry
      return (
        <div className="confirm vouch">
          <div className="confirm-head">
            <span className="intent">vouch</span>
            <code className="path">{request.path}</code>
          </div>

          <p className="warn">
            Vouching records a standing rule for this path, so it applies to later reads as
            well as this one. Only do it for content you know the origin of.
          </p>

          <VettingNotice vetting={request.vetting} />
          <pre className="preview">{request.preview}</pre>
          {request.truncated && (
            <div className="quarantine-foot">
              This is the beginning of the file, not all of it.
            </div>
          )}

          {decision === null ? (
            <div className="confirm-actions">
              <button
                className="reject"
                onClick={() => onDecide('vouch', request.request, false)}
              >
                Leave it confined
              </button>
              <button
                className="approve"
                onClick={() => onDecide('vouch', request.request, true)}
              >
                Vouch for this path
              </button>
            </div>
          ) : (
            <div className={`decided ${decision}`}>
              {decision === 'approve'
                ? 'You vouched for this path'
                : 'You left it confined'}
            </div>
          )}
        </div>
      )
    }
  }
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

function VettingNotice({ vetting }: { vetting?: import('../../shared/protocol').Vetting }): React.JSX.Element {
  const verdict = vetting?.verdict
  const label = verdict === 'safe' ? 'No instructions detected' : verdict === 'unsafe' ? 'Possible instructions detected' : 'Check inconclusive'
  return <div className={`vetting-notice ${verdict === 'safe' ? 'safe' : 'caution'}`}>
    <strong>{label}</strong>
    {vetting?.reason && <p>{vetting.reason}</p>}
    {vetting?.detail && <p>{vetting.detail}</p>}
    <small>{!vetting ? 'No checker assessment was recorded. Review the content before deciding.' : verdict === 'safe' || verdict === 'unsafe' ? 'The checker received this content at the backend before this question. Its assessment can be wrong; you decide whether the planner may read it.' : 'The check did not complete. Content may already have reached the backend. You still decide whether the planner may read it.'}</small>
  </div>
}
