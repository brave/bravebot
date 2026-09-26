import { Watches } from './Watches'
import type { FileAttachment } from '../../shared/files'
import { Permissions } from './Permissions'
import { useLayoutEffect, useEffect, useMemo, useRef, useState } from 'react'
import { ChevronDownIcon, ChevronRightIcon } from 'lucide-react'
import { isConfined, type AskAnswer, type AskPrompt, type Phase, type Shown, type TodoRow } from '../../shared/protocol'
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
import { projectLabel } from '../../shared/recents'
import { conversationPreferences, setConversation, setExperience, useExperience } from '../experience'
import { ErrorCard } from './ErrorCard'
import { FilePreview } from './FilePreview'
import { TurnFooter, TurnNotices, type OpenAudit } from './TurnDetails'
import type { Turns, TurnDisclosure } from '../turn-details'
import { cn } from '@/lib/utils'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Bubble, BubbleContent } from '@/components/ui/bubble'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import {
  Field,
  FieldLegend,
  FieldSet,
} from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import {
  InputGroup,
  InputGroupAddon,
  InputGroupText,
  InputGroupTextarea,
} from '@/components/ui/input-group'
import { Marker, MarkerContent, MarkerIcon } from '@/components/ui/marker'
import { Message, MessageContent } from '@/components/ui/message'
import { Spinner } from '@/components/ui/spinner'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'

