import { Watches } from './Watches'
import type { FileAttachment } from '../../shared/files'
import { Permissions } from './Permissions'
import { cn } from 'cn'
import { forwardRef, useLayoutEffect, useEffect, useMemo, useRef, useState } from 'react'
import { isConfined, type AskAnswer, type AskPrompt, type Phase, type Shown, type TodoRow } from '../../shared/protocol'
import * as t from '../transcript'
import type { Side } from '../columns'
import type { Asked } from '../App'
import type { ExportFormat } from '../../shared/export'
import { Diff } from './Diff'
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
import { Button } from './ui/button'
import { ButtonGroup } from './ui/button-group'
import { Input } from './ui/input'
import { Alert, AlertDescription, AlertTitle } from './ui/alert'
import {
  Attachment,
  AttachmentAction,
  AttachmentActions,
  AttachmentContent,
  AttachmentGroup,
  AttachmentTitle,
  AttachmentTrigger,
} from './ui/attachment'
import { Badge } from './ui/badge'
import { Bubble, BubbleContent } from './ui/bubble'
import { Card as ShadCard, CardContent, CardFooter, CardHeader, CardTitle } from './ui/card'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from './ui/collapsible'
import { Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from './ui/empty'
import { Field, FieldContent, FieldDescription, FieldLegend, FieldSet } from './ui/field'
import { InputGroup, InputGroupAddon, InputGroupButton, InputGroupInput, InputGroupTextarea } from './ui/input-group'
import { Kbd } from './ui/kbd'
import { Marker, MarkerContent, MarkerIcon } from './ui/marker'
import { Message, MessageContent } from './ui/message'
import { Spinner } from './ui/spinner'
import { Toggle } from './ui/toggle'
import { ToggleGroup, ToggleGroupItem } from './ui/toggle-group'
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip'

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
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          variant="ghost"
          size="icon-sm"
          className={cn('fold-toggle', side)}
          aria-expanded={!collapsed}
          aria-controls={side === 'left' ? 'sessions-column' : 'context-column'}
          aria-label={side === 'left' ? 'Session list' : 'Context panel'}
          onClick={() => onToggle(side)}
        >
          {/* Pointing outward when folded — the way the column will come back — and inward
              when open. Decorative: the button is already named and its state announced. */}
          <span className={cn('fold-chevron', !collapsed && 'open')} aria-hidden="true">
            {side === 'left' ? '›' : '‹'}
          </span>
        </Button>
      </TooltipTrigger>
      <TooltipContent>{collapsed ? 'Show' : 'Hide'} {what}</TooltipContent>
    </Tooltip>
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
  const readingAnchor = useRef<{ handle: string; id: string; offset: number } | null>(null)
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
  const rememberReadingAnchor = (element: HTMLElement): void => {
    if (following.current || !live) {
      readingAnchor.current = null
      return
    }
    const top = element.getBoundingClientRect().top
    const wrapper = [...element.querySelectorAll<HTMLElement>('[data-entry-id]')].find((candidate) => {
      const row = candidate.firstElementChild as HTMLElement | null
      return row ? row.getBoundingClientRect().top >= top + 20 : false
    })
    const row = wrapper?.firstElementChild as HTMLElement | null
    readingAnchor.current = wrapper && row
      ? { handle: live.handle, id: wrapper.dataset.entryId ?? '', offset: row.getBoundingClientRect().top - top }
      : null
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
  useLayoutEffect(() => {
    const restore = (): void => {
      const element = scroller.current
      const anchor = readingAnchor.current
      if (!element || !anchor || anchor.handle !== live?.handle || following.current) return
      const wrapper = element.querySelector<HTMLElement>(`[data-entry-id="${CSS.escape(anchor.id)}"]`)
      const row = wrapper?.firstElementChild as HTMLElement | null
      if (!row) return
      const offset = row.getBoundingClientRect().top - element.getBoundingClientRect().top
      element.scrollTop += offset - anchor.offset
      lastScroll.current = element.scrollTop
      readingAnchor.current = { ...anchor, offset: row.getBoundingClientRect().top - element.getBoundingClientRect().top }
    }
    restore()
    // Radix disclosure content can finish measuring after parent layout effects. A second
    // frame keeps the same reading anchor stable when late notices expand above it.
    const frame = requestAnimationFrame(restore)
    return () => cancelAnimationFrame(frame)
  })
  useEffect(() => {
    const element = scroller.current
    if (!element) return
    const remember = () => rememberReadingAnchor(element)
    element.addEventListener('scroll', remember, { passive: true })
    return () => element.removeEventListener('scroll', remember)
  }, [live?.handle])
  useEffect(() => {
    const element = scroller.current
    if (!element) return
    let frame = 0
    const restore = (): void => {
      const anchor = readingAnchor.current
      if (!anchor || anchor.handle !== live?.handle || following.current) return
      const wrapper = element.querySelector<HTMLElement>(`[data-entry-id="${CSS.escape(anchor.id)}"]`)
      const row = wrapper?.firstElementChild as HTMLElement | null
      if (!row) return
      const offset = row.getBoundingClientRect().top - element.getBoundingClientRect().top
      element.scrollTop += offset - anchor.offset
      lastScroll.current = element.scrollTop
    }
    const observer = new MutationObserver(() => {
      cancelAnimationFrame(frame)
      frame = requestAnimationFrame(restore)
    })
    observer.observe(element, { childList: true, subtree: true, characterData: true, attributes: true })
    return () => { observer.disconnect(); cancelAnimationFrame(frame) }
  }, [live?.handle])
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
      {live && <div className="conversation-toolbar flex flex-wrap items-center gap-0.5 px-[38px] pt-1.5 pb-0.5">
        <Toggle size="sm" pressed={searching} onPressedChange={setSearching} aria-expanded={searching}>Find</Toggle>
        <Button variant="ghost" size="sm" onClick={() => setPermissions(true)}>Permissions</Button>
        <Button variant="ghost" size="sm" onClick={() => setWatches(true)}>Watches</Button>
        <Toggle size="sm" pressed={focusedLayout !== null} onPressedChange={() => {
          if (focusedLayout) {
            for (const side of ['left', 'right'] as const) if (collapsed[side] !== focusedLayout[side]) onToggle(side)
            setFocusedLayout(null)
          } else {
            setFocusedLayout({ ...collapsed })
            for (const side of ['left', 'right'] as const) if (!collapsed[side]) onToggle(side)
          }
        }}>{focusedLayout ? 'Exit focus' : 'Focus'}</Toggle>
        <Toggle size="sm" pressed={preferences.density === 'compact'} onPressedChange={() => setExperience('density', preferences.density === 'compact' ? 'comfortable' : 'compact')}>
          {preferences.density === 'compact' ? 'Comfortable view' : 'Compact view'}
        </Toggle>
        <ExportMenu canExport={canExport} includeTools={includeTools} onToggleTools={onToggleTools} onExport={onExport} />
      </div>}
      {backendReady === false && <Alert className="backend-status flex flex-col gap-1.5 border-y border-warn/20 bg-warn/10 px-[58px] py-2.5 text-foreground" role="status">
        <AlertTitle><strong>Backend setup needed</strong></AlertTitle>
        <AlertDescription>
          <span>You can browse conversations and prepare drafts.</span>
          <div><Button variant="outline" size="sm" onClick={onSetup}>Setup help</Button><Button variant="outline" size="sm" onClick={onCheckBackend}>Check again</Button><Button variant="outline" size="sm" onClick={onDiagnostics}>Diagnostics</Button></div>
        </AlertDescription>
      </Alert>}
      {live && <div className="context-status px-5 py-1.5 text-[11px] text-muted-foreground" title="The model’s last request size, not accumulated token usage. New messages may change the next request.">
        {live.phase === 'compacting' ? 'Summarising context…' : live.contextTokens === undefined ? 'Context measurement unavailable' : live.contextTokens === 0 ? 'Context not yet measured' : `${live.contextTokens.toLocaleString()} context tokens at last request`}
        {!!live.archived && <span> · Earlier context summarised</span>}
      </div>}
      {problem && <ErrorCard detail={problem} />}
      {searching && <InputGroup className="conversation-search gap-1.25 rounded-none border-0 border-t border-border bg-muted px-5 py-2">
        <InputGroupInput className="min-h-8 rounded-md border border-border bg-card px-2 py-1.5" autoFocus type="search" aria-label="Find in conversation" placeholder="Find in conversation…" value={query}
          onChange={(event) => { setQuery(event.target.value); setMatch(0) }}
          onKeyDown={(event) => { if (event.key === 'Escape') setSearching(false); if (event.key === 'Enter') setMatch((n) => n + (event.shiftKey ? -1 + matches.length : 1)) }} />
        <InputGroupAddon className="gap-1 text-xs" align="inline-end">
          <span role="status">{matches.length ? `${match % matches.length + 1} of ${matches.length}` : query ? 'No matches' : ''}</span>
          <InputGroupButton size="icon-sm" disabled={!matches.length} onClick={() => setMatch((n) => n + matches.length - 1)} aria-label="Previous match">↑</InputGroupButton>
          <InputGroupButton size="icon-sm" disabled={!matches.length} onClick={() => setMatch((n) => n + 1)} aria-label="Next match">↓</InputGroupButton>
          <InputGroupButton size="icon-sm" onClick={() => setSearching(false)} aria-label="Close search">×</InputGroupButton>
        </InputGroupAddon>
      </InputGroup>}
      {live?.autoVetting && <VettingBanner />}
      {live?.forkedFrom && <ForkBanner from={live.forkedFrom} onOpen={onOpenParent} />}
    </header>
  )

  if (!live) {
    return (
      <main className="transcript empty-state">
        {head}
        <div className="empty-body flex flex-1 place-items-center overflow-y-auto text-left text-muted-foreground">
          <Empty className="w-full max-w-[570px] border-0 p-0">
            <EmptyHeader className="max-w-none">
              <EmptyMedia className="welcome-mark size-[38px] rounded-[10px] bg-primary text-[18px] font-semibold text-primary-foreground">B</EmptyMedia>
              <EmptyTitle><h1 className="text-lg font-medium tracking-tight">What would you like to build?</h1></EmptyTitle>
              <EmptyDescription>Work with an agent in your project. Track changes and review approval requests as you work.</EmptyDescription>
            </EmptyHeader>
            <EmptyContent>
              <Button className="primary" onClick={() => onNew()}>Open project</Button>
              {!!recents.length && <div className="welcome-recents mt-2 flex w-full flex-col items-start gap-2 text-left"><h2 className="text-xs font-semibold text-foreground">Recent projects</h2>{recents.slice(0, 5).map((directory) =>
                <Button variant="outline" className="h-auto w-full flex-col items-start justify-start text-left" key={directory} onClick={() => onNew(directory)}><strong>{directory.split('/').pop()}</strong><span>{directory}</span></Button>)}</div>}
              <p className="welcome-hint text-sm leading-relaxed text-muted-foreground">Choose a conversation to resume work, or create a bot with a purpose and persistent memory.</p>
            </EmptyContent>
          </Empty>
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
        if (following.current) {
          readingAnchor.current = null
          setUnseen(false)
        } else {
          rememberReadingAnchor(element)
        }
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
          <Marker className={`working${bot ? ' working-bot' : ''}`}>
            {/* For a bot, the bot itself, looking down at the page — the one place in the transcript
                its face carries something the header does not: it is *here*, at the point of
                attention, only while something is happening, and its posture is the indicator. It
                mounts already working, since the row exists only while a turn runs. A plain session
                has no face and keeps the spinner. */}
            <MarkerIcon>
              {bot ? (
                <BotAvatar seed={bot.avatar} size={22} doing="working" />
              ) : (
                <Spinner className="spinner" />
              )}
            </MarkerIcon>
            <MarkerContent>
              {live.phase ? phaseWord(live.phase) : 'Working'}
              {live.tokens > 0 && <span className="count"> · {live.tokens} tokens written</span>}
            </MarkerContent>
            {Object.values(live.turns).filter((turn) => turn.status === 'running').slice(-1).map((turn) =>
              <Button variant="link" className="turn-audit-link" key={turn.turn} aria-controls="turn-audit-inspector" onClick={(event) => onAudit(turn.turn, event.currentTarget)}>Audit</Button>)}
            <Button variant="outline" className="cancel" onClick={onCancel}>
              Cancel
            </Button>
          </Marker>
        )}
        <div ref={bottom} />
      </div>

      <Alert className="attention-bar" role="status" aria-live="polite">
        <AlertDescription className="attention-content">
          {pending ? <Button variant="ghost" className="pending-jump" onClick={() => {
            document.dispatchEvent(new CustomEvent('bravebot:reveal-entry', { detail: pending.id }))
            const element = scroller.current?.querySelector<HTMLElement>(`[data-entry-id="${pending.id}"]`)
            jump(element ?? bottom.current)
          }}>{pending.kind === 'ask' ? 'Your answer is needed' : 'Approval needed'} · {waitingOn(pending.kind)} — Review ↑</Button> :
            live.running ? <span>{live.phase ? phaseWord(live.phase) : 'Working'} · You can draft your next message</span> :
            <span>{live.entries.at(-1)?.kind === 'error' ? 'Needs attention' : live.entries.length ? 'Ready for your next message' : 'Ready to begin'}</span>}
        </AlertDescription>
        {unseen && <Button variant="outline" size="sm" onClick={latest}>New activity ↓</Button>}
      </Alert>
      <footer className="composer">
        {queued.length > 0 && <ShadCard className="queued-messages">
          <CardHeader className="queued-head flex-row items-center gap-2 p-0">
            <CardTitle><strong>{queuePaused ? 'Queue paused' : 'Queued after this turn'}</strong></CardTitle>
            {queuePaused && <Button variant="outline" size="sm" disabled={live.running || backendReady === false} onClick={onResumeQueued}>Resume queue</Button>}
          </CardHeader>
          <CardContent className="queued-list p-0">
            {queued.map((text, index) => <div className="flex items-center gap-2" key={index}><span className="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap">{text}</span><Button variant="ghost" size="icon-xs" aria-label={`Remove queued message ${index + 1}`} onClick={() => onRemoveQueued(index)}>×</Button></div>)}
          </CardContent>
        </ShadCard>}
        {attachments.length > 0 && <>
          <Alert className="attachment-context">
            <AlertDescription>These files will be sent as trusted context with your message.</AlertDescription>
          </Alert>
          <AttachmentGroup className="attachment-chips">
            {attachments.map((file) => <Attachment key={file.id} size="sm">
              <AttachmentTrigger aria-label={`Preview attachment ${file.path}`} onClick={() => setPreviewPath(file.path)} />
              <AttachmentContent><AttachmentTitle title={file.path}>{file.path}</AttachmentTitle></AttachmentContent>
              <AttachmentActions>
                <AttachmentAction aria-label={`Remove attachment ${file.path}`} onClick={() => onRemoveAttachment(file.id)}>×</AttachmentAction>
              </AttachmentActions>
            </Attachment>)}
          </AttachmentGroup>
        </>}
        <InputGroup className="composer-box">
          <InputGroupTextarea ref={input} rows={2} value={draft} aria-label="Message the agent" title="Unsent drafts are saved locally on this device. Clear the message to remove its saved draft."
            placeholder={pending ? 'Draft your next message while you review…' : 'Describe a task, ask a question, or paste code…'}
            onChange={(event) => onDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter' && !event.shiftKey && !event.metaKey && !event.ctrlKey && !event.altKey
                && !event.nativeEvent.isComposing && event.keyCode !== 229) {
                event.preventDefault()
                if (!event.repeat && !live.running && backendReady !== false && draft.trim()) { latest(); onSubmit() }
              }
            }} />
          <InputGroupAddon className="composer-toolbar" align="block-end">
            <ModelPicker session={live.handle} scope={bot ? 'bot' : 'conversation'} key={live.handle} model={live.model} disabled={live.running} onChoose={onModel} />
            <Button variant="outline" className="attach-files" onClick={onAttach} disabled={attachments.length >= 5} title="Choose project files to share as trusted context">Attach files</Button>
            <span className="composer-hint"><Kbd>Enter</Kbd> to send · <Kbd>Shift+Enter</Kbd> for newline</span>
            {live.running && <Button variant="destructive" className="stop" onClick={onCancel}>Stop</Button>}
            <Button className="send" onClick={() => { latest(); live.running ? onQueue() : onSubmit() }} disabled={!draft.trim() || !!live.askingTrust || backendReady === false}>
              {live.running ? 'Queue message' : 'Send'}
            </Button>
          </InputGroupAddon>
        </InputGroup>
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
    <ButtonGroup className="export-split">
      <PopMenu
        open={open}
        trigger={<Button
          variant="outline"
          className="export-open"
          disabled={!canExport}
          title={canExport ? 'Export this conversation' : 'Nothing has been said yet'}
        >
          Export
          <span className="export-chevron" aria-hidden="true">
            ⌄
          </span>
        </Button>}
        items={items}
        label="Export the conversation as"
        onChoose={(id) => (id === TOOLS ? onToggleTools() : onExport(id as ExportFormat))}
        onOpenChange={setOpen}
      />
    </ButtonGroup>
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
 * The container an approval is drawn in: a real shadcn Card, flush so its own parts carry the
 * padding and rules, and carrying the semantic hooks the drivers and the marking tests key on.
 * `confirm` is on every approval; the security state (`untrusted`, `releases`, `output`,
 * `vouch`, `vetted-read`) is the class the LAYER-5 container markings are drawn from, so it
 * lives here on the container rather than on any element content could reach.
 */
