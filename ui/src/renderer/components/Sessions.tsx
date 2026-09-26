import { SidebarTools } from './SidebarTools'
import { createContext, useContext, useCallback, useMemo, useState } from 'react'
import { LayoutGridIcon, PlusIcon } from 'lucide-react'
import type { SessionSummary } from '../../shared/protocol'
import type { ContextTarget } from '../../shared/commands'
import { keyOf } from '../../shared/forks'
import { projectLabel } from '../../shared/recents'
import { ForkIcon } from './ForkIcon'
import { PopMenu, type PopItem } from './PopMenu'
import { conversationKey } from '../../shared/experience'
import { useExperience, setConversation } from '../experience'
import { Button } from '@/components/ui/button'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import { Toggle } from '@/components/ui/toggle'
import { cn } from '@/lib/utils'
export const SessionInfo = createContext<Record<string, { bot?: string; state?: string }>>({})

interface Props {
  sessions: SessionSummary[]
  openId: string | undefined
  /** Which sessions came out of another one, by `directory/id`. */
  forked: ReadonlySet<string>
  onOpen: (summary: SessionSummary) => void
  onNew: (directory?: string) => void
  /**
   * How the list is arranged, and how to say it changed.
   *
   * Held by [`Sidebar`] rather than here, which is a departure from the note on `query` below and
   * for the reason that note itself gives: this half is written to disk, and the column now has
   * two lists sharing one file. One owner of what is remembered means one write, rather than two
   * components racing to describe the same preference.
   */
  grouped: boolean
  onGroup: (grouped: boolean) => void
  collapsed: ReadonlySet<string>
  onCollapse: (collapsed: ReadonlySet<string>) => void
}

/**
 * Ask the main process for the menu that belongs to a thing here.
 *
 * Only the kind and the id travel. What the menu says is decided over there, from labels
 * compiled into it, so nothing on screen can put a word into a native menu.
 */
function contextMenu(target: ContextTarget, id: string) {
  return (event: React.MouseEvent): void => {
    event.preventDefault()
    window.bravebot.popupContext({ target, id })
  }
}

/**
 * The left-hand column: one list across every project, newest first.
 *
 * Flat by default, because this is a chat list and a chat list has one column. The project
 * is the secondary line, the way a group chat names itself under the message. But a flat
 * list cannot answer "what have I been doing in *this* checkout" without typing the project
 * name, so the toggle beside the filter box gathers the same rows under headings instead.
 *
 * Both are rendering decisions over what is already here rather than questions for the
 * bridge: `session.list` hands over every session at once, with the directory on each, so a
 * round trip would only make this slower and able to fail.
 */
export { contextMenu }