interface Live {
  model: string | null
  handle: string
  summary: { title: string; project: string; branch: string | null; directory: string }
  entries: t.Entry[]
  turns: Turns
  todos: TodoRow[]
  quarantine: Shown[]
  phase: Phase | null
  checking: number | null
  contextTokens?: number
  archived?: number
  tokens: number
  running: boolean
  askingTrust: string | null
  forkedFrom: { directory: string; id: string; title: string; prompt: number } | null
  focus: number | null
  autoVetting?: boolean
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
    // Sits on top of the drag strip and must stay clickable rather than moving the window, the
    // same exemption the New button gets in the session list.
    <button
      className={cn(
        'fold-toggle relative z-40 grid size-5.5 flex-none place-items-center rounded-md border-0 bg-transparent p-0',
        'text-[15px] leading-none text-muted-foreground/70 [-webkit-app-region:no-drag]',
        'hover:bg-foreground/12 hover:text-foreground',
        'focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-primary',
        side,
      )}
      aria-expanded={!collapsed}
      aria-controls={side === 'left' ? 'sessions-column' : 'context-column'}
      aria-label={side === 'left' ? 'Session list' : 'Context panel'}
      title={`${collapsed ? 'Show' : 'Hide'} ${what}`}
      onClick={() => onToggle(side)}
    >
      {/* Pointing outward when folded — the way the column will come back — and inward
          when open. Decorative: the button is already named and its state announced.

          Same duration and easing as the panels' chevrons, so both kinds of fold in this window
          read as one idea. A separate class from `.chevron`, though: sharing one would tie the
          two together and a change meant for either would land on both. */}
      <span
        className={cn(
          'fold-chevron inline-block transition-transform duration-[180ms] ease-[cubic-bezier(0.32,0.72,0,1)] motion-reduce:transition-none',
          !collapsed && 'open rotate-180',
        )}
        aria-hidden="true"
      >
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
    // The left padding moves with the fold, on the same easing, so the toggle inside slides clear
    // of the traffic lights rather than passing under them somewhere in the middle. With the
    // session list folded away the lights are over this header and it has to clear them itself —
    // `.sessions-head`, which normally does, is not on screen.
    <header
      className={cn(
        'transcript-head relative border-b border-border px-5 pt-3.5 pb-2.5',
        'transition-[padding-left] duration-[180ms] ease-[cubic-bezier(0.32,0.72,0,1)] motion-reduce:transition-none',
        '[-webkit-app-region:drag] [.app.left-folded_&]:pl-[78px]',
      )}
    >
      {/* The strip belongs to the header rather than to the window: positioned against its own
          header, it is right at every frame of a fold and at every width of a drag. */}
      <div className="drag absolute top-0 right-0 left-0 h-9.5 [-webkit-app-region:drag]" />
      <div className="head-row flex items-center gap-2.5 [-webkit-app-region:no-drag]">
        <ColumnToggle side="left" collapsed={collapsed.left} onToggle={onToggle} />
        {/* Rendered even with nothing to name: it is what holds the two toggles at
            opposite ends of the header, and without it they collect in the corner. */}
        {/* `min-w-0` so the path below can ellipsise instead of shouldering the right-hand
            toggle off the edge of the window. */}
        <div className="head-titles min-w-0 flex-1">
          {live && (
            <>
              {/* A bot's session is titled by whatever was asked first, like every session — but a
                  bot is not an occasion, it is somebody, and a header saying "Hello." over a
                  conversation with the Custodian names the wrong thing. So a bot's own name and
                  face stand where the title would, and the title becomes the second line beside
                  the checkout: still there, no longer pretending to say whose this is. */}
              {/* Laid out as a row so a bot's face sits on the word rather than above it. */}
              <h1 className="m-0 flex items-center gap-[7px] text-sm font-semibold">
                {bot && <BotAvatar seed={bot.avatar} size={30} doing={doing} />}
                {bot ? bot.name : live.summary.title}
              </h1>
              {/* The same string in the tooltip, because this line ellipsises and the
                  half it drops is the end of the path — which is the half that says which
                  checkout of a project this is. */}
              <span
                className="where block truncate font-mono text-xs text-muted-foreground/70"
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
      {/* Every control here is bordered rather than filled: a strip with an accent-coloured
          button in it would have a primary action, and none of these is one. */}
      {live && <div className={cn(
        'conversation-toolbar flex flex-wrap items-center gap-1.5 px-5 pt-1.5 pb-2.5',
        '[&>button]:min-h-7.5 [&>button]:rounded-md [&>button]:border [&>button]:border-border',
        '[&>button]:bg-transparent [&>button]:px-2.5 [&>button]:py-1 [&>button]:text-xs',
      )}>
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
      {backendReady === false && <div className={cn(
        'backend-status flex flex-col gap-1.25 bg-warning/10 px-5 py-3 text-xs',
        '[&_button]:rounded-[5px] [&_button]:border [&_button]:border-border [&_button]:bg-background [&_button]:px-2 [&_button]:py-1.25',
      )} role="status"><strong>Backend setup needed</strong><span>You can browse conversations and prepare drafts.</span><div className="flex gap-1.5"><button onClick={onSetup}>Setup help</button><button onClick={onCheckBackend}>Check again</button><button onClick={onDiagnostics}>Diagnostics</button></div></div>}
      {live && <div className="context-status shrink-0 px-5 py-1.5 text-[11px] text-muted-foreground" title="The model’s last request size, not accumulated token usage. New messages may change the next request.">
        {live.phase === 'compacting' ? 'Summarising context…' : live.contextTokens === undefined ? 'Context measurement unavailable' : live.contextTokens === 0 ? 'Context not yet measured' : `${live.contextTokens.toLocaleString()} context tokens at last request`}
        {!!live.archived && <span> · Earlier context summarised</span>}
      </div>}
      {problem && <ErrorCard detail={problem} />}
      {searching && <div className={cn(
        'conversation-search flex items-center gap-1.5 border-t border-border px-5 py-2',
        '[&_button]:min-h-7.5 [&_button]:min-w-7 [&_button]:rounded-md [&_button]:border [&_button]:border-border [&_button]:bg-background',
      )}>
        <input className="min-w-0 flex-1 rounded-md border border-border bg-background p-2 text-foreground [appearance:none] focus:border-primary focus:outline-none [&::-webkit-search-cancel-button]:[appearance:none]"
          autoFocus type="search" aria-label="Find in conversation" placeholder="Find in conversation…" value={query}
          onChange={(event) => { setQuery(event.target.value); setMatch(0) }}
          onKeyDown={(event) => { if (event.key === 'Escape') setSearching(false); if (event.key === 'Enter') setMatch((n) => n + (event.shiftKey ? -1 + matches.length : 1)) }} />
        <span className="text-xs whitespace-nowrap text-muted-foreground" role="status">{matches.length ? `${match % matches.length + 1} of ${matches.length}` : query ? 'No matches' : ''}</span>
        <button disabled={!matches.length} onClick={() => setMatch((n) => n + matches.length - 1)} aria-label="Previous match">↑</button>
        <button disabled={!matches.length} onClick={() => setMatch((n) => n + 1)} aria-label="Next match">↓</button>
        <button onClick={() => setSearching(false)} aria-label="Close search">×</button>
      </div>}
      {live?.autoVetting && <VettingBanner />}
      {live?.forkedFrom && <ForkBanner from={live.forkedFrom} onOpen={onOpenParent} />}
    </header>
  )

  if (!live) {
    return (
      <main className="transcript empty-state flex flex-col overflow-hidden">
        {head}
        {/* The centring is on the body rather than the column, because the column also holds a
            header — the empty state still needs the fold toggles and the window's drag strip. */}
        <div className="empty-body grid flex-1 items-start justify-center justify-items-center overflow-y-auto text-left text-muted-foreground/70">
          <div className="w-[min(560px,100%)] p-6">
            <div className="welcome-mark mb-5.5 grid size-12.5 place-items-center rounded-[15px] bg-primary text-2xl font-bold text-primary-foreground">B</div>
            <h1 className="mt-0 mb-3.5 text-[28px] leading-[1.2] tracking-[-0.5px] text-foreground">What would you like to build?</h1>
            <p className="leading-[1.6]">Work with an agent in your project. Track changes and review approval requests as you work.</p>
            <button className="primary rounded-lg border-0 bg-primary px-4.5 py-2.5 font-semibold text-primary-foreground" onClick={() => onNew()}>Open project</button>
            {!!recents.length && <div className={cn(
              'welcome-recents mt-8 grid gap-1.5',
              '[&>button]:flex [&>button]:flex-col [&>button]:gap-0.75 [&>button]:rounded-lg [&>button]:border [&>button]:border-border',
              '[&>button]:bg-transparent [&>button]:px-3 [&>button]:py-2.5 [&>button]:text-left',
            )}><h2 className="text-[13px] text-muted-foreground">Recent projects</h2>{recents.slice(0, 5).map((directory) =>
              <button key={directory} onClick={() => onNew(directory)}><strong>{projectLabel(directory)}</strong><span className="text-xs text-muted-foreground [overflow-wrap:anywhere]">{directory}</span></button>)}</div>}
            <p className="welcome-hint mt-6 text-xs text-muted-foreground">Choose a conversation to resume work, or create a bot with a purpose and persistent memory.</p>
          </div>
        </div>
      </main>
    )
  }

  return (
    <main className="transcript flex flex-col overflow-hidden">
      {head}

      {/* The reading measure is held by the padding rather than by a wrapper: 840px of text with
          the rest given away, so a wide window leaves room for code and diffs without the prose
          running the width of a monitor. */}
      <div className={cn(
        'entries flex-1 overflow-y-auto py-6 px-[max(20px,calc((100%-840px)/2))]',
        '[.app.compact_&]:py-4',
      )} ref={scroller} onScroll={(event) => {
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
              // `contents` keeps the wrapper out of the layout entirely, so the bubbles flow
              // exactly as they did. It also means the wrapper cannot be painted, so where a
              // fork's link landed is marked on the row inside it — and the mark fades, because
              // it answers "which one did I ask for" and then stops being a question.
              className={cn(
                'entry-hit contents',
                (run.entry.id === focused || run.entry.id === matches[match % (matches.length || 1)])
                  && 'focused [&>*]:animate-[fork-landed_1.6s_ease-out] [&>*]:rounded-lg',
              )}
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
                // control that disappears is one the reader has to go looking for again. Greyed
                // too for a prompt this window has sent and not yet been told the ordinal of:
                // that prompt is in the conversation, but nothing here knows where.
                forkable={!live.running && (run.entry.kind !== 'user' || run.entry.prompt !== undefined)}
              />}
              {(run.entry.kind === 'assistant' || (run.entry.kind === 'error' && run.entry.turn !== undefined)) &&
                <TurnFooter details={run.entry.turn === undefined ? undefined : live.turns[run.entry.turn]} onDisclosure={onTurnDisclosure} onAudit={onAudit} />}
            </div>
          ),
        )}

        {live.running && (
          <div className={cn(
            'working flex flex-wrap items-center gap-2 px-0.5 py-2 text-xs text-muted-foreground',
            // A shade tighter than the spinner's, because the figure has air of its own inside
            // its disc. The row's height is Cancel's either way, so nothing else moves.
            bot && 'working-bot gap-[7px]',
          )}>
            {/* For a bot, the bot itself, looking down at the page — the one place in the transcript
                its face carries something the header does not: it is *here*, at the point of
                attention, only while something is happening, and its posture is the indicator. It
                mounts already working, since the row exists only while a turn runs. A plain session
                has no face and keeps the spinner. */}
            {bot ? (
              <BotAvatar seed={bot.avatar} size={22} doing="working" />
            ) : (
              <Spinner className="spinner size-2.25 border-2 border-primary border-r-transparent" />
            )}
            {workingWord(live.phase, live.checking)}
            {live.tokens > 0 && <span className="count text-muted-foreground/70"> · {live.tokens} tokens written</span>}
            {Object.values(live.turns).filter((turn) => turn.status === 'running').slice(-1).map((turn) =>
              <Button variant="link" size="sm" className="turn-audit-link h-auto p-0 py-1.25 text-xs text-muted-foreground hover:text-foreground hover:underline pointer-coarse:min-h-11" key={turn.turn} aria-controls="turn-audit-inspector" onClick={(event) => onAudit(turn.turn, event.currentTarget)}>Audit</Button>)}
            <Button variant="outline" size="sm" className="cancel ml-auto h-auto rounded-md px-2.5 py-0.75 text-[11px] font-normal" onClick={onCancel}>
              Cancel
            </Button>
          </div>
        )}
        <div ref={bottom} />
      </div>

      {/* The same measure the composer keeps, so the two strips under the transcript line up. */}
      <div className={cn(
        'attention-bar flex min-h-8.5 items-center justify-between gap-2 py-1.5 text-xs text-muted-foreground',
        'px-[max(20px,calc((100%-880px)/2))]',
      )} aria-live="polite">
        {pending ? <Button variant="secondary" size="sm" className="pending-jump rounded-[7px] border border-border bg-warning/10 px-2.5 py-1.5 text-left font-semibold text-warning hover:bg-warning/20" onClick={() => {
          document.dispatchEvent(new CustomEvent('bravebot:reveal-entry', { detail: pending.id }))
          const element = scroller.current?.querySelector<HTMLElement>(`[data-entry-id="${pending.id}"]`)
          jump(element ?? bottom.current)
        }}>{pending.kind === 'ask' ? 'Your answer is needed' : 'Approval needed'} · {waitingOn(pending.kind)} — Review ↑</Button> :
          live.running ? <span>{workingWord(live.phase, live.checking)} · You can draft your next message</span> :
          <span>{live.entries.at(-1)?.kind === 'error' ? 'Needs attention' : live.entries.length ? 'Ready for your next message' : 'Ready to begin'}</span>}
        {unseen && <Button variant="secondary" size="sm" className="rounded-[7px] border border-border bg-background px-2.5 py-1.5" onClick={latest}>New activity ↓</Button>}
      </div>
      <footer className={cn(
        'composer flex flex-col items-stretch gap-2.5 border-t border-border bg-background pt-3 pb-4',
        'px-[max(20px,calc((100%-880px)/2))]',
      )}>
        {queued.length > 0 && <div className="queued-messages flex flex-col gap-2 rounded-lg bg-bubble-agent px-3 py-2 text-xs"><strong>{queuePaused ? 'Queue paused' : 'Queued after this turn'}</strong>{queuePaused && <Button size="sm" disabled={live.running || backendReady === false} onClick={onResumeQueued}>Resume queue</Button>}{queued.map((text, index) => <div key={index} className="flex items-center gap-2"><span className="min-w-0 flex-1 truncate">{text}</span><Button variant="ghost" size="icon-xs" aria-label={`Remove queued message ${index + 1}`} onClick={() => onRemoveQueued(index)}>×</Button></div>)}</div>}
        {attachments.length > 0 && <div className="attachment-chips flex flex-col gap-2 text-xs"><p className="m-0 mb-1 w-full text-warning">These files will be sent as trusted context with your message.</p>{attachments.map((file) => <span key={file.id} className="inline-flex max-w-full items-center gap-1 rounded-md border border-border"><Button variant="link" size="sm" className="min-w-0 truncate" onClick={() => setPreviewPath(file.path)}>{file.path}</Button><Button variant="ghost" size="icon-xs" aria-label={`Remove attachment ${file.path}`} onClick={() => onRemoveAttachment(file.id)}>×</Button></span>)}</div>}
        <InputGroup className="h-auto items-stretch">
          <InputGroupTextarea
            ref={input}
            className="max-h-[210px] min-h-[70px] w-full flex-none resize-none p-3.5 leading-[1.5]"
            rows={2}
            value={draft}
            aria-label="Message the agent"
            title="Unsent drafts are saved locally on this device. Clear the message to remove its saved draft."
            placeholder={pending ? 'Draft your next message while you review…' : 'Describe a task, ask a question, or paste code…'}
            onChange={(event) => onDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter' && !event.shiftKey && !event.metaKey && !event.ctrlKey && !event.altKey
                && !event.nativeEvent.isComposing && event.keyCode !== 229) {
                event.preventDefault()
                if (!event.repeat && !live.running && backendReady !== false && draft.trim()) { latest(); onSubmit() }
              }
            }}
          />
          <InputGroupAddon align="block-end" className="composer-toolbar flex flex-wrap items-center gap-2 [&_.model-picker]:max-w-55 [&_.model-picker]:self-center">
            <ModelPicker session={live.handle} scope={bot ? 'bot' : 'conversation'} key={live.handle} model={live.model} disabled={live.running} onChoose={onModel} />
            <Button variant="outline" size="sm" className="attach-files h-auto rounded-[7px] border-border bg-background px-2.25 py-1.75 text-xs font-normal whitespace-nowrap" onClick={onAttach} disabled={attachments.length >= 5} title="Choose project files to share as trusted context">Attach files</Button>
            {/* The first thing a narrow window gives up: it is a reminder, and the two controls
                beside it are not. */}
            <InputGroupText className="composer-hint min-w-[130px] flex-1 text-[11px] text-muted-foreground max-[1120px]:hidden">Enter to send · Shift+Enter for newline</InputGroupText>
            {live.running && <Button variant="outline" size="sm" className="stop min-h-9 rounded-lg border-destructive bg-background px-3 py-1.5 text-destructive" onClick={onCancel}>Stop</Button>}
            <Button
              className="send ml-auto h-auto min-h-9 rounded-[10px] px-4.5 font-medium whitespace-nowrap"
              onClick={() => { latest(); live.running ? onQueue() : onSubmit() }}
              disabled={!draft.trim() || !!live.askingTrust || backendReady === false}
            >
              {live.running ? 'Queue message' : 'Send'}
            </Button>
          </InputGroupAddon>
        </InputGroup>
      </footer>
      {watches && <Watches session={live.handle} onClose={() => setWatches(false)} />}
      {permissions && <Permissions session={live.handle} onClose={() => {
        setPermissions(false)
        queueMicrotask(() => {
          for (const el of document.querySelectorAll('button')) {
            if (el.textContent === 'Permissions') { (el as HTMLButtonElement).focus(); break }
          }
        })
      }} />}
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
    <div className="export-split ml-auto self-end">
      {/* `PopMenu` already flips above its anchor when there is no room below, which is the
          whole reason a menu can hang off a control at the bottom of the window. */}
      <PopMenu
        open={open}
        onOpenChange={setOpen}
        items={items}
        label="Export the conversation as"
        onChoose={(id) => (id === TOOLS ? onToggleTools() : onExport(id as ExportFormat))}
        trigger={
          <Button
            variant="outline"
            size="sm"
            className={cn(
              'export-open inline-flex h-auto min-h-7.5 items-center gap-1.25 rounded-md border-border',
              'bg-transparent px-2.5 py-1 text-xs font-normal whitespace-nowrap text-muted-foreground',
              'hover:bg-code hover:text-foreground',
            )}
            disabled={!canExport}
            title={canExport ? 'Export this conversation' : 'Nothing has been said yet'}
          >
            Export
            {/* Centring the boxes is not enough, and the target is not what it looks like either.
                `⌄` sits on the baseline, so a centred box still reads low; and "Export" carries a
                descender, which drags the word's box centre below the middle of the letters
                somebody actually looks at. So the mark is aligned to the label's cap centre. In
                `em`, so it holds if the size changes. */}
            <ChevronDownIcon data-icon="inline-end" className="export-chevron translate-y-[-0.11em] text-[10px] leading-none" aria-hidden="true" />
          </Button>
        }
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
 * What the session is waiting on. A running check wins over the phase, which a check does not
 * change, and one function serves both places the word is drawn so they cannot disagree.
 */