function ApprovalCard({ className, ...props }: React.ComponentProps<typeof ShadCard>): React.JSX.Element {
  return <ShadCard className={cn('my-3 gap-0 py-0', className)} {...props} />
}

/** The head row of an approval: what is being asked, and what it is about. */
function ApprovalHead({ children, className }: { children: React.ReactNode; className?: string }): React.JSX.Element {
  return <CardTitle className={cn('confirm-head font-normal leading-snug', className)}>{children}</CardTitle>
}

/** The decisions at the foot of an approval card: buttons, or what became of the question. */
function ApprovalActions({ children }: { children: React.ReactNode }): React.JSX.Element {
  return <CardFooter className="confirm-actions">{children}</CardFooter>
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
    <Collapsible className={`tool-run ${open ? 'open' : ''}`} open={open} onOpenChange={setOpen} asChild>
      <section>
        <CollapsibleTrigger asChild>
          <Button
            variant="ghost"
            className="tool-run-head"
            // The verb in the title, the name staying put — the rule `ColumnToggle` states above.
            title={open ? 'Hide these steps' : 'Show these steps'}
          >
            <span className={`chevron ${open ? 'open' : ''}`} aria-hidden="true">
              ›
            </span>
            {entries.length} step{entries.length === 1 ? '' : 's'}
          </Button>
        </CollapsibleTrigger>
        <CollapsibleContent forceMount>
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
        </CollapsibleContent>
      </section>
    </Collapsible>
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

  const choose = (question: number, chosen: number[]): void => {
    setPicked((old) => old.map((current, at) => (at === question ? chosen : current)))
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
      <ApprovalCard className="confirm ask">
        <CardHeader className="confirm-header gap-0 p-0">
          <ApprovalHead>
            <Badge variant="outline" className="intent">asked</Badge>
            <span className="path">
              {prompts.length} question{prompts.length === 1 ? '' : 's'}
            </span>
          </ApprovalHead>
        </CardHeader>
        <CardContent className="confirm-body p-0">
          {prompts.map((prompt, at) => (
            // Keyed by position, not by `prompt.key`: that key is canonical *content*, and a
            // series may legitimately contain the same question twice. The order never
            // changes — the agent emits one prompt per question, in order — so the index is
            // both stable and unique where the content is only stable.
            <div className="asked-answer" key={at}>
              <div className="question">{prompt.question}</div>
              {answers && <div className="given">{describe(prompt, answers[at])}</div>}
            </div>
          ))}
          {!answers && <Unanswered />}
        </CardContent>
      </ApprovalCard>
    )
  }

  return (
    <ApprovalCard className="confirm ask">
      <CardHeader className="confirm-header gap-0 p-0">
        <ApprovalHead>
          <Badge variant="outline" className="intent">asked</Badge>
          <span className="path">
            {prompts.length} question{prompts.length === 1 ? '' : 's'}
          </span>
        </ApprovalHead>
      </CardHeader>

      <CardContent className="confirm-body p-0">
        {prompts.map((prompt, at) => (
          <FieldSet className="ask-question" key={at}>
            <FieldLegend>
              <span className="header">{prompt.header}</span>
              {prompt.multiple && <Badge variant="secondary" className="any">pick any</Badge>}
            </FieldLegend>
            <Field>
              <FieldContent>
                <FieldDescription className="question">{prompt.question}</FieldDescription>
                <QuestionChoices
                  multiple={prompt.multiple}
                  rows={prompt.rows}
                  selected={picked[at] ?? []}
                  onChange={(selected) => choose(at, selected)}
                />
                <Input
                  className="typed"
                  value={typed[at] ?? ''}
                  aria-label={`Answer: ${prompt.question}`}
                  placeholder={prompt.rows.length > 0 ? 'or say something else…' : 'your answer…'}
                  onChange={(event) =>
                    setTyped((old) => old.map((text, index) => (index === at ? event.target.value : text)))
                  }
                />
              </FieldContent>
            </Field>
          </FieldSet>
        ))}
      </CardContent>

      <ApprovalActions>
        {/* Declining every question is a real answer and the turn continues, so it is a
            button here rather than something a person has to leave blank and guess at. */}
        <Button variant="outline" className="reject" onClick={() => onAnswer(request.request.request, prompts.map(() => ({})))}>
          Decline
        </Button>
        <Button className="approve" onClick={() => onAnswer(request.request.request, collected())}>
          Answer
          {/* Leaving a question blank declines it, which is legitimate but should not be a
              surprise — with several questions on screen it is easy to answer two of three
              and not notice. Said on the button rather than after the fact. */}
          {blank > 0 && prompts.length > 1 && (
            <span className="aside"> · {blank} declined</span>
          )}
        </Button>
      </ApprovalActions>
    </ApprovalCard>
  )
}

