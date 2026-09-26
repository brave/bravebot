import { useEffect, useState } from 'react'
import { ChevronRightIcon, XIcon } from 'lucide-react'
import { cn } from '@/lib/utils'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Empty, EmptyDescription } from '@/components/ui/empty'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import { Fold } from './Fold'
import { FileTree } from './FileTree'
import { PanelIcon } from './PanelIcon'
import { PANEL_NAMES, type PanelName } from '../../shared/state'
import { isConfined, type Activity, type Phase, type Shown, type TodoRow } from '../../shared/protocol'
import type { Entry } from '../transcript'

interface Live {
  /** The session's handle. The file tree names it rather than naming a folder. */
  handle: string
  summary: { directory: string }
  entries: Entry[]
  todos: TodoRow[]
  quarantine: Shown[]
  phase: Phase | null
  tokens: number
  running: boolean
}

/**
 * The column itself.
 *
 * `[&>*]:min-w-…` hands every direct child the width the column will come back at, so a fold
 * slides them under a clip instead of reflowing them — panel headings re-wrapping into narrower
 * and narrower shapes for 180ms, on their way to being invisible. A no-op while the column is
 * open, where the two widths are the same number.
 *
 * A folded column must not be tabbable or read out, and a zero-width track does neither on its
 * own; hiding waits for the fold to finish so there is something to watch on the way out, and
 * lifts at once on the way back. Driven from the root class so the column need not know that
 * folding exists.
 *
 * Narrow windows have no room for a third track, so the column stops being one and lies over the
 * transcript instead — with the shadow that says it is on top, and inert while it is folded away.
 */
const COLUMN = cn(
  'context',
  'flex flex-col overflow-hidden bg-sidebar',
  '[&>*]:min-w-[var(--col-right-open)]',
  '[.app.right-folded_&]:invisible [.app.right-folded_&]:[transition:visibility_0s_linear_180ms]',
  '[.app.no-session_&]:invisible',
  'max-[1120px]:absolute max-[1120px]:inset-y-0 max-[1120px]:right-0 max-[1120px]:z-35 max-[1120px]:w-[310px] max-[1120px]:bg-background max-[1120px]:shadow-[-12px_0_32px_#0002]',
  'max-[1120px]:[.app.right-folded_&]:pointer-events-none',
)

/** The lists of paths this column keeps, and the parts one line of one is made of. */
const FILE_LIST = 'files flex flex-col gap-0 text-xs'
const FILE_ROW = 'flex items-baseline gap-1.5 py-[3px]'
/** Full-strength ink: a path is the thing being read here, not an aside about one. */
const FILE_LINK = 'context-link h-auto min-w-0 truncate px-0 py-1 text-left font-mono text-xs text-foreground'
const TAG = 'tag ml-auto text-[9px] uppercase'

/**
 * What each outcome is coloured.
 *
 * Only the two ends are spoken for: a write that landed, and one that will not. Everything
 * between them is the warn colour, because "approved", "applying" and "waiting" are the same
 * thing to a reader — not finished yet — and colouring them apart would claim a difference this
 * panel cannot stand behind.
 */
const TAG_TINT: Record<Write['state'], string> = {
  approved: 'text-warning',
  applying: 'text-warning',
  waiting: 'text-warning',
  applied: 'text-success',
  failed: 'text-destructive',
  refused: 'text-destructive',
  cancelled: 'text-muted-foreground/70',
}

/**
 * The panels, in the order they appear, and the order their buttons appear in.
 *
 * One list rather than a set of `useState`s and a hand-written row of buttons: the bar and the
 * column are then two readings of the same thing, and a panel added later cannot end up in one
 * and not the other. The names come from `shared/state.ts`, which is also what decides whether a
 * name in the preferences file is a panel at all — so the column, the bar and the file on disk are
 * all talking about the same five things.
 */


/**
 * The right-hand column: what this session has touched, and the folder it is touching it in.
 *
 * The first four panels are derived from the transcript rather than tracked separately, so the two
 * cannot disagree about what happened. The fifth is the exception and says so: a file tree reads
 * the disk, because the question it answers — what else is in there, and what does this file look
 * like in a real editor — is not one the transcript can be asked.
 */