export function Sessions({
  sessions,
  openId,
  forked,
  onOpen,
  onNew,
  grouped,
  onGroup,
  collapsed,
  onCollapse,
}: Props): React.JSX.Element {
  // Local rather than lifted into `App`. The convention there is that state lives in `App`,
  // but the reason given for the composer's draft is that the menu has to read it; nothing
  // outside this column reads the query, and — more to the point — `App` looks a right-
  // clicked session up in `sessions` by id. Filtering a copy it holds would make a menu item
  // fail on a row that is hidden a moment later.
  const [query, setQuery] = useState('')
  const preferences = useExperience()
  const [archive, setArchive] = useState(false)
  const shown = useMemo(() => matching(sessions, query).filter((session) =>
    !!preferences.conversations[conversationKey(session.directory, session.id)]?.archived === archive
  ).sort((a, b) => Number(!!preferences.conversations[conversationKey(b.directory, b.id)]?.pinned) - Number(!!preferences.conversations[conversationKey(a.directory, a.id)]?.pinned)), [sessions, query, preferences, archive])

  // Unlike the query, this is remembered between launches: which way somebody likes their
  // list is not a per-run thought. The stored value arrives asynchronously and so cannot
  // seed `useState` — the same dance `columns.ts` documents — which is why the column
  // renders flat for a frame before adopting it. `ready` keeps that first frame from
  // writing the default back over what is on disk.
  const toggleGroup = useCallback(
    (directory: string) => {
      const next = new Set(collapsed)
      if (!next.delete(directory)) next.add(directory)
      onCollapse(next)
    },
    [collapsed, onCollapse],
  )

  const groups = useMemo(() => (grouped ? grouping(shown) : []), [grouped, shown])

  // A live query opens every group for as long as it runs. A person who typed something and
  // got a heading with nothing under it has been shown the opposite of what they asked for,
  // and quietly reopening beats making them undo a fold they set days ago — which is why
  // this reads through `collapsed` rather than clearing it.
  const searching = query.trim().length > 0

  return (
    <>
      {/* Every control in the header has to opt out of native window dragging, or the pointer
          moves the window instead of pressing the thing under it. */}
      <header className="sessions-head flex-none px-3 pt-1.5 pb-2 [-webkit-app-region:drag] [&_:is(a,button,input,select,textarea)]:[-webkit-app-region:no-drag]">
        <SidebarTools query={query} onQuery={setQuery} label="Filter sessions" action={<NewSession onNew={onNew} />}>
          {/* The label stays put and `aria-pressed` carries the state, with the verb in the
              tooltip — the same disclosure discipline the column folds follow. A control
              that renamed itself would be one the reader has to re-find after every press. */}
          <Toggle
            className="session-group grid size-auto min-h-8 w-7.5 flex-none place-items-center rounded-md border-0 bg-transparent text-muted-foreground hover:bg-bubble-agent hover:text-foreground aria-pressed:bg-bubble-agent aria-pressed:text-primary"
            pressed={grouped}
            aria-label="Group by project"
            title={grouped ? 'Show one flat list' : 'Group by project'}
            onPressedChange={onGroup}
          >
            <LayoutGridIcon />
          </Toggle>
        </SidebarTools>
        <div className="session-scope mt-2.5 flex gap-1">
          <Button className="flex-1" variant={!archive ? 'secondary' : 'ghost'} size="sm" aria-pressed={!archive} onClick={() => setArchive(false)}>Conversations</Button>
          <Button className="flex-1" variant={archive ? 'secondary' : 'ghost'} size="sm" aria-pressed={archive} onClick={() => setArchive(true)}>Archived</Button>
        </div>
      </header>

      <div className="session-list min-h-0 flex-1 overflow-y-auto px-2 pt-1 pb-3">
        {sessions.length === 0 && (
          <p className="empty px-1.5 py-2 text-xs text-muted-foreground/70 [&_code]:font-mono [&_code]:text-[11px]">
            No sessions yet. Open a project to begin — or start one in a terminal with{' '}
            <code>bravebot</code> and it will appear here.
          </p>
        )}
        {/* Said separately, because the message above is a fact about the machine and would
            be a lie about a list that is merely filtered down to nothing. */}
        {sessions.length > 0 && shown.length === 0 && (
          <p className="empty px-1.5 py-2 text-xs text-muted-foreground/70">{query.trim() ? `No conversation matches “${query}”.` : archive ? 'No archived conversations.' : 'No active conversations. Start a new session or restore one from Archived.'}</p>
        )}
        {!grouped &&
          shown.map((session) => (
            <Session
              key={`${session.directory}/${session.id}`}
              session={session}
              openId={openId}
              forked={forked.has(keyOf(session.directory, session.id))}
              onOpen={onOpen}
            />
          ))}
        {grouped &&
          groups.map((group) => (
            <Group
              key={group.directory}
              group={group}
              open={searching || !collapsed.has(group.directory)}
              onToggle={toggleGroup}
              openId={openId}
              forked={forked}
              onOpen={onOpen}
              onNew={onNew}
            />
          ))}
      </div>
    </>
  )
}

/**
 * One checkout's sessions, under a heading that opens and shuts them.
 *
 * Most of the heading is the disclosure control rather than a chevron beside it: the name is
 * the biggest thing in reach, and a group that can be folded should not ask for a 10px arrow
 * to be hit. `aria-expanded` carries the state and the name stays put — the disclosure
 * discipline the column folds and the context panels already follow.
 *
 * The heading is a row of two buttons rather than one, because the second one starts a
 * session here and a button cannot be nested inside a button. That is also why the fold is
 * the *inner* control: making the row itself clickable and the plus a child would have been
 * the nesting problem wearing a different hat.
 *
 * Rows stay mounted while shut (`keepMounted`) so collapsing does not throw away the list.
 */