function QuestionChoices({
  multiple,
  rows,
  selected,
  onChange,
}: {
  multiple: boolean
  rows: AskPrompt['rows']
  selected: number[]
  onChange: (selected: number[]) => void
}): React.JSX.Element {
  const choices = rows.map((row) => (
    <ToggleGroupItem
      className={`choice ${selected.includes(row.index) ? 'picked' : ''}`}
      key={row.index}
      value={String(row.index)}
    >
      <span className="label">{row.label}</span>
      {row.detail && <span className="detail">{row.detail}</span>}
    </ToggleGroupItem>
  ))

  return multiple ? (
    <ToggleGroup
      asChild
      type="multiple"
      className="choices"
      value={selected.map(String)}
      onValueChange={(values) => onChange(values.map(Number).sort((a, b) => a - b))}
    >
      <StylelessToggleGroupRoot>{choices}</StylelessToggleGroupRoot>
    </ToggleGroup>
  ) : (
    <ToggleGroup
      asChild
      type="single"
      className="choices"
      value={selected[0] === undefined ? '' : String(selected[0])}
      onValueChange={(value) => onChange(value ? [Number(value)] : [])}
    >
      <StylelessToggleGroupRoot>{choices}</StylelessToggleGroupRoot>
    </ToggleGroup>
  )
}