export function workingWord(phase: Phase | null, checking: number | null): string {
  if (checking !== null) return `Checking ${checking} ${checking === 1 ? 'line' : 'lines'}`
  return phase ? phaseWord(phase) : 'Working'
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
    <Collapsible open={open} onOpenChange={setOpen} className={cn('tool-run my-0.5', open && 'open')}>
      <Card size="sm" className="gap-0 bg-transparent py-0 ring-0">
        {/* The quietest thing in the transcript that is still a control: this is apparatus between
            a question and its answer, and it should not compete with either. */}
        <CollapsibleTrigger
          className={cn(
            'tool-run-head flex items-center gap-1.5 rounded-none border-0 border-l-2 border-border',
            'bg-transparent px-2 py-0.75 text-[11px] text-muted-foreground/70 hover:text-muted-foreground',
            'focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-primary',
          )}
          // The verb in the title, the name staying put — the rule `ColumnToggle` states above.
          title={open ? 'Hide these steps' : 'Show these steps'}
        >
          <ChevronRightIcon
            className={cn(
              'chevron inline-block size-3! transition-transform duration-[180ms] ease-[cubic-bezier(0.32,0.72,0,1)] motion-reduce:transition-none',
              open && 'open rotate-90',
            )}
            aria-hidden="true"
          />
          {entries.length} step{entries.length === 1 ? '' : 's'}
        </CollapsibleTrigger>
        <CardContent className="p-0">
          {/* Fold keeps the `.fold` drive-script hook and height animation. */}
          <Fold open={open}>
            {entries.map((entry) => (
              <div key={entry.id} data-entry-id={entry.id}><EntryCard
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
        </CardContent>
      </Card>
    </Collapsible>
  )
}

/* ---- the approval cards ------------------------------------------------------------
   Seven of them, sharing one shape: a head naming the act, the thing being approved, and a row
   of answers at the foot. Held here as strings rather than written out at each of the seven,
   because the whole point of them looking alike is that a reviewer learns the shape once.
   An approval a reviewer cannot read is decorative. */

const CARD = 'confirm my-3 overflow-hidden rounded-[10px] border border-border'

const HEAD = 'confirm-head flex items-baseline gap-2 border-b border-border bg-bubble-agent px-3 py-2'
const INTENT = 'intent text-[10px] font-bold uppercase text-primary'
const PATH = 'path flex-1 font-mono text-xs'
const COUNTS = 'counts font-mono text-[11px] text-muted-foreground/70'

/** Something the reader is being told to look twice at, above the thing itself. */
const WARN = 'warn m-0 border-b border-border bg-warning/10 px-3 py-1.75 text-[11px] text-warning'

/** What the answer would actually cover, which is not always what the head says. */
const SCOPE = 'permission-scope m-0 border-t border-border px-3 py-2.5 text-xs text-muted-foreground [overflow-wrap:anywhere]'

/* Sticky at the foot, so the answers stay in reach however long the diff above them is. */
const ACTIONS = 'confirm-actions sticky bottom-0 z-1 flex flex-wrap justify-end gap-2 border-t border-border bg-background px-3 py-2.5'

/** In full, never truncated: the answer rests on the bytes, so a preview would be asking for an
 *  approval of what nobody saw. */
const PREVIEW = 'preview m-0 max-h-45 overflow-auto bg-background px-2.5 py-2 font-mono text-[11px] whitespace-pre-wrap'

const DECIDED = 'decided border-t border-border px-3 py-2 text-[11px]'

/** The shape of the asides a card can carry about its own contents: what a checker made of them,
 *  what an isolated processor said about them, what this window thinks it recognised in them. Each
 *  site adds its own class hook; this is only the box they share. */
const NOTICE = 'm-3 rounded-lg border border-border px-4 py-3'

const decidedTone = (decision: string): string =>
  decision === 'approve' ? 'text-success' : decision === 'reject' ? 'text-destructive' : 'text-muted-foreground'

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
      <div className={cn(CARD, 'ask flex flex-col gap-0')}>
        <div className={HEAD}>
          <span className={INTENT}>asked</span>
          <span className={cn(PATH, 'font-sans text-muted-foreground')}>
            {prompts.length} question{prompts.length === 1 ? '' : 's'}
          </span>
        </div>
        {prompts.map((prompt, at) => (
          // Keyed by position, not by `prompt.key`: that key is canonical *content*, and a
          // series may legitimately contain the same question twice. The order never
          // changes — the agent emits one prompt per question, in order — so the index is
          // both stable and unique where the content is only stable.
          <div className="asked-answer flex flex-col gap-1 border-b border-border px-3 py-2" key={at}>
            <div className="question text-xs text-muted-foreground">{prompt.question}</div>
            {answers && <div className="given mt-0.5 text-xs before:text-muted-foreground/70 before:content-['→_']">{describe(prompt, answers[at])}</div>}
          </div>
        ))}
        {!answers && <Unanswered />}
      </div>
    )
  }

  return (
    // Drawn in the accent rather than the confine purple: unlike the output and vouch cards,
    // nothing here is untrusted — the planner may only ask at all when it has not been shown
    // untrusted content, so these are the model's own words.
    <div className={cn(CARD, 'ask flex flex-col gap-0')}>
      <div className={HEAD}>
        <span className={INTENT}>asked</span>
        <span className={cn(PATH, 'font-sans text-muted-foreground')}>
          {prompts.length} question{prompts.length === 1 ? '' : 's'}
        </span>
      </div>

      {prompts.map((prompt, at) => (
        <FieldSet className="ask-question m-0 border-0 border-b border-border px-3 py-2.5" key={at}>
          <FieldLegend className="flex items-baseline gap-1.5 p-0">
            <span className="header text-[10px] font-bold tracking-[0.04em] text-muted-foreground/70 uppercase">{prompt.header}</span>
            {/* Said once, next to the question it applies to. Somebody picking a second option
                should not have to discover from the interface's behaviour that they were
                allowed to. */}
            {prompt.multiple && <span className="any rounded-[4px] bg-foreground/16 px-1.25 text-[10px] text-muted-foreground">pick any</span>}
          </FieldLegend>
          <div className="question mt-1 mb-2">{prompt.question}</div>

          <Field>
            <ToggleGroup
              className="choices mb-2 grid w-full gap-1.25"
              orientation="vertical"
              spacing={1}
              multiple={prompt.multiple}
              value={(picked[at] ?? []).map(String)}
              onValueChange={(next) => {
                setPicked((old) =>
                  old.map((chosen, index) =>
                    index === at
                      ? next.map(Number).sort((a, b) => a - b)
                      : chosen,
                  ),
                )
              }}
            >
              {prompt.rows.map((row) => (
                <ToggleGroupItem
                  key={row.index}
                  value={String(row.index)}
                  className={cn(
                    'choice h-auto w-full flex-col items-start gap-0.5 rounded-[7px] border border-border',
                    'bg-transparent px-2.25 py-1.5 text-left font-normal whitespace-normal',
                    'hover:border-muted-foreground/70',
                    (picked[at] ?? []).includes(row.index) && 'picked border-primary bg-warning/10',
                  )}
                >
                  <span className="label block text-xs">{row.label}</span>
                  {row.detail && <span className="detail block text-[11px] text-muted-foreground/70">{row.detail}</span>}
                </ToggleGroupItem>
              ))}
            </ToggleGroup>
          </Field>

          <Input
            className="typed w-full rounded-[7px] px-2 py-1.25 text-xs"
            value={typed[at] ?? ''}
            placeholder={prompt.rows.length > 0 ? 'or say something else…' : 'your answer…'}
            onChange={(event) =>
              setTyped((old) => old.map((text, index) => (index === at ? event.target.value : text)))
            }
          />
        </FieldSet>
      ))}

      <div className={ACTIONS}>
        {/* Declining every question is a real answer and the turn continues, so it is a
            button here rather than something a person has to leave blank and guess at. */}
        <Button variant="outline" className="reject hover:bg-destructive/10 hover:text-destructive" onClick={() => onAnswer(request.request.request, prompts.map(() => ({})))}>
          Decline
        </Button>
        <Button className="approve font-medium" onClick={() => onAnswer(request.request.request, collected())}>
          Answer
          {/* Leaving a question blank declines it, which is legitimate but should not be a
              surprise — with several questions on screen it is easy to answer two of three
              and not notice. Said on the button rather than after the fact. */}
          {blank > 0 && prompts.length > 1 && (
            <span className="aside text-[11px] opacity-75"> · {blank} declined</span>
          )}
        </Button>
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
    // Laid out like the notes next door, because it is the same kind of thing: something the
    // window says about the session rather than something the session said. Quieter, though — a
    // note is for something that went wrong, and this only says where we are.
    <Marker className="fork-banner mt-1.5 rounded-md border border-border px-2.25 py-1.25 text-[11px] text-muted-foreground">
      <MarkerIcon className="fork-mark mr-1.25 text-primary">
        <ForkIcon />
      </MarkerIcon>
      <MarkerContent>
        Forked from{' '}
        {/* A long title is the common case, and it must not push the sentence onto a second line
            and the transcript down with it. A button because nothing here navigates; the underline
            is what tells a reader it does something. */}
        <Button variant="link" size="sm" className="link inline-block h-auto max-w-[24em] truncate p-0 align-bottom text-primary underline" onClick={onOpen} title="Show the session this was forked from">
          {from.title}
        </Button>
        , before prompt {from.prompt + 1}.
      </MarkerContent>
    </Marker>
  )
}