function Group({
  group,
  open,
  onToggle,
  openId,
  forked,
  onOpen,
  onNew,
}: {
  group: Group
  open: boolean
  onToggle: (directory: string) => void
  openId: string | undefined
  forked: ReadonlySet<string>
  onOpen: (summary: SessionSummary) => void
  onNew: (directory: string) => void
}): React.JSX.Element {
  return (
    <Collapsible
      // Air between one checkout's group and the next, set on the second of a pair rather than on
      // every group, so the first is not pushed off the top of the list.
      className="session-group-section [.session-group-section+&]:mt-2"
      open={open}
      onOpenChange={(next) => {
        if (next !== open) onToggle(group.directory)
      }}
    >
      {/* The full path in the tooltip, because two checkouts of one project share a basename
          and picking the wrong one is a mistake nothing later announces — the same trap the
          recents menu guards against. On both buttons: the one that starts a session here is
          exactly where that mistake would cost something. */}
      {/* Sticky, because the name of the checkout is the one thing worth keeping on screen while
          its sessions scroll past — and `--sidebar` is translucent, so a sticky heading wearing it
          alone would let the rows scroll visibly through. The background-image reproduces exactly
          what the column already shows, over an opaque background-color: the same two layers, but
          something a row cannot be seen through. Hover paints a third layer over that rather than
          replacing it, so the ground does not drop out from under the pointer. */}
      <div className="session-group-head sticky top-0 z-1 flex w-full items-stretch bg-background [background-image:linear-gradient(var(--sidebar),var(--sidebar))] hover:[background-image:linear-gradient(color-mix(in_srgb,var(--foreground)_8%,transparent),color-mix(in_srgb,var(--foreground)_8%,transparent)),linear-gradient(var(--sidebar),var(--sidebar))]">
        {/* The fold takes the whole row bar the plus, not just the chevron: the name is the biggest
            thing in reach, and asking for a 10px arrow instead would make folding the fiddliest
            gesture in the column.

            Louder than the muted ink the context panels' headings use, because these carry more
            weight — those name a section of one session, these name the checkout every row beneath
            them belongs to. The accent proper would be too much: full primary is what a pressed
            control and the open row wear, and a dozen headings in it would drown both. */}
        <CollapsibleTrigger
          className="session-group-fold flex min-w-0 flex-1 items-center gap-1.5 border-0 bg-transparent pt-2 pr-1 pb-1.5 pl-2.5 text-left text-[11px] font-semibold tracking-[0.05em] text-primary/75 uppercase focus-visible:outline focus-visible:outline-primary focus-visible:-outline-offset-2"
          title={group.directory}
        >
          <span
            className={cn(
              'chevron inline-block flex-none text-muted-foreground/70 transition-transform duration-[180ms] ease-[cubic-bezier(0.32,0.72,0,1)] motion-reduce:transition-none',
              open && 'open rotate-90',
            )}
            aria-hidden="true"
          >
            ›
          </span>
          {/* The column narrows to 200px, which is not enough for every checkout's name. */}
          <span className="session-group-name truncate">{group.project}</span>
          <span className="count ml-auto flex-none rounded-lg bg-foreground/15 px-1.5 py-px text-[10px]">{group.sessions.length}</span>
        </CollapsibleTrigger>
        {/* The same thing **New session** does, minus the folder picker — the directory is
            already known, and the picker's whole job is to find one out. Named for the
            project rather than "New session" so that a reader of the button list is told
            which of a dozen identical-looking pluses they have landed on. */}
        {/* Quiet by default — a dozen of these at full strength would compete with the names they
            sit beside — and always present rather than revealed on hover: a shortcut nobody can
            see is not much of a shortcut. */}
        <Button
          variant="ghost"
          size="icon-sm"
          className="session-group-new size-auto flex-none rounded-none bg-transparent pt-2 pr-2.5 pb-1.5 pl-1 leading-none text-muted-foreground/70 hover:bg-transparent hover:text-primary"
          aria-label={`New session in ${group.project}`}
          title={`New session in ${group.directory}`}
          onClick={() => onNew(group.directory)}
        >
          <PlusIcon />
        </Button>
      </div>
      <CollapsibleContent keepMounted>
        {group.sessions.map((session) => (
          <Session
            key={`${session.directory}/${session.id}`}
            session={session}
            openId={openId}
            forked={forked.has(keyOf(session.directory, session.id))}
            onOpen={onOpen}
          />
        ))}
      </CollapsibleContent>
    </Collapsible>
  )
}

/**
 * One row, whichever arrangement it is standing in.
 *
 * The same component under a heading as in the flat list, so the two paths cannot drift into
 * showing different things about a session. The project stays on the row even when the
 * heading above already says it: the row is what a person reads, and a row that means
 * something different depending on how far up the list they last looked is worse than a
 * word repeated.
 */