/** Keep primitive-generated inline layout styles outside security-marked transcript entries. */
const StylelessToggleGroupRoot = forwardRef<HTMLDivElement, React.ComponentProps<'div'>>(
  function StylelessToggleGroupRoot({ style: _style, ...props }, ref) {
    return <div ref={ref} {...props} />
  },
)

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
      <Button variant="link" className="link" onClick={onOpen} title="Show the session this was forked from">
        {from.title}
      </Button>
      , before prompt {from.prompt + 1}.
    </p>
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
    <p className="fork-banner vetting-banner" role="note">
      <strong>Auto-vetting is on.</strong> A check that finds nothing reads content to the model
      without asking you. Kept in <code>~/.bravebot/vetting</code>.
    </p>
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
  const [detailsOpen, setDetailsOpen] = useState(false)
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
    <Collapsible className="interrupted-request" open={detailsOpen} onOpenChange={setDetailsOpen}>
      <strong>Request cancelled when the turn ended</strong>
      <CollapsibleTrigger asChild>
        <Button variant="ghost" aria-expanded={detailsOpen}>Request details</Button>
      </CollapsibleTrigger>
      <CollapsibleContent forceMount>
        {card}
      </CollapsibleContent>
    </Collapsible>
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
        <Message align="end" className="message-row user-message">
          <MessageContent>
            <Bubble align="end">
              <BubbleContent className="bubble user">
              {entry.text}
              {/* Inside the bubble, and positioned out of it. The wrapper this row sits in is
                  `display: contents` and has no box to hang anything off, and the bubble is the
                  only thing here that knows where the row actually is on screen. */}
              <Button
                variant="ghost"
                size="icon-xs"
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
              </Button>
              </BubbleContent>
            </Bubble>
          </MessageContent>
        </Message>
      )

    case 'assistant':
      // The only formatted surface in the app. See Markdown.tsx for why it is the only one.
      return (
        <Message className="message-row assistant-message">
          <MessageContent>
            <Bubble variant="ghost">
              <BubbleContent className="bubble assistant">
                <Markdown text={entry.text} />
              </BubbleContent>
            </Bubble>
          </MessageContent>
        </Message>
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
      // otherwise. Reached only from the record's own tag, never from a message's wording, so
      // nothing anybody types earns this row.
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
          {landing && isConfined(landing) && (
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
        <ShadCard className="quarantine my-2.5 gap-0 overflow-hidden py-0">
          <div className="quarantine-head">
            <span className="mark">confined</span>
            {/* Ellipsised, and it is a path: what gets cut is the part that identifies it. */}
            <span className="origin" title={shown.origin}>
              {shown.origin}
            </span>
            <span className="label">{shown.label}</span>
          </div>
          <CardContent className="p-0">
            <pre className="preview">{shown.preview.join('\n')}</pre>
          </CardContent>
          <div className="quarantine-foot">
            {shown.lines} line{shown.lines === 1 ? '' : 's'} total ·{' '}
            {shown.reach === 'no_model'
              ? 'in no model’s context: nothing can be sent to read this'
              : 'not in the planner’s context; a processor can be sent to read it'}
          </div>
        </ShadCard>
      )
    }

    case 'confirm': {
      const { request, decision } = entry
      return (
        <ApprovalCard className={cn('confirm', request.untrusted && 'untrusted')}>
          <CardHeader className="confirm-header gap-0 p-0">
            <ApprovalHead>
              <Badge variant="outline" className="intent">{request.intent}</Badge>
              <code className="path">{request.path}</code>
              <span className="counts">
                +{request.added} −{request.removed}
              </span>
            </ApprovalHead>
          </CardHeader>

          <CardContent className="confirm-body p-0">
            {request.untrusted && (
              <Alert className="warn">
                <AlertDescription>This came from somewhere nobody vouched for. The agent never read it — an
                  isolated processor wrote it. Read it as you would a stranger’s patch.</AlertDescription>
              </Alert>
            )}
            {!request.exact && (
              <Alert className="warn">
                <AlertDescription>The files were too dissimilar to diff exactly. This is an approximation of
                  the change.</AlertDescription>
              </Alert>
            )}

            <p className="permission-scope">{request.existing ? 'Update an existing project file.' : 'Create a new project file.'} This decision applies to the change shown below.</p>
            {request.remark && <Alert className="processor-remark"><AlertDescription><strong>Processor’s remark · untrusted</strong>
              <pre>{request.remark.preview.join('\n')}</pre>
              <small>{request.remark.label}{request.remark.lines > request.remark.preview.length ? ` · ${request.remark.lines - request.remark.preview.length} more lines not shown` : ''}. Review the diff before approving.</small>
            </AlertDescription></Alert>}
            {request.credentials && request.credentials.length > 0 && <Alert className="credential-finding">
              <AlertTitle><strong>This looks like it would put a secret in the tree</strong></AlertTitle>
              <AlertDescription><ul>{request.credentials.map((found) => <li key={found}>{found}</li>)}</ul>
                <small>Going by the name beside the value and how the value reads. Nothing recognised it as a particular provider’s key, so it is a guess and yours to settle.</small></AlertDescription>
            </Alert>}
            <Diff changes={request.changes} />
          </CardContent>

          {!answerable ? (
            <ApprovalActions><Unanswered /></ApprovalActions>
          ) : decision === null ? (
            <ApprovalActions>
              <Button variant="outline" className="reject" onClick={() => onDecide('confirm', request.request, false)}>
                Don’t write
              </Button>
              <Button className="approve" onClick={() => onDecide('confirm', request.request, true)}>
                {request.existing ? 'Apply this change' : 'Create this file'}
              </Button>
            </ApprovalActions>
          ) : (
            <ApprovalActions>
              <div className={`decided ${decision}`}>
                {decision === 'approve' ? 'You approved this write' : 'You refused this write'}
              </div>
            </ApprovalActions>
          )}
        </ApprovalCard>
      )
    }

    case 'run': {
      const { request, decision, remember } = entry
      return (
        <ApprovalCard className={cn('confirm run', request.releasesPrivate && 'releases')}>
          <CardHeader className="confirm-header gap-0 p-0">
            <ApprovalHead>
              <Badge variant="outline" className="intent">run</Badge>
              <code className="path">{request.directory}</code>
            </ApprovalHead>
          </CardHeader>

          <CardContent className="confirm-body p-0">
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
            {answerable && decision === null && <p className="permission-scope"><strong>Remembered approval:</strong> {request.vouches.map((v) => v.display).join('; ')}. Covers these exact commands and trusts their output for this conversation, including after reopening it. Revoke through Permissions.</p>}
            {!!request.ambient?.length && (
              <Alert className="warn">
                <AlertDescription>This spends access that is yours elsewhere. Nobody is asked for it at the moment it
                  is used, and nothing here takes it back afterwards.
                  <ul>
                    {request.ambient.map((spent) => (
                      <li key={`${spent.authority}:${spent.named}`}>
                        <code>{spent.named}</code>: {t.ambientSentence(spent.authority)}
                      </li>
                    ))}
                  </ul>
                </AlertDescription>
              </Alert>
            )}
            {request.releasesPrivate && (
              <Alert className="warn">
                <AlertDescription>This hands your own data to the program. Whatever it does with those bytes
                  happens somewhere the agent stops governing them.</AlertDescription>
              </Alert>
            )}
          </CardContent>

          {!answerable ? (
            <ApprovalActions><Unanswered /></ApprovalActions>
          ) : decision === null ? (
            <ApprovalActions>
              <Button variant="outline" className="reject" onClick={() => onDecide('run', request.request, false)}>
                Don’t run
              </Button>
              <Button className="approve" onClick={() => onDecide('run', request.request, true)}>
                Run once
              </Button>
              {/* Separate from "Run once" rather than a checkbox beside it: remembering
                  answers every later question about these programs, so it should take its
                  own deliberate press. The title says exactly what it would cover. */}
              <Button
                variant="outline"
                className="approve always"
                title={`Stop asking about: ${request.vouches.map((v) => v.display).join(', ')}`}
                onClick={() => onDecide('run', request.request, true, true)}
              >
                Trust command and output
              </Button>
            </ApprovalActions>
          ) : (
            <ApprovalActions>
              <div className={`decided ${decision}`}>
                {decision === 'reject'
                  ? 'You refused this command'
                  : remember
                    ? 'You ran this and vouched for the programs'
                    : 'You ran this once'}
              </div>
            </ApprovalActions>
          )}
        </ApprovalCard>
      )
    }

    case 'output': {
      const { request, decision } = entry
      return (
        <ApprovalCard className="confirm output">
          <CardHeader className="confirm-header gap-0 p-0">
            <ApprovalHead>
              <Badge variant="outline" className="intent">read output</Badge>
              <code className="path">{request.command}</code>
              <span className="counts">
                {request.lines} line{request.lines === 1 ? '' : 's'}
              </span>
            </ApprovalHead>
          </CardHeader>

          <CardContent className="confirm-body p-0">
            <Alert className="warn">
              <AlertDescription>The planner has not seen this. Read it yourself before deciding: approving is
                what puts it into the model’s context, and anything in here that reads like an
                instruction will be read there as one.</AlertDescription>
            </Alert>

            {/* In full, never truncated. The answer to this question rests on the bytes, so
                a preview would be asking for an approval of what nobody saw. */}
            <VettingNotice vetting={request.vetting} />
            <pre className="preview">{request.output}</pre>
          </CardContent>

          {!answerable ? (
            <ApprovalActions><Unanswered /></ApprovalActions>
          ) : decision === null ? (
            <ApprovalActions>
              <Button
                variant="outline"
                className="reject"
                onClick={() => onDecide('output', request.request, false)}
              >
                Keep it out
              </Button>
              <Button
                className="approve"
                onClick={() => onDecide('output', request.request, true)}
              >
                Let the planner read it
              </Button>
            </ApprovalActions>
          ) : (
            <ApprovalActions>
              <div className={`decided ${decision}`}>
                {decision === 'approve'
                  ? 'You let the planner read this'
                  : 'You kept this out of the planner’s context'}
              </div>
            </ApprovalActions>
          )}
        </ApprovalCard>
      )
    }

    case 'vet': {
      const { request, decision } = entry
      return (
        <ApprovalCard className="confirm vetted-read">
          <CardHeader className="confirm-header gap-0 p-0">
            <ApprovalHead>
              <Badge variant="outline" className="intent">read once</Badge>
              <code className="path">{request.origin}</code>
              <span className="counts">{request.lines} lines</span>
            </ApprovalHead>
          </CardHeader>
          <CardContent className="confirm-body p-0">
            <p className="permission-scope">Expected contents: {request.expects}</p>
            <VettingNotice vetting={request.vetting} />
            <Alert className="warn"><AlertDescription>Approval lets the planner read only this content. It does not trust this file for future reads.</AlertDescription></Alert>
            <pre className="preview">{request.content}</pre>
          </CardContent>
          {!answerable ? <ApprovalActions><Unanswered /></ApprovalActions> : decision === null ? <ApprovalActions>
            <Button variant="outline" className="reject" onClick={() => onDecide('vet', request.request, false)}>Keep it out</Button>
            <Button className="approve" onClick={() => onDecide('vet', request.request, true)}>Let the planner read once</Button>
          </ApprovalActions> : <ApprovalActions><div className={`decided ${decision}`}>{decision === 'approve' ? 'You allowed this content once' : 'You kept this content out'}</div></ApprovalActions>}
        </ApprovalCard>
      )
    }
    case 'ask':
      return <Questions request={entry} answers={entry.answers} onAnswer={onAnswer} />

    case 'vouch': {
      const { request, decision } = entry
      return (
        <ApprovalCard className="confirm vouch">
          <CardHeader className="confirm-header gap-0 p-0">
            <ApprovalHead>
              <Badge variant="outline" className="intent">vouch</Badge>
              <code className="path">{request.path}</code>
            </ApprovalHead>
          </CardHeader>

          <CardContent className="confirm-body p-0">
            <Alert className="warn">
              <AlertDescription>Vouching records a standing rule for this path, so it applies to later reads as
                well as this one. Only do it for content you know the origin of.</AlertDescription>
            </Alert>

            <VettingNotice vetting={request.vetting} />
            <pre className="preview">{request.preview}</pre>
            {request.truncated && (
              <div className="quarantine-foot">
                This is the beginning of the file, not all of it.
              </div>
            )}
          </CardContent>

          {!answerable ? (
            <ApprovalActions><Unanswered /></ApprovalActions>
          ) : decision === null ? (
            <ApprovalActions>
              <Button
                variant="outline"
                className="reject"
                onClick={() => onDecide('vouch', request.request, false)}
              >
                Leave it confined
              </Button>
              <Button
                className="approve"
                onClick={() => onDecide('vouch', request.request, true)}
              >
                Vouch for this path
              </Button>
            </ApprovalActions>
          ) : (
            <ApprovalActions>
              <div className={`decided ${decision}`}>
                {decision === 'approve'
                  ? 'You vouched for this path'
                  : 'You left it confined'}
              </div>
            </ApprovalActions>
          )}
        </ApprovalCard>
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
  return <Alert className={`vetting-notice ${verdict === 'safe' ? 'safe' : 'caution'}`}>
    <AlertTitle><strong>{label}</strong></AlertTitle>
    <AlertDescription>
      {vetting?.reason && <p>{vetting.reason}</p>}
      {vetting?.detail && <p>{vetting.detail}</p>}
      <small>{!vetting ? 'No checker assessment was recorded. Review the content before deciding.' : verdict === 'safe' || verdict === 'unsafe' ? 'The checker received this content at the backend before this question. Its assessment can be wrong; you decide whether the planner may read it.' : 'The check did not complete. Content may already have reached the backend. You still decide whether the planner may read it.'}</small>
    </AlertDescription>
  </Alert>
}