export function Context({ live, onClose, audit }: { live: Live | null; onClose: () => void; audit?: React.ReactNode }): React.JSX.Element {
  const [off, setOff] = useState<ReadonlySet<PanelName>>(new Set())

  useEffect(() => {
    void window.bravebot.readPanels().then((panels) => setOff(new Set(panels.off)))
  }, [])

  const on = PANEL_NAMES.filter((name) => !off.has(name))
  const setOn = (next: readonly string[]) => {
    const pressed = new Set(next.filter((name): name is PanelName =>
      (PANEL_NAMES as readonly string[]).includes(name)))
    const nextOff = PANEL_NAMES.filter((name) => !pressed.has(name))
    setOff(new Set(nextOff))
    window.bravebot.writePanels({ off: nextOff })
  }

  const reveal = (path: string) => {
    const entry = [...(live?.entries ?? [])].reverse().find((entry) =>
      entry.kind === 'tool' ? fileTarget(entry.activity.target) === path : entry.kind === 'confirm' ? entry.request.path === path : false)
    if (entry) document.dispatchEvent(new CustomEvent('bravebot:reveal-entry', { detail: entry.id }))
  }

  const filesOn = !off.has('files')
  if (!live) return <aside className={cn(COLUMN, filesOn && 'context-files')} id="context-column" />

  const files = touched(live.entries)
  const writes = written(live.entries)
  const replayed = live.entries.filter((entry) => entry.kind === 'replayed-tool')

  // A session read back off disk has calls in its transcript and nothing behind them:
  // the record keeps what a turn did, not what came of it. Saying "nothing read yet"
  // under a transcript that plainly shows a read is worse than saying nothing — it is
  // the interface contradicting itself. So the two cases are distinguished, and the
  // replayed one says what it actually knows.
  const onlyReplayed = replayed.length > 0 && files.length === 0

  // What each panel is called, which is what its button says it will show or hide. Read off the
  // same value the heading uses, so the bar cannot promise "Files read" over a panel headed
  // "Calls made".
  const labels: Record<PanelName, string> = {
    plan: 'Plan',
    read: onlyReplayed ? 'Calls made' : 'Files read',
    writes: 'Changes',
    confined: 'Confined content',
    files: 'Files',
  }

  return (
    <aside className={cn(COLUMN, filesOn && 'context-files')} id="context-column">
      {/* With the tree on, the body is a column that clips and hands the panel below it the rest
          of the height; with it off, the body is what scrolls. */}
      <div
        className={cn(
          'context-content min-h-0 flex-1',
          filesOn ? 'flex flex-col overflow-hidden' : 'overflow-y-auto',
        )}
        hidden={!!audit}
      >
      {/* One connected row, because these five are one choice about one column rather than five
          unrelated switches — the shape a segmented control has on this platform.

          A hidden panel is hidden in CSS rather than unmounted. Unmounting is tidier to write and
          worse to use: it would throw away which folders somebody had opened in the tree and how
          each panel was folded, so turning a panel off and on again would silently undo their
          work. `display: none` takes it out of the tab order and the accessibility tree just the
          same. */}
      {/* Wrapped, because `.context > *` hands every direct child of this column the width the
          column will come back at when it unfolds — which a full-width row of buttons plus its
          own margins overflows. The wrapper takes that width and the bar sits inside it. */}
      <div className="context-head flex-none px-3.5 pt-3.5 pb-2">
        <div className="inspector-title flex items-center justify-between gap-2 pb-3.5 text-[13px]">
          <strong>Project context</strong>
          <Button
            variant="ghost"
            size="icon-sm"
            className="drawer-close"
            onClick={onClose}
            aria-label="Close context panel"
          >
            <XIcon />
          </Button>
        </div>
        {/* Borders on the insides only, so the row reads as a single object with divisions in it
            rather than as five controls that happen to be adjacent. Nothing of its own behind it:
            the lit segments are the light ones, so the ground has to be the column's, or the
            panels that are *off* would be the ones glowing. */}
        <ToggleGroup
          multiple
          spacing={0}
          className="panel-bar w-full overflow-hidden rounded-[7px] border border-border bg-transparent"
          value={on}
          onValueChange={setOn}
          aria-label="Context panels"
        >
          {PANEL_NAMES.map((name) => (
            <ToggleGroupItem
              key={name}
              value={name}
              // On, in the accent and on a ground of its own. Two differences rather than one:
              // colour alone would leave the state invisible to anybody who cannot see this
              // particular primary, and `aria-pressed` is what says it to a screen reader anyway.
              className={cn(
                'panel-pick h-[26px] flex-1 rounded-none border-0 border-l border-border first:border-l-0',
                'text-muted-foreground/70 hover:bg-foreground/12 hover:text-muted-foreground',
                'aria-pressed:bg-tree aria-pressed:text-primary',
              )}
              aria-controls={`panel-${name}`}
              aria-label={labels[name]}
              title={labels[name]}
            >
              <PanelIcon panel={name} />
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
      </div>

      <Section id="plan" title="Plan" count={live.todos.length} off={off.has('plan')}>
        {live.todos.length === 0 ? (
          <ContextEmpty>{onlyReplayed ? 'No plan was recorded.' : 'No plan yet.'}</ContextEmpty>
        ) : (
          <ul className="todos flex flex-col gap-0 text-xs">
            {live.todos.map((row, index) => (
              <li
                key={index}
                className={cn(
                  row.status,
                  'flex gap-1.5 py-[3px] text-muted-foreground',
                  row.status === 'done' && 'text-muted-foreground/70 line-through',
                  row.status === 'active' && 'font-medium text-foreground',
                )}
              >
                <span className="marker text-primary">
                  {row.status === 'done' ? '✓' : row.status === 'active' ? '▸' : '·'}
                </span>
                {row.content}
              </li>
            ))}
          </ul>
        )}
      </Section>

      {/* Named for what it actually holds. Live, these are files and whether the planner
          was allowed to read them. Replayed, they are every call the turn made — reads,
          writes and processors alike — because that is all the record kept. Calling that
          list "files read" would be a third thing the interface got wrong about a session
          it did not watch. */}
      <Section
        id="read"
        title={labels.read}
        count={onlyReplayed ? replayed.length : files.length}
        off={off.has('read')}
      >
        {onlyReplayed ? (
          <>
            <ContextEmpty>
              From the record. It keeps what each turn did, not what came of it, so
              there is nothing to say about where these landed.
            </ContextEmpty>
            {/* Every path in this list and the two below it ellipsises, and a path clipped
                on the right loses the filename — the one part of it somebody is reading
                for. The tooltip is the whole string back. It repeats what is already on
                screen when the path is short enough to fit, which is the cheaper of the two
                mistakes available: the alternative is measuring every row on every render
                to decide whether to offer one.

                These rows are lines from the record, which have no outcome behind them, so they
                are marked off down the left and drawn back: all they say is that the turn made
                the call. */}
            <ul className={FILE_LIST}>
              {replayed.map((entry) =>
                entry.kind === 'replayed-tool' ? (
                  <li key={entry.id} className={cn('from-record', FILE_ROW, 'border-l-2 border-border pl-1.5')}>
                    <code className="min-w-0 truncate font-mono text-[11px] opacity-70" title={entry.text}>{entry.text}</code>
                  </li>
                ) : null,
              )}
            </ul>
          </>
        ) : files.length === 0 ? (
          <ContextEmpty>Nothing read yet.</ContextEmpty>
        ) : (
          <ul className={FILE_LIST}>
            {files.map((file) => (
              <li key={file.target} className={cn(file.confined && 'confined', FILE_ROW)}>
                <Button
                  variant="link"
                  className={FILE_LINK}
                  onClick={() => reveal(file.target)}
                  title={file.target}
                >
                  {file.target}
                </Button>
                {file.confined && <Badge variant="outline" className={cn(TAG, 'text-confine')}>confined</Badge>}
              </li>
            ))}
          </ul>
        )}
      </Section>

      <Section id="writes" title="Changes" count={writes.length} off={off.has('writes')}>
        {writes.length === 0 ? (
          <ContextEmpty>
            {onlyReplayed
              ? 'Not recorded for past turns.'
              : 'Nothing has been written.'}
          </ContextEmpty>
        ) : (
          <ul className={FILE_LIST}>
            {writes.map((write) => (
              <li key={write.target} className={cn(write.state, FILE_ROW)}>
                <Button
                  variant="link"
                  className={FILE_LINK}
                  onClick={() => reveal(write.target)}
                  title={write.target}
                >
                  {write.target}
                </Button>
                <Badge variant="outline" className={cn(TAG, TAG_TINT[write.state])}>{write.state}</Badge>
              </li>
            ))}
          </ul>
        )}
      </Section>

      <Section
        id="confined"
        title="Confined content"
        count={live.quarantine.length}
        off={off.has('confined')}
      >
        {live.quarantine.length === 0 ? (
          <ContextEmpty>
            {onlyReplayed
              ? 'Not recorded for past turns. Confined content is never written down.'
              : 'Nothing confined.'}
          </ContextEmpty>
        ) : (
          <ul className="confined-list flex flex-col gap-0 text-xs">
            {live.quarantine.map((shown, index) => (
              <li key={index} className="border-b border-border py-[5px]">
                <div className="origin font-mono text-[11px] break-all" title={shown.origin}>
                  {shown.origin}
                </div>
                <div className="detail text-[10px] text-muted-foreground/70">
                  {shown.lines} line{shown.lines === 1 ? '' : 's'} · {shown.label}
                </div>
              </li>
            ))}
          </ul>
        )}
      </Section>

      {/* Last, and on purpose. The four panels above are derived from the transcript — what the
          session touched — and this one reads the disk. Putting it under them keeps that boundary
          visible instead of interleaving two kinds of claim.

          No count: the others count something the session did, where this would count entries in
          the top of a folder, which is a number nobody is waiting for.

          Keyed by the handle so switching sessions resets the tree rather than showing one
          project's folders under another's root while the new listing arrives. */}
      <section
        className={cn(
          'panel files-panel flex min-h-0 flex-1 border-b border-border px-3.5 pt-2 pb-3.5',
          off.has('files') && 'off hidden',
        )}
        id="panel-files"
        aria-label="Project files"
      >
        <FileTree
          key={live.handle}
          session={live.handle}
          root={live.summary.directory}
          running={live.running}
        />
      </section>
      </div>
      {audit}
    </aside>
  )
}

/**
 * What a panel says when it has nothing to list.
 *
 * Its own margins rather than the panel's padding, because a panel that holds one of these is
 * holding a sentence and not a list, and the room a list needs around it makes the sentence look
 * lost. The panel takes its padding back in `has-[.none]`.
 */
function ContextEmpty({ children }: { children: React.ReactNode }): React.JSX.Element {
  return (
    <Empty className="none mt-1 mb-2.5 min-h-0 flex-none items-start gap-0 border-0 p-0 text-left text-muted-foreground/70">
      <EmptyDescription className="text-left text-xs text-inherit">{children}</EmptyDescription>
    </Empty>
  )
}

function Section({
  id,
  title,
  count,
  off = false,
  children,
}: {
  /** Which panel this is, so the button in the bar can point at it. */
  id: PanelName
  title: string
  /** How many things are in it, for the pill in the head. Omitted where there is nothing to count. */
  count?: number
  /**
   * Whether the bar has turned it off. Distinct from folded: folding is about this panel's own
   * contents and lives in the head, where turning it off is a choice about the column and lives
   * at the top of it. A panel that is off keeps everything it knows, including its fold.
   */
  off?: boolean
  children: React.ReactNode
}): React.JSX.Element {
  const [open, setOpen] = useState(true)
  return (
    // Turned off from the bar. Out of the flow rather than folded away: this is not a movement
    // anybody should watch, and it takes the panel out of the tab order and the accessibility
    // tree with it.
    <section className={cn('panel flex-none border-b border-border', off && 'off hidden')} id={`panel-${id}`}>
      <Button
        variant="ghost"
        className="panel-head h-auto w-full justify-start rounded-none px-3.5 py-2.5 text-xs font-semibold tracking-[0.02em] text-muted-foreground"
        aria-expanded={open}
        // The verb in the title and the name staying put, the rule `ColumnToggle` states.
        title={`${open ? 'Hide' : 'Show'} ${title.toLowerCase()}`}
        onClick={() => setOpen(!open)}
      >
        <ChevronRightIcon
          className={cn(
            'chevron size-3! transition-transform duration-[180ms] ease-[cubic-bezier(0.32,0.72,0,1)] motion-reduce:transition-none',
            open && 'open rotate-90',
          )}
          aria-hidden="true"
        />
        {title}
        {count !== undefined && count > 0 && (
          <Badge variant="secondary" className="count ml-auto h-auto rounded-lg px-1.5 py-px text-[10px]">{count}</Badge>
        )}
      </Button>
      {/* A panel holding a sentence instead of a list gives most of its padding back: the
          sentence carries its own, and the two together left a hole under every empty panel. */}
      <Fold
        open={open}
        className="panel-inner px-3.5 pt-2 pb-3.5 has-[.none]:pt-0 has-[.none]:pb-1"
      >
        {children}
      </Fold>
    </section>
  )
}

/**
 * Which files the turn opened, and whether the planner was allowed to read them.
 *
 * The distinction is the whole point of the tool, so it is carried into the list rather
 * than flattened into "files". A name here does not mean the model saw the contents.
 */
function touched(entries: Entry[]): { target: string; confined: boolean }[] {
  const seen = new Map<string, boolean>()
  for (const entry of entries) {
    if (entry.kind !== 'tool' || !entry.activity.target) continue
    if (entry.activity.failed) continue
    if (!namesAFile(entry.activity.verb)) continue
    // A file the turn wrote is not a file it read. It has its own panel, and naming it
    // here as well told the reader the model had seen contents it never opened.
    if (isWrite(entry.activity)) continue
    const confined = entry.landing !== null && isConfined(entry.landing)
    // Once confined, always shown as confined: a file read into quarantine and later
    // named again should not lose the mark.
    seen.set(entry.activity.target, (seen.get(entry.activity.target) ?? false) || confined)
  }
  return [...seen].map(([target, confined]) => ({ target, confined }))
}

/** The agent decorates reference targets with a slot and label; match their underlying path. */
function fileTarget(target: string): string {
  return target.replace(/^ref:\d+\([TU],(?:pub|priv)\):/, '').replace(/^\.\//, '')
}

/** One file the turn wrote, and how far that write got. */
interface Write {
  target: string
  state: 'approved' | 'applying' | 'applied' | 'failed' | 'refused' | 'waiting' | 'cancelled'
}

/**
 * What the turn wrote.
 *
 * A write reaches this column two ways: as a confirmation somebody answered, and as the
 * call itself. Only the first was read here, so a write that needed no confirmation — a
 * path already approved, a session running without prompts — left the panel saying
 * nothing had been written underneath a transcript plainly showing a write. Both are read
 * now, and merged by path so a confirmed write is one row rather than two.
 */
export function written(entries: Entry[]): Write[] {
  const rows = new Map<string, Write>()
  const calls = new Map<string, Activity>()
  for (const entry of entries) {
    if (entry.kind === 'confirm') {
      const call = calls.get(entry.request.path)
      rows.set(entry.request.path, {
        target: entry.request.path,
        state:
          entry.interrupted ? 'cancelled' : entry.decision === 'approve'
            ? call ? call.failed ? 'failed' : call.note === null ? 'applying' : 'applied' : 'approved'
            : entry.decision === 'reject'
              ? 'refused'
              : 'waiting',
      })
      continue
    }
    if (entry.kind !== 'tool' || !isWrite(entry.activity) || !entry.activity.target) continue
    const target = fileTarget(entry.activity.target)
    calls.set(target, entry.activity)
    // A call still running has not changed anything yet, and a refused or failed one never
    // will. Neither may read as "applied".
    const state = entry.activity.failed
      ? 'failed'
      : entry.activity.note === null
        ? 'applying'
        : 'applied'
    // An outcome already recorded for this path — a decision, or the finish of the same
    // call — outranks a line that has not finished, so a pending row cannot overwrite it.
    rows.set(target, { target, state })
  }
  return [...rows.values()]
}

/**
 * Whether a call's target is a file at all.
 *
 * Not every call that has a target has a *path*: a search names a pattern, a skill names a
 * skill, and asking names a count of questions — the agent puts whatever identifies the call
 * in that field, which is right for a transcript line and wrong for this list. Listed
 * indiscriminately, "2 questions" appeared under Files read as though the model had opened a
 * file by that name.
 *
 * An allow-list rather than a list of things to skip, because the failure directions are not
 * equal. A new tool missing from here is absent from a panel; a new tool that slipped past a
 * deny-list would be this interface claiming the model read something it never opened. The
 * verbs are literals from the agent's dispatch table, so this matches the tool that ran.
 */
function namesAFile(verb: string): boolean {
  return verb === 'Read' || verb === 'List' || verb === 'Write' || verb === 'Update'
}

/**
 * Whether a call changed a file.
 *
 * The verbs are literals the driver's dispatch table picks, never anything the model
 * wrote, so matching them matches the tool that ran rather than prose about it. `changes`
 * is checked as well because only a write carries any: a driver that grows a verb this
 * list has not heard of should still land in the Writes panel, since a write missing from
 * it is the failure this exists to prevent.
 */
function isWrite(activity: Activity): boolean {
  return activity.verb === 'Write' || activity.verb === 'Update' || activity.changes.length > 0
}