function Session({
  session,
  openId,
  forked,
  onOpen,
}: {
  session: SessionSummary
  openId: string | undefined
  forked: boolean
  onOpen: (summary: SessionSummary) => void
}): React.JSX.Element {
  const key = conversationKey(session.directory, session.id)
  const preferences = useExperience().conversations[key]
  const info = useContext(SessionInfo)[key]
  const [menu, setMenu] = useState(false)
  const current = session.id === openId
  const state = info?.state
  return <div className={cn('session-row relative flex items-center rounded-[9px]', current && 'current bg-bubble-user text-bubble-user-foreground')}>
    <Button
      variant="ghost"
      className={cn(
        // A row is three stacked lines, not a label, so the button has to grow rather than
        // hold the default control height. The right-hand padding is the room the actions
        // button below sits in.
        'session mb-px h-auto min-w-0 flex-1 flex-col items-start gap-0.5 rounded-lg bg-transparent px-2.5 py-2 pr-8 text-left font-normal hover:bg-foreground/5',
        '[.app.comfortable_&]:py-3',
        current && 'current bg-transparent text-inherit hover:bg-foreground/10',
      )}
      onClick={() => onOpen(session)}
      onContextMenu={contextMenu('session', session.id)}
    >
      <span className="session-title w-full truncate font-medium" title={session.title}>
        {preferences?.pinned && <svg className="session-pin mr-1.5 inline-block align-[-2px]" width="13" height="15" viewBox="0 0 16 18" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" role="img" aria-label="Pinned" focusable="false">
          <path d="M5 2h6M6 2v6l-3 4h10l-3-4V2M8 12v4" />
        </svg>}
        {forked && (
          <span className="fork-mark mr-1.5 text-primary/75 [&_svg]:align-[-1px]">
            <ForkIcon size={11} />
            <span className="offscreen sr-only">Forked.</span>
          </span>
        )}{session.title}
      </span>
      <span className={cn('session-where w-full truncate text-xs font-normal', current ? 'opacity-85' : 'text-muted-foreground/70')}>{session.project}{session.branch && <span className="branch"> · {session.branch}</span>} · {ago(session.updated)}</span>
      {(info?.bot || state) && <span className="session-badges mt-1.5 flex flex-wrap gap-1 text-[10px] font-normal">
        {info.bot && <span className="rounded bg-bubble-agent px-1.5 py-px text-muted-foreground">{info.bot}</span>}
        {state && <span className={cn(
          `session-state ${state.toLowerCase().replaceAll(' ', '-')}`,
          'rounded px-1.5 py-px',
          state === 'Working' || state === 'Needs approval' || state === 'Needs answer'
            ? 'bg-warning/10 text-warning'
            : state === 'Failed'
              ? 'bg-destructive/10 text-destructive'
              : state === 'Completed'
                ? 'bg-bubble-agent text-success'
                : 'bg-bubble-agent text-muted-foreground',
        )}>{state}</span>}
      </span>}
    </Button>
    <PopMenu
      open={menu}
      onOpenChange={setMenu}
      label="Conversation actions"
      items={[{ id: 'pin', label: preferences?.pinned ? 'Unpin conversation' : 'Pin conversation' }, { id: 'archive', label: preferences?.archived ? 'Restore conversation' : 'Archive conversation' }]}
      onChoose={(id) => setConversation(key, id === 'pin' ? { pinned: !preferences?.pinned } : { archived: !preferences?.archived })}
      trigger={<Button variant="ghost" size="icon-sm" className="session-more absolute top-2.5 right-1 rounded-md bg-transparent text-muted-foreground" aria-label={`Actions for ${session.title}`}>⋯</Button>}
    />
  </div>

}

/**
 * The button that starts a session, and the list of places to start one in.
 *
 * A split control: the button itself does exactly what it always did — opens the folder
 * picker — and the chevron beside it offers the projects opened before. Anything else would
 * have made the common case slower to reach in order to make the second case possible.
 */
