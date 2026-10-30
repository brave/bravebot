import { memo, useState } from 'react'
import { FileTree } from './FileTree'
import { type PanelName } from '../../shared/state'
import { isConfined, type Activity, type SessionNote, type Waiting, type Shown, type TodoRow } from '../../shared/protocol'
import type { Entry } from '../transcript'
import { Button, Collapse, Icon, Label, ProgressRing, TabItem, Tabs, type IconName } from '../nala'
import { FileGlyph } from './FileGlyph'
import { LabelChip } from './LabelChip'
import { middleTruncate } from '../truncate'

interface Live {
  /** The session's handle. The file tree names it rather than naming a folder. */
  handle: string
  summary: { directory: string }
  entries: Entry[]
  todos: TodoRow[]
  notes?: SessionNote[]
  quarantine: Shown[]
  phase: Waiting | null
  tokens: number
  running: boolean
  /** The conversation's integrity word, where it is known since the last turn began. */
  context?: string | null
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
 * cannot disagree about what happened. The notes are read off the record. The last is the
 * exception and says so: a file tree reads the disk, because the question it answers — what else is
 * in there, and what does this file look like in a real editor — is not one the transcript can be
 * asked.
 */
export const Context = memo(function Context({ live, audit, details, onAddNote }: {
  live: Live | null
  /** Open the dialog that adds a note to this session's record. */
  onAddNote?: () => void
  audit?: React.ReactNode
  /** Drawn in place of the panels, for a bot's own page and its home conversation, which have no project to inspect. */
  details?: React.ReactNode
}): React.JSX.Element {
  const [tab, setTab] = useState<'overview' | 'files'>('overview')
  const off = new Set<PanelName>(tab === 'overview' ? ['files'] : ['plan', 'read', 'writes', 'confined', 'notes'])
  const reveal = (path: string) => {
    const entry = [...(live?.entries ?? [])].reverse().find((entry) =>
      entry.kind === 'tool' ? fileTarget(entry.activity.target) === path : entry.kind === 'confirm' ? entry.request.path === path : false)
    if (entry) document.dispatchEvent(new CustomEvent('bravebot:reveal-entry', { detail: entry.id }))
  }

  // An open audit still takes the column, since that is where it is drawn.
  if (details) return <aside className="context" id="context-column">{audit || details}</aside>
  if (!live) return <aside className={`context ${tab === 'files' ? 'context-files' : ''}`} id="context-column" />

  const files = touched(live.entries)
  const writes = written(live.entries)
  const replayed = live.entries.filter((entry) => entry.kind === 'replayed-tool')
  const notes = live.notes ?? []

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
    notes: 'Notes',
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
        <Tabs className="inspector-tabs" size="small" value={tab} data-test="inspector-tabs"
          onChange={({ value }) => { if (value === 'overview' || value === 'files') setTab(value) }}>
          <TabItem value="overview">Overview</TabItem>
          <TabItem value="files">Files</TabItem>
        </Tabs>
      </div>

      <Section id="plan" title="Plan"count={live.todos.length} off={off.has('plan')}
        about="The steps the agent has set itself, and which it has done.">
        {live.todos.length === 0 ? (
          <Empty icon="list-checks">{onlyReplayed ? 'No plan was recorded.' : 'No plan yet.'}</Empty>
        ) : (
          <ul className="todos">
            {live.todos.map((row, index) => {
              const { rowClass, marker, hiddenSuffix } = todoRowLook(row, live.running)
              return (
                <li key={index} className={rowClass}>
                  <span className="marker" aria-hidden="true">
                    {marker === 'done' ? <Icon name="check-circle-filled" /> : marker === 'cancelled' ? <Icon name="close" /> : marker === 'active' ? <ProgressRing mode="indeterminate" /> : <Icon name="radio-unchecked" />}
                  </span>
                  <span className="todo-text">{row.content}</span>
                  <span className="visually-hidden">{hiddenSuffix}</span>
                </li>
              )
            })}
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
        about={onlyReplayed ? 'Every call the saved turns made, from the record.' : 'Files the agent opened, and whether their contents were confined from the planner.'}
      >
        {onlyReplayed ? (
          <>
            <Empty icon="window-console">
              From the record. It keeps what each turn did, not what came of it, so
              there is nothing to say about where these landed.
            </Empty>
            {/* Every path in this list and the two below it ellipsises, and a path clipped
                on the right loses the filename — the one part of it somebody is reading
                for. The tooltip is the whole string back. It repeats what is already on
                screen when the path is short enough to fit, which is the cheaper of the two
                mistakes available: the alternative is measuring every row on every render
                to decide whether to offer one. */}
            <ul className="files">
              {replayed.map((entry) =>
                entry.kind === 'replayed-tool' ? (
                  <li key={entry.id} className="file-row from-record">
                    <Icon name="window-console" className="row-icon" />
                    <code className="file-path" data-tooltip={entry.text}>{entry.text}</code>
                  </li>
                ) : null,
              )}
            </ul>
          </>
        ) : files.length === 0 ? (
          <Empty icon="folder">Nothing read yet.</Empty>
        ) : (
          <ul className="files">
            {files.map((file) => (
              <li key={file.target} className={`file-row${file.confined ? ' confined' : ''}`}>
                <FileLink path={file.target} shown={file.shown} onReveal={reveal} />
                {file.confined && <Label mode="outline" className="tag" color="blue">confined</Label>}
              </li>
            ))}
          </ul>
        )}
      </Section>

      <Section id="writes" title="Changes" count={writes.length} off={off.has('writes')}
        about="Files the agent asked to write, and how far each write got.">
        {writes.length === 0 ? (
          <Empty icon="edit-box">
            {onlyReplayed
              ? 'Not recorded for past turns.'
              : 'Nothing has been written.'}
          </Empty>
        ) : (
          <ul className="files">
            {writes.map((write) => (
              <li key={write.target} className={`file-row ${write.state}`}>
                <FileLink path={write.target} shown={write.shown} onReveal={reveal} />
                <Label mode="outline" className="tag" color={stateColor(write.state)}>{write.state}</Label>
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
        about="Content held apart from the planner: shown to you, and to no model unless you let it through."
      >
        {typeof live.context === 'string' && <dl className="context-facts" data-test="conversation-integrity">
          <dt>Conversation</dt>
          <dd>{live.context === 'trusted' ? 'Trusted' : 'Has met untrusted content'}</dd>
        </dl>}
        {live.quarantine.length === 0 ? (
          <Empty icon="shield-done">
            {onlyReplayed
              ? 'Not recorded for past turns. Confined content is never written down.'
              : 'Nothing confined.'}
          </Empty>
        ) : (
          <ul className="confined-list">
            {live.quarantine.map((shown, index) => (
              <li key={index} className="confined-row">
                <Icon name="shield-done" className="row-icon" />
                <span className="confined-text">
                  <code className="origin" data-tooltip={shown.origin}>{middleTruncate(shown.origin, PATH_WIDTH)}</code>
                  <span className="detail">{shown.lines} line{shown.lines === 1 ? '' : 's'}<LabelChip label={shown.labelAxes} /></span>
                </span>
              </li>
            ))}
          </ul>
        )}
      </Section>

      {/* Read off the record rather than the transcript, and plain text: the person wrote them,
          and they are never sent to the model. */}
      <Section id="notes" title="Notes" count={notes.length} off={off.has('notes')}
        about="Notes you kept on this chat record. Never sent to the model.">
        {notes.length === 0 ? (
          <Empty icon="note">No notes.</Empty>
        ) : (
          <ul className="notes" data-test="notes">
            {notes.map((note, index) => (
              <li key={index}>
                <span className="note-turn num">Turn {note.turn}</span>
                <span className="note-text">{note.text}</span>
              </li>
            ))}
          </ul>
        )}
        {onAddNote && (
          <Button kind="plain-faint" size="small" className="note-add" isDisabled={live.running} onClick={onAddNote} data-test="notes-add">
            <Icon slot="icon-before" name="plus-add" />
            Add note…
          </Button>
        )}
      </Section>

      {/* Last, and on purpose. The panels above are derived from the session — what it
          touched and what was noted on it — and this one reads the disk. Putting it under them keeps that boundary
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
})

/**
 * How a plan row draws, given whether a turn is running.
 *
 * `done` and `cancelled` are settled and draw the same whether or not a turn is running. `active`
 * only means "the model is working on this right now" while a turn is running: once the turn ends
 * with the row still `active` or `pending`, nothing is working on it, so both draw as left open —
 * the row the planner has not settled, shown with the same hollow mark and dimmed ink as a row
 * that was never started, and no ring that would read as work still going.
 */
export function todoRowLook(
  row: TodoRow,
  running: boolean,
): { rowClass: string; marker: 'done' | 'cancelled' | 'active' | 'open'; hiddenSuffix: string } {
  if (row.status === 'done') return { rowClass: 'done', marker: 'done', hiddenSuffix: ', done' }
  if (row.status === 'cancelled') return { rowClass: 'cancelled', marker: 'cancelled', hiddenSuffix: ', cancelled' }
  if (row.status === 'active' && running) return { rowClass: 'active', marker: 'active', hiddenSuffix: ', in progress' }
  if (running) return { rowClass: 'pending', marker: 'open', hiddenSuffix: '' }
  return { rowClass: 'left-open', marker: 'open', hiddenSuffix: ', left open' }
}

/** What a write's state means at a glance: done, refused or failed, or still moving. */
function stateColor(state: string): 'green' | 'red' | 'yellow' | 'neutral' {
  if (state === 'applied') return 'green'
  if (state === 'refused' || state === 'failed') return 'red'
  if (state === 'waiting' || state === 'approved' || state === 'applying') return 'yellow'
  return 'neutral'
}

/** How many characters of a path fit a row of this column before the middle gives way. */
const PATH_WIDTH = 40

/**
 * A path in one of the lists, which takes the transcript to where it was touched.
 *
 * `path` is what the call named and what the transcript is searched for; `shown` is the same name
 * spelled so a blank or whitespace-ended one is visible (PROMPT-1), and is all that is drawn.
 */
function FileLink({ path, shown, onReveal }: { path: string; shown: string; onReveal: (path: string) => void }): React.JSX.Element {
  const cut = middleTruncate(shown, PATH_WIDTH)
  return (
    <button type="button" className="context-link" onClick={() => onReveal(path)}
      data-tooltip={cut === shown ? `Show ${shown} in the conversation` : shown}>
      <FileGlyph name={path} />
      <span className="file-path">{cut}</span>
    </button>
  )
}

/** One caption line for a panel with nothing in it, under the panel's own icon. */
function Empty({ icon, children }: { icon: IconName; children: React.ReactNode }): React.JSX.Element {
  return <p className="panel-empty"><Icon name={icon} />{children}</p>
}

function Section({
  id,
  title,
  count,
  about,
  off = false,
  children,
}: {
  /** Which panel this is, so the button in the bar can point at it. */
  id: PanelName
  title: string
  /** How many things are in it, for the count in the head. Shown at zero as well: "none" is an answer. */
  count: number
  /** What the panel holds, for the tooltip on its heading. */
  about: string
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
      <Collapse className="flat-collapse panel-collapse" isOpen={open} onToggle={({ open: next }) => setOpen(next)} data-test={`panel-${id}-collapse`}>
        <span slot="title" className="panel-title" data-tooltip={about}>
          {title}
          <Label color="neutral" className="count num">{count}</Label>
          <span className="visually-hidden">{`, ${count} ${count === 1 ? 'item' : 'items'}`}</span>
        </span>
        <div className="panel-inner">{children}</div>
      </Collapse>
    </section>
  )
}

/**
 * Which files the turn opened, and whether the planner was allowed to read them.
 *
 * The distinction is the whole point of the tool, so it is carried into the list rather
 * than flattened into "files". A name here does not mean the model saw the contents.
 */
function touched(entries: Entry[]): { target: string; shown: string; confined: boolean }[] {
  const seen = new Map<string, { shown: string; confined: boolean }>()
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
    seen.set(entry.activity.target, { shown: entry.activity.shownTarget, confined: (seen.get(entry.activity.target)?.confined ?? false) || confined })
  }
  return [...seen].map(([target, { shown, confined }]) => ({ target, shown, confined }))
}

/** The agent decorates reference targets with a slot and label; match their underlying path. */
function fileTarget(target: string): string {
  return target.replace(/^ref:\d+\([TU],(?:pub|priv)\):/, '').replace(/^\.\//, '')
}

/** One file the turn wrote, and how far that write got. */
interface Write {
  target: string
  /** `target` as the bridge spelled it, so a blank or whitespace-ended name is visible (PROMPT-1). */
  shown: string
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
        shown: entry.request.shownPath,
        state:
          entry.interrupted ? 'cancelled' : entry.decision === 'approve'
            ? call ? outcome(call) : 'approved'
            : entry.decision === 'reject'
              ? 'refused'
              : 'waiting',
      })
      continue
    }
    if (entry.kind !== 'tool' || !isWrite(entry.activity) || !entry.activity.target) continue
    const target = fileTarget(entry.activity.target)
    calls.set(target, entry.activity)
    // An outcome already recorded for this path — a decision, or the finish of the same
    // call — outranks a line that has not finished, so a pending row cannot overwrite it.
    rows.set(target, { target, shown: entry.activity.shownFile, state: outcome(entry.activity) })
  }
  return [...rows.values()]
}

/**
 * How far a write call got. One still running has not changed anything yet, and a refused or
 * failed one never will, so neither reads as "applied".
 *
 * The agent marks a refusal and a failure alike as `failed`. What tells them apart is the note:
 * every refusal the agent writes, whether plan mode's, a deny rule's or a person's, opens with
 * "refused:", a literal in its own dispatch placed ahead of anything a path or a reason fills
 * in, so the word is the agent's and not a file's or the model's.
 */
function outcome(call: Activity): Write['state'] {
  if (call.failed) return call.note?.startsWith('refused:') ? 'refused' : 'failed'
  return call.note === null ? 'applying' : 'applied'
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