/**
 * That this session opened with auto-vetting on, which CHECK-11 has said at the top and kept said.
 *
 * In the header for the reason `ForkBanner` is: it stays on screen however far the transcript
 * scrolls, and it is not a `t.Entry`, so no export carries it. The mode's whole effect is a
 * question that never appears, which nothing else on screen could show. Nothing is drawn when the
 * mode is off, since asking is the ordinary state.
 */
function VettingBanner(): React.JSX.Element {
  return (
    <Alert className="fork-banner vetting-banner mt-1.5 rounded-md border border-border border-l-[3px] border-l-primary px-2.25 py-1.25 text-[11px] text-muted-foreground [&_[data-slot=alert-title]]:text-foreground" role="note">
      <AlertTitle>Auto-vetting is on.</AlertTitle>
      <AlertDescription>
        A check that finds nothing reads content to the model without asking you. Kept in{' '}
        <code>~/.bravebot/vetting</code>.
      </AlertDescription>
    </Alert>
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
  return <div className={cn(DECIDED, 'unanswered text-muted-foreground')}>Nobody answered this</div>
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
export function Row({
  entry,
  onRecover,
  onChooseModel,
  onDecide,
  onAnswer,
  onFork,
  forkable,
}: RowProps): React.JSX.Element {
  const card = (
    <EntryCard
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
    <div className="interrupted-request flex flex-col gap-2 rounded-lg border border-border px-4 py-3 text-muted-foreground [&_pre]:max-h-60 [&_pre]:overflow-auto [&_pre]:whitespace-pre-wrap [&_pre]:[overflow-wrap:anywhere]">
      <strong>Request cancelled when the turn ended</strong>
      <Collapsible>
        <CollapsibleTrigger className="flex items-center gap-1">
          <ChevronRightIcon className="size-3! transition-transform group-data-open:rotate-90" />
          Request details
        </CollapsibleTrigger>
        <CollapsibleContent>
          {card}
        </CollapsibleContent>
      </Collapsible>
    </div>
  )
}

/**
 * The card itself, which is the same card whether or not the turn it belongs to ended.
 *
 * What ending it changes is only whether the question can still be answered: `answerable` is
 * false for an interrupted entry, and every arm that would draw controls draws
 * `<Unanswered />` instead. Nothing about how the content is marked depends on it.
 */
function EntryCard({
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
        <Message align="end">
          <MessageContent>
            {/* `overflow-visible` so the fork chip beside it is not clipped away, and `relative`
                so the chip has an edge to be placed against. */}
            <Bubble variant="default" align="end" className="bubble user my-2 ml-auto max-w-[85%]">
              <BubbleContent className={cn(
                'relative overflow-visible rounded-[14px_14px_4px_14px] bg-bubble-user px-4 py-2.5',
                'text-bubble-user-foreground text-sm leading-[1.65] whitespace-pre-wrap',
                '[.app.compact_&]:text-[13px] [.app.compact_&]:leading-[1.5]',
              )}>
                {entry.text}
                {/* Inside the bubble, and positioned out of it. The wrapper this row sits in is
                    `display: contents` and has no box to hang anything off, and the bubble is the
                    only thing here that knows where the row actually is on screen. */}
                <Button
                  variant="ghost"
                  size="icon-xs"
                  className={cn(
                    // A chip rather than a bare glyph: `⑂` is a thin mark, and at this size,
                    // unbacked, it reads as a smudge beside the bubble rather than as something to
                    // press. `select-none` because it sits inside the bubble, and without it a drag
                    // across the prompt would select the glyph and paste it into the copied text.
                    // Centred with `top` rather than `-translate-y-1/2`. The chip is a fixed square,
                    // so the arithmetic is exact either way, but Tailwind's half-translate sets the
                    // independent `translate` property, and a pointer aimed at the middle of the
                    // chip then lands on the bubble behind it — `drive-fork.mjs` cannot click the
                    // control at all, and neither can anything else driving the window.
                    'fork-here absolute top-[calc(50%-12px)] left-[-30px] size-6 rounded-[7px]',
                    'border border-border bg-background p-0 leading-none text-muted-foreground select-none',
                    // Hidden until the row is under the pointer, because a transcript is a column
                    // of these and a control on every one would be a column of controls. Kept in
                    // the tree rather than rendered on hover so it can be tabbed to, and shown when
                    // focused — a control only a pointer can reach is one a keyboard cannot fork
                    // with.
                    'opacity-0 transition-[opacity,color] duration-[120ms] ease-out focus-visible:opacity-100',
                    // Plainly unavailable rather than gone: a control that vanishes mid-turn is one
                    // the reader has to go looking for when the turn ends.
                    forkable
                      ? 'group-hover/bubble:opacity-100 hover:border-primary hover:bg-background hover:text-primary'
                      : 'cursor-default group-hover/bubble:opacity-35',
                  )}
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
                </Button>
              </BubbleContent>
            </Bubble>
          </MessageContent>
        </Message>
      )

    case 'assistant':
      // The only formatted surface in the app. See Markdown.tsx for why it is the only one.
      return (
        <Message align="start">
          <MessageContent>
            <Bubble variant="ghost" align="start" className="bubble assistant my-2 w-full max-w-full">
              <BubbleContent className={cn(
                'w-full max-w-full bg-transparent px-0 py-1.5 text-sm leading-[1.65]',
                '[.app.compact_&]:text-[13px] [.app.compact_&]:leading-[1.5]',
                '[&_pre]:text-xs',
              )}>
                <Markdown text={entry.text} />
              </BubbleContent>
            </Bubble>
          </MessageContent>
        </Message>
      )

    case 'narration':
      return <div className="narration mx-0.5 my-2 text-muted-foreground italic">{entry.text}</div>

    case 'attached':
      // A line rather than a bubble, and the path rather than the contents. This is the same
      // register the tool lines are in — something that happened on the way to the reply — which
      // is what it is: a file somebody named, read at the top of a turn.
      return (
        <div className="attached mx-0.5 my-2 truncate font-mono text-[11px] text-muted-foreground/70" title={entry.path}>
          Read {entry.path}
        </div>
      )

    case 'consolidation':
      // The same line register as the attachment above, and deliberately not a bubble. This is
      // something that happened on the way to a reply rather than something anybody said, and the
      // whole reason it has an entry of its own is that drawing it as a prompt would claim
      // otherwise. Reached only from the record's own tag, never from a message's wording, so
      // nothing anybody types earns this row.
      return <div className="attached consolidation mx-0.5 my-2 truncate font-mono text-[11px] text-muted-foreground/70">Asked to bring its memory up to date</div>

    case 'watch':
      return <div className="watch-turn grid gap-1.5 border-l-2 border-primary px-4 py-3"><strong>{entry.text}</strong><span className="text-xs text-muted-foreground">Automatic turn · file contents still follow normal read permissions</span></div>
    case 'error':
      return <ErrorCard category={entry.category} attempts={entry.attempts} status={entry.status} detail={entry.text} onRetry={onRecover} onModel={onChooseModel} />

    case 'replayed-tool':
      // No outcome, because the record does not keep one. Drawn quietly for the same
      // reason: a call the agent could not even name reads as "Tool", and giving that
      // the prominence of a real line would be worse than the gap.
      return <div className="tool replayed my-0.5 border-l-2 border-border px-2 py-0.75 font-mono text-[11px] text-muted-foreground opacity-55">{entry.text}</div>

    case 'tool': {
      const { activity, landing } = entry
      const running = activity.note === null
      return (
        <div className={cn(
          'tool my-0.5 flex flex-wrap items-baseline gap-1.5 border-l-2 border-border px-2 py-0.75 text-xs text-muted-foreground',
          activity.failed && 'failed border-l-destructive',
          running && 'running',
        )}>
          <span className="verb font-semibold text-foreground">{activity.verb}</span>
          {activity.target && <span className="target font-mono text-[11px]">({activity.target})</span>}
          {running ? (
            <span className="ellipsis text-primary">…</span>
          ) : (
            <span className={cn('note text-muted-foreground/70', activity.failed && 'text-destructive')}>
              {activity.note}
              {waitedWord(activity.waitedSeconds)}
            </span>
          )}
          {landing && isConfined(landing) && (
            <span className="confined rounded-[4px] bg-confine/10 px-1.25 text-[10px] tracking-[0.03em] text-confine uppercase" title={landingHint(landing)}>
              {landing === 'quarantined' ? 'quarantined' : 'name only'}
            </span>
          )}
        </div>
      )
    }

    case 'quarantined': {
      const { shown } = entry
      return (
        // Confined content is marked structurally, not by a line of text it could imitate: the
        // hatching is the one thing on screen a reply cannot draw for itself.
        <div className={cn(
          'quarantine my-2.5 overflow-hidden rounded-[10px] border border-confine',
          '[background:repeating-linear-gradient(45deg,transparent,transparent_7px,var(--confine-bg)_7px,var(--confine-bg)_14px)]',
        )}>
          <div className="quarantine-head flex items-baseline gap-2 border-b border-confine bg-background px-2.5 py-1.5 text-[11px]">
            <span className="mark text-[10px] font-bold text-confine uppercase">confined</span>
            {/* Ellipsised, and it is a path: what gets cut is the part that identifies it. */}
            <span className="origin flex-1 truncate font-mono" title={shown.origin}>
              {shown.origin}
            </span>
            <span className="label font-mono text-muted-foreground/70">{shown.label}</span>
          </div>
          <pre className={PREVIEW}>{shown.preview.join('\n')}</pre>
          <div className="quarantine-foot border-t border-border bg-background px-2.5 py-1.25 text-[10px] text-muted-foreground/70">
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
        // Reviewing a body nobody vouched for is a different act, and must not look alike.
        <div className={cn(CARD, request.untrusted && 'untrusted border-2 border-warning')}>
          <div className={HEAD}>
            <span className={INTENT}>{request.intent}</span>
            <code className={PATH}>{request.path}</code>
            <span className={COUNTS}>
              +{request.added} −{request.removed}
            </span>
          </div>

          {request.untrusted && (
            <p className={WARN}>
              This came from somewhere nobody vouched for. The agent never read it — an
              isolated processor wrote it. Read it as you would a stranger’s patch.
            </p>
          )}
          {!request.exact && (
            <p className={WARN}>
              The files were too dissimilar to diff exactly. This is an approximation of
              the change.
            </p>
          )}

          <p className={SCOPE}>{request.existing ? 'Update an existing project file.' : 'Create a new project file.'} This decision applies to the change shown below.</p>
          {request.remark && <div className={cn('processor-remark', NOTICE)}><strong>Processor’s remark · untrusted</strong>
            <pre className="my-2 whitespace-pre-wrap [overflow-wrap:anywhere]">{request.remark.preview.join('\n')}</pre>
            <small className="leading-[1.5] text-muted-foreground">{request.remark.label}{request.remark.lines > request.remark.preview.length ? ` · ${request.remark.lines - request.remark.preview.length} more lines not shown` : ''}. Review the diff before approving.</small>
          </div>}
          {/* The one notice that is a question about the lines below it, so it is the one that
              carries the warn colours rather than the plain border the others get. */}
          {request.credentials && request.credentials.length > 0 && <div className={cn('credential-finding', NOTICE, 'border-warning bg-warning/10')}><strong className="text-warning">This looks like it would put a secret in the tree</strong>
            <ul className="my-2 list-none p-0">{request.credentials.map((found) => <li className="my-1 font-mono text-[11px] [overflow-wrap:anywhere]" key={found}>{found}</li>)}</ul>
            <small className="leading-[1.5] text-muted-foreground">Going by the name beside the value and how the value reads. Nothing recognised it as a particular provider’s key, so it is a guess and yours to settle.</small>
          </div>}
          <Diff changes={request.changes} />

          {!answerable ? (
            <Unanswered />
          ) : decision === null ? (
            <div className={ACTIONS}>
              <Button variant="outline" className="reject hover:bg-destructive/10 hover:text-destructive" onClick={() => onDecide('confirm', request.request, false)}>
                Don’t write
              </Button>
              <Button className="approve font-medium" onClick={() => onDecide('confirm', request.request, true)}>
                {request.existing ? 'Apply this change' : 'Create this file'}
              </Button>
            </div>
          ) : (
            <div className={cn(DECIDED, decision, decidedTone(decision))}>
              {decision === 'approve' ? 'You approved this write' : 'You refused this write'}
            </div>
          )}
        </div>
      )
    }

    case 'run': {
      const { request, decision, remember } = entry
      return (
        // A run that would hand the user's own data to a program is marked the same way an
        // unvouched-for write is, on confidentiality rather than integrity. Two different reasons
        // to look twice, drawn alike because the instruction to the reader is the same one.
        <div className={cn(CARD, 'run', request.releasesPrivate && 'releases border-2 border-warning')}>
          <div className={HEAD}>
            <span className={INTENT}>run</span>
            <code className={PATH}>{request.directory}</code>
          </div>

          {/* The line the planner wrote, above the plan and as context. It is not what the
              answer binds to: two spellings that compile alike are one thing to agree to, and
              the plan below is the one being agreed to. Drawn all the same, because a reader
              comparing the two is what would catch a compiler that got the line wrong. */}
          {/* A run of spaces in a quoted argument is a difference between the line and the plan, so
              the code here keeps its whitespace: collapsing it the way HTML does by default would
              hide exactly the discrepancy these two rows are drawn to expose. */}
          {request.line && <p className={cn(SCOPE, '[&_code]:whitespace-pre-wrap')}><strong>The model wrote:</strong> <code>{request.line}</code></p>}

          {/* The argv, one stage per line, with what each name resolved to underneath.
              Both are shown because they are two different claims: $PATH decides what
              `grep` means, and a person vouching for a program should be looking at the
              binary rather than the word. */}
          {request.plan && <p className={cn(SCOPE, '[&_code]:whitespace-pre-wrap')}><strong>Execution plan:</strong> <code>{request.plan}</code></p>}
          {request.stdin && <p className={cn(SCOPE, '[&_code]:whitespace-pre-wrap')}><strong>Standard input:</strong> <code>{request.stdin}</code></p>}
          {!!request.writes?.length && <div className={SCOPE}><strong>Files created or modified:</strong><ul>{request.writes.map(path => <li key={path}><code>{path}</code></li>)}</ul></div>}
          {/* Monospaced and allowed to wrap: a long command must be readable in full, since
              approving what is cut off at the edge of a card is the thing this card exists to
              prevent. */}
          <ol className="stages m-0 list-none px-3 py-2">
            {request.stages.map((stage, index) => (
              <li className="py-0.75 [&+li]:mt-0.75 [&+li]:border-t [&+li]:border-dashed [&+li]:border-border [&+li]:pt-1.5" key={index}>
                <code className="argv block font-mono text-xs whitespace-pre-wrap [overflow-wrap:anywhere]">{stage.display}</code>
                <span className="resolved block font-mono text-[10px] text-muted-foreground/70 [overflow-wrap:anywhere]">
                  {stage.resolved ?? 'not found on PATH'}
                </span>
              </li>
            ))}
          </ol>

          <p className={SCOPE}>Run this command in the project folder shown above. “Run once” approves only this execution.</p>
          {answerable && decision === null && <p className={SCOPE}><strong>Remembered approval:</strong> {request.vouches.map((v) => v.display).join('; ')}. Covers these exact commands and trusts their output for this conversation, including after reopening it. Revoke through Permissions.</p>}
          {!!request.ambient?.length && (
            <div className={WARN}>
              This spends access that is yours elsewhere. Nobody is asked for it at the moment it
              is used, and nothing here takes it back afterwards.
              <ul>
                {request.ambient.map((spent) => (
                  <li key={`${spent.authority}:${spent.named}`}>
                    <code>{spent.named}</code>: {t.ambientSentence(spent.authority)}
                  </li>
                ))}
              </ul>
            </div>
          )}
          {request.releasesPrivate && (
            <p className={WARN}>
              This hands your own data to the program. Whatever it does with those bytes
              happens somewhere the agent stops governing them.
            </p>
          )}

          {!answerable ? (
            <Unanswered />
          ) : decision === null ? (
            <div className={ACTIONS}>
              <Button variant="outline" className="reject hover:bg-destructive/10 hover:text-destructive" onClick={() => onDecide('run', request.request, false)}>
                Don’t run
              </Button>
              <Button className="approve font-medium" onClick={() => onDecide('run', request.request, true)}>
                Run once
              </Button>
              {/* Separate from "Run once" rather than a checkbox beside it: remembering
                  answers every later question about these programs, so it should take its
                  own deliberate press. The title says exactly what it would cover. */}
              <Button
                variant="outline"
                className="approve always border-border bg-transparent text-foreground"
                title={`Stop asking about: ${request.vouches.map((v) => v.display).join(', ')}`}
                onClick={() => onDecide('run', request.request, true, true)}
              >
                Trust command and output
              </Button>
            </div>
          ) : (
            <div className={cn(DECIDED, decision, decidedTone(decision))}>
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
        // Output the planner has not read is confined content, and is drawn as confined content
        // rather than as a quotation: it is the one card whose whole body is untrusted bytes.
        <div className={cn(CARD, 'output border-confine')}>
          <div className={HEAD}>
            <span className={cn(INTENT, 'text-confine')}>read output</span>
            <code className={PATH}>{request.command}</code>
            <span className={COUNTS}>
              {request.lines} line{request.lines === 1 ? '' : 's'}
            </span>
          </div>

          <p className={WARN}>
            The planner has not seen this. Read it yourself before deciding: approving is
            what puts it into the model’s context, and anything in here that reads like an
            instruction will be read there as one.
          </p>

          <VettingNotice vetting={request.vetting} />
          <pre className={PREVIEW}>{request.output}</pre>

          {!answerable ? (
            <Unanswered />
          ) : decision === null ? (
            <div className={ACTIONS}>
              <Button
                variant="outline"
                className="reject hover:bg-destructive/10 hover:text-destructive"
                onClick={() => onDecide('output', request.request, false)}
              >
                Keep it out
              </Button>
              <Button
                className="approve font-medium"
                onClick={() => onDecide('output', request.request, true)}
              >
                Let the planner read it
              </Button>
            </div>
          ) : (
            <div className={cn(DECIDED, decision, decidedTone(decision))}>
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
      return <div className={cn(CARD, 'vetted-read')}>
        <div className={HEAD}><span className={INTENT}>read once</span><code className={PATH}>{request.origin}</code><span className={COUNTS}>{request.lines} lines</span></div>
        <p className={SCOPE}>Expected contents: {request.expects}</p>
        <VettingNotice vetting={request.vetting} />
        <p className={WARN}>Approval lets the planner read only this content. It does not trust this file for future reads.</p>
        <pre className={PREVIEW}>{request.content}</pre>
        {!answerable ? <Unanswered /> : decision === null ? <div className={ACTIONS}>
          <Button variant="outline" className="reject hover:bg-destructive/10 hover:text-destructive" onClick={() => onDecide('vet', request.request, false)}>Keep it out</Button>
          <Button className="approve font-medium" onClick={() => onDecide('vet', request.request, true)}>Let the planner read once</Button>
        </div> : <div className={cn(DECIDED, decision, decidedTone(decision))}>{decision === 'approve' ? 'You allowed this content once' : 'You kept this content out'}</div>}
      </div>
    }
    case 'ask':
      return <Questions request={entry} answers={entry.answers} onAnswer={onAnswer} />

    case 'vouch': {
      const { request, decision } = entry
      return (
        <div className={cn(CARD, 'vouch')}>
          <div className={HEAD}>
            <span className={cn(INTENT, 'text-confine')}>vouch</span>
            <code className={PATH}>{request.path}</code>
          </div>

          <p className={WARN}>
            Vouching records a standing rule for this path, so it applies to later reads as
            well as this one. Only do it for content you know the origin of.
          </p>

          <VettingNotice vetting={request.vetting} />
          <pre className={PREVIEW}>{request.preview}</pre>
          {request.truncated && (
            <div className="quarantine-foot border-t border-border bg-background px-2.5 py-1.25 text-[10px] text-muted-foreground/70">
              This is the beginning of the file, not all of it.
            </div>
          )}

          {!answerable ? (
            <Unanswered />
          ) : decision === null ? (
            <div className={ACTIONS}>
              <Button
                variant="outline"
                className="reject hover:bg-destructive/10 hover:text-destructive"
                onClick={() => onDecide('vouch', request.request, false)}
              >
                Leave it confined
              </Button>
              <Button
                className="approve font-medium"
                onClick={() => onDecide('vouch', request.request, true)}
              >
                Vouch for this path
              </Button>
            </div>
          ) : (
            <div className={cn(DECIDED, decision, decidedTone(decision))}>
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
  return (
    <Alert className={cn(
      'vetting-notice',
      NOTICE,
      '[&_p]:my-2 [&_p]:whitespace-pre-wrap [&_p]:[overflow-wrap:anywhere]',
      verdict === 'safe' ? 'safe' : 'caution border-l-[3px] border-l-primary',
    )} variant={verdict === 'unsafe' ? 'destructive' : 'default'}>
      <AlertTitle>{label}</AlertTitle>
      <AlertDescription className="flex flex-col gap-1">
        {vetting?.reason && <p>{vetting.reason}</p>}
        {vetting?.detail && <p>{vetting.detail}</p>}
        <small className="leading-[1.5] text-muted-foreground">{!vetting ? 'No checker assessment was recorded. Review the content before deciding.' : verdict === 'safe' || verdict === 'unsafe' ? 'The checker received this content at the backend before this question. Its assessment can be wrong; you decide whether the planner may read it.' : 'The check did not complete. Content may already have reached the backend. You still decide whether the planner may read it.'}</small>
      </AlertDescription>
    </Alert>
  )
}