function NewSession({ onNew }: { onNew: (directory?: string) => void }): React.JSX.Element {
  const [open, setOpen] = useState(false)
  const [directories, setDirectories] = useState<string[]>([])

  // Read when the menu is opened rather than held and kept in step: the list changes in the
  // main process, and a copy up here would be one more thing that can be stale.
  const onOpenChange = useCallback((next: boolean) => {
    if (!next) {
      setOpen(false)
      return
    }
    void window.bravebot.readRecents().then((found) => {
      setDirectories(found)
      setOpen(true)
    })
  }, [])

  const items: PopItem[] = directories.length
    ? directories.map((directory) => ({
        id: directory,
        label: projectLabel(directory),
        // Two checkouts of one project share a basename, and picking the wrong one is a
        // mistake nothing later would announce.
        detail: directory,
      }))
    : [{ id: 'none', label: 'No projects opened yet', enabled: false }]

  return (
    <div className="new-split flex w-full items-stretch gap-1">
      {/* Bordered rather than filled, and the border is transparent inside the header's row of
          tools: the column already has one accent-coloured thing in it — whichever row is open —
          and a second would leave the pair with no primary action between them. */}
      <Button
        className="new h-auto min-w-0 flex-1 justify-start rounded-lg border border-transparent bg-transparent px-2.5 py-1.5 text-left font-normal text-foreground hover:bg-foreground/10"
        onClick={() => onNew()}
        title="Open a project"
      >
        <span className="plus mr-1 font-semibold text-primary">+</span> New session
      </Button>
      <PopMenu
        open={open}
        onOpenChange={onOpenChange}
        items={items}
        label="Projects opened before"
        onChoose={(id) => onNew(id)}
        trigger={
          <Button
            variant="outline"
            size="icon-sm"
            className="new-recent h-auto w-6.5 flex-none rounded-lg border-0 bg-transparent p-0 text-[11px] text-muted-foreground hover:text-foreground"
            aria-label="Projects opened before"
            title="Projects opened before"
          >
            <span aria-hidden="true">⌄</span>
          </Button>
        }
      />
    </div>
  )
}

/**
 * The sessions a typed query leaves standing.
 *
 * Matched against exactly what a row shows — title, project, branch — so a person can always
 * see why something is in the list. The directory is deliberately not in the haystack even
 * though every session carries one: `project` is its last segment, and searching the whole
 * path would mean every checkout under `~/repos` answered to "repos".
 *
 * Every whitespace-separated term has to appear somewhere, in any order. Plain substrings
 * rather than a fuzzy score: a fuzzy match on a list this short mostly buys the right to
 * return rows the reader cannot account for.
 */
function matching(sessions: SessionSummary[], query: string): SessionSummary[] {
  const terms = query.toLowerCase().split(/\s+/).filter(Boolean)
  if (terms.length === 0) return sessions
  return sessions.filter((session) => {
    const haystack = `${session.title} ${session.project} ${session.branch ?? ''}`.toLowerCase()
    return terms.every((term) => haystack.includes(term))
  })
}

interface Group {
  directory: string
  project: string
  sessions: SessionSummary[]
}

/**
 * The same sessions, gathered under the checkout each was started in.
 *
 * Keyed on `directory` rather than `project`, because two checkouts of one repository share
 * a basename and folding them together would put work from one under a heading that means
 * the other.
 *
 * Nothing is sorted. The list arrives newest-first across every project, so one pass in
 * order leaves the rows in each group newest-first and the groups themselves in the order
 * their newest session appeared — which is the ordering we want, arrived at by not
 * disturbing the one we were given. Imposing it separately would be a second opinion about
 * recency, free to disagree with the bridge's.
 *
 * Grouping happens after filtering, so a group whose every row was filtered away has no
 * heading left behind to say otherwise.
 */
function grouping(sessions: SessionSummary[]): Group[] {
  const groups = new Map<string, Group>()
  for (const session of sessions) {
    const group = groups.get(session.directory)
    if (group) group.sessions.push(session)
    else {
      groups.set(session.directory, {
        directory: session.directory,
        project: session.project,
        sessions: [session],
      })
    }
  }
  return [...groups.values()]
}

/**
 * How long ago, in the words a person says it in.
 *
 * Deliberately the same thresholds as the agent's own `how_long_ago`, so a session does
 * not read as "2 hours ago" here and "1 hour ago" in the terminal.
 */
function ago(then: number): string {
  const seconds = Math.max(0, Math.floor(Date.now() / 1000) - then)
  if (seconds < 60) return 'just now'
  const [count, unit] =
    seconds < 3600
      ? [Math.floor(seconds / 60), 'minute']
      : seconds < 86400
        ? [Math.floor(seconds / 3600), 'hour']
        : seconds < 2592000
          ? [Math.floor(seconds / 86400), 'day']
          : [Math.floor(seconds / 2592000), 'month']
  return `${count} ${unit}${count === 1 ? '' : 's'} ago`
}
