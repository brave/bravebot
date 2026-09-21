import { useState } from 'react'
import { Fold } from './Fold'
import { FileTree } from './FileTree'
import { type PanelName } from '../../shared/state'
import type { Activity, Phase, Shown, TodoRow } from '../../shared/protocol'
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
      <div className="context-content" hidden={!!audit}>
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
        <div className="inspector-title"><strong>Project context</strong><button className="drawer-close" onClick={onClose} aria-label="Close context panel">×</button></div>
        <div className="inspector-tabs" role="tablist" aria-label="Project context">
          {(['overview', 'files'] as const).map((name) => <button key={name} role="tab" aria-selected={tab === name} onClick={() => setTab(name)}>{name === 'overview' ? 'Overview' : 'Files'}</button>)}
        </div>

      </div>

      <Section id="plan" title="Plan" count={live.todos.length} off={off.has('plan')}>
        {live.todos.length === 0 ? (
          <p className="none">{onlyReplayed ? 'No plan was recorded.' : 'No plan yet.'}</p>
        ) : (
          <ul className="todos">
            {live.todos.map((row, index) => (
              <li key={index} className={row.status}>
                <span className="marker">
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
            <p className="none">
              From the record. It keeps what each turn did, not what came of it, so
              there is nothing to say about where these landed.
            </p>
            {/* Every path in this list and the two below it ellipsises, and a path clipped
                on the right loses the filename — the one part of it somebody is reading
                for. The tooltip is the whole string back. It repeats what is already on
                screen when the path is short enough to fit, which is the cheaper of the two
                mistakes available: the alternative is measuring every row on every render
                to decide whether to offer one. */}
            <ul className="files">
              {replayed.map((entry) =>
                entry.kind === 'replayed-tool' ? (
                  <li key={entry.id} className="from-record">
                    <code title={entry.text}>{entry.text}</code>
                  </li>
                ) : null,
              )}
            </ul>
          </>
        ) : files.length === 0 ? (
          <p className="none">Nothing read yet.</p>
        ) : (
          <ul className="files">
            {files.map((file) => (
              <li key={file.target} className={file.confined ? 'confined' : ''}>
                <button className="context-link" onClick={() => reveal(file.target)} title={file.target}>{file.target}</button>
                {file.confined && <span className="tag">confined</span>}
              </li>
            ))}
          </ul>
        )}
      </Section>

      <Section id="writes" title="Changes" count={writes.length} off={off.has('writes')}>
        {writes.length === 0 ? (
          <p className="none">
            {onlyReplayed
              ? 'Not recorded for past turns.'
              : 'Nothing has been written.'}
          </p>
        ) : (
          <ul className="files">
            {writes.map((write) => (
              <li key={write.target} className={write.state}>
                <button className="context-link" onClick={() => reveal(write.target)} title={write.target}>{write.target}</button>
                <span className="tag">{write.state}</span>
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
          <p className="none">
            {onlyReplayed
              ? 'Not recorded for past turns. Confined content is never written down.'
              : 'Nothing confined.'}
          </p>
        ) : (
          <ul className="confined-list">
            {live.quarantine.map((shown, index) => (
              <li key={index}>
                <div className="origin" title={shown.origin}>
                  {shown.origin}
                </div>
                <div className="detail">
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
      <section className={`files-panel ${off.has('files') ? 'off' : ''}`} id="panel-files" aria-label="Project files">
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
    <section className={`panel ${off ? 'off' : ''}`} id={`panel-${id}`}>
      <button
        className="panel-head"
        aria-expanded={open}
        // The verb in the title and the name staying put, the rule `ColumnToggle` states.
        title={`${open ? 'Hide' : 'Show'} ${title.toLowerCase()}`}
        onClick={() => setOpen(!open)}
      >
        <span className={`chevron ${open ? 'open' : ''}`} aria-hidden="true">
          ›
        </span>
        {title}
        {count !== undefined && count > 0 && <span className="count">{count}</span>}
      </button>
      <Fold open={open} className="panel-inner">
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
    const confined = entry.landing !== null && entry.landing !== 'context'
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
