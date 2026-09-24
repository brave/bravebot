import { useState } from 'react'
import { FileTree } from './FileTree'
import { type PanelName } from '../../shared/state'
import { isConfined, type Activity, type Phase, type Shown, type TodoRow } from '../../shared/protocol'
import type { Entry } from '../transcript'
import { Badge } from './ui/badge'
import { Button } from './ui/button'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from './ui/collapsible'
import { Empty, EmptyDescription } from './ui/empty'
import { Item } from './ui/item'
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs'
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip'

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
  const [tab, setTab] = useState<'overview' | 'files'>('overview')
  const off = new Set<PanelName>(tab === 'overview' ? ['files'] : ['plan', 'read', 'writes', 'confined'])
  const reveal = (path: string) => {
    const entry = [...(live?.entries ?? [])].reverse().find((entry) =>
      entry.kind === 'tool' ? fileTarget(entry.activity.target) === path : entry.kind === 'confirm' ? entry.request.path === path : false)
    if (entry) document.dispatchEvent(new CustomEvent('bravebot:reveal-entry', { detail: entry.id }))
  }

  if (!live) return <aside className={`context ${tab === 'files' ? 'context-files' : ''}`} id="context-column" />

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
    <aside className={`context ${tab === 'files' ? 'context-files' : ''}`} id="context-column">
      <Tabs value={tab} onValueChange={(value) => setTab(value as typeof tab)} className="context-content gap-0" hidden={!!audit}>
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
      <div className="context-head">
        <div className="inspector-title"><strong>Project context</strong><Tooltip><TooltipTrigger asChild><Button variant="ghost" size="icon-sm" className="drawer-close" onClick={onClose} aria-label="Close context panel">×</Button></TooltipTrigger><TooltipContent>Close context panel</TooltipContent></Tooltip></div>
        <TabsList className="inspector-tabs" variant="line" aria-label="Project context">
          {(['overview', 'files'] as const).map((name) => <TabsTrigger key={name} value={name}>{name === 'overview' ? 'Overview' : 'Files'}</TabsTrigger>)}
        </TabsList>

      </div>

      <TabsContent value="overview" forceMount className="context-overview data-[state=inactive]:hidden">
      <Section id="plan" title="Plan" count={live.todos.length} off={off.has('plan')}>
        {live.todos.length === 0 ? (
          <ContextEmpty>{onlyReplayed ? 'No plan was recorded.' : 'No plan yet.'}</ContextEmpty>
        ) : (
          <ul className="todos">
            {live.todos.map((row, index) => (
              <Item asChild size="sm" key={index}><li className={`${row.status} flex-nowrap rounded-none`}>
                <span className="marker">
                  {row.status === 'done' ? '✓' : row.status === 'active' ? '▸' : '·'}
                </span>
                {row.content}
              </li></Item>
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
                to decide whether to offer one. */}
            <ul className="files">
              {replayed.map((entry) =>
                entry.kind === 'replayed-tool' ? (
                  <Item asChild size="sm" key={entry.id}><li className="from-record flex-nowrap rounded-none">
                    <code title={entry.text}>{entry.text}</code>
                  </li></Item>
                ) : null,
              )}
            </ul>
          </>
        ) : files.length === 0 ? (
          <ContextEmpty>Nothing read yet.</ContextEmpty>
        ) : (
          <ul className="files">
            {files.map((file) => (
              <Item asChild size="sm" key={file.target}><li className={`${file.confined ? 'confined ' : ''}flex-nowrap rounded-none`}>
                <Button variant="link" className="context-link" onClick={() => reveal(file.target)} title={file.target}>{file.target}</Button>
                {file.confined && <Badge variant="outline" className="tag">confined</Badge>}
              </li></Item>
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
          <ul className="files">
            {writes.map((write) => (
              <Item asChild size="sm" key={write.target}><li className={`${write.state} flex-nowrap rounded-none`}>
                <Button variant="link" className="context-link" onClick={() => reveal(write.target)} title={write.target}>{write.target}</Button>
                <Badge variant="outline" className="tag">{write.state}</Badge>
              </li></Item>
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
          <ul className="confined-list">
            {live.quarantine.map((shown, index) => (
              <Item asChild size="sm" key={index}><li className="block rounded-none">
                <div className="origin" title={shown.origin}>
                  {shown.origin}
                </div>
                <div className="detail">
                  {shown.lines} line{shown.lines === 1 ? '' : 's'} · {shown.label}
                </div>
              </li></Item>
            ))}
          </ul>
        )}
      </Section>
      </TabsContent>

      {/* Last, and on purpose. The four panels above are derived from the transcript — what the
          session touched — and this one reads the disk. Putting it under them keeps that boundary
          visible instead of interleaving two kinds of claim.

          No count: the others count something the session did, where this would count entries in
          the top of a folder, which is a number nobody is waiting for.

          Keyed by the handle so switching sessions resets the tree rather than showing one
          project's folders under another's root while the new listing arrives. */}
      <TabsContent value="files" forceMount className={`files-panel ${off.has('files') ? 'off' : ''} data-[state=inactive]:hidden`} id="panel-files" aria-label="Project files">
        <FileTree
          key={live.handle}
          session={live.handle}
          root={live.summary.directory}
          running={live.running}
        />
      </TabsContent>
      </Tabs>
      {audit}
    </aside>
  )
}

function ContextEmpty({ children }: { children: React.ReactNode }): React.JSX.Element {
  return <Empty className="none min-h-0 flex-none items-start gap-0 border-0 p-0 text-left">
    <EmptyDescription className="text-left text-inherit">{children}</EmptyDescription>
  </Empty>
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
    <Collapsible open={open} onOpenChange={setOpen} asChild>
      <section className={`panel ${off ? 'off' : ''}`} id={`panel-${id}`}>
        <CollapsibleTrigger asChild><Button
          variant="ghost"
          className="panel-head"
          // The verb in the title and the name staying put, the rule `ColumnToggle` states.
          title={`${open ? 'Hide' : 'Show'} ${title.toLowerCase()}`}
        >
          <span className={`chevron ${open ? 'open' : ''}`} aria-hidden="true">
            ›
          </span>
          {title}
          {count !== undefined && count > 0 && <Badge variant="secondary" className="count">{count}</Badge>}
        </Button></CollapsibleTrigger>
        <CollapsibleContent forceMount className="panel-inner">
          {children}
        </CollapsibleContent>
      </section>
    </Collapsible>
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
