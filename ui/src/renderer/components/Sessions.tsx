import { SidebarTools } from './SidebarTools'
import { createContext, useContext, useCallback, useMemo, useRef, useState } from 'react'
import type { SessionSummary } from '../../shared/protocol'
import type { ContextTarget } from '../../shared/commands'
import { keyOf } from '../../shared/forks'
import { Fold } from './Fold'
import { ForkIcon } from './ForkIcon'
import { PopMenu, type PopItem } from './PopMenu'
import { conversationKey } from '../../shared/experience'
import { useExperience, setConversation } from '../experience'
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
      <header className="sessions-head">
        <SidebarTools query={query} onQuery={setQuery} label="Filter sessions" action={<NewSession onNew={onNew} />}>
          {/* The label stays put and `aria-pressed` carries the state, with the verb in the
              tooltip — the same disclosure discipline the column folds follow. A control
              that renamed itself would be one the reader has to re-find after every press. */}
          <button
            className="session-group"
            aria-pressed={grouped}
            aria-label="Group by project"
            title={grouped ? 'Show one flat list' : 'Group by project'}
            onClick={() => onGroup(!grouped)}
          >
            <span aria-hidden="true">▤</span>
          </button>
        </SidebarTools>
        <div className="session-scope"><button aria-pressed={!archive} onClick={() => setArchive(false)}>Conversations</button><button aria-pressed={archive} onClick={() => setArchive(true)}>Archived</button></div>
      </header>

      <div className="session-list">
        {sessions.length === 0 && (
          <p className="empty">
            No sessions yet. Open a project to begin — or start one in a terminal with{' '}
            <code>bravebot</code> and it will appear here.
          </p>
        )}
        {/* Said separately, because the message above is a fact about the machine and would
            be a lie about a list that is merely filtered down to nothing. */}
        {sessions.length > 0 && shown.length === 0 && (
          <p className="empty">{query.trim() ? `No conversation matches “${query}”.` : archive ? 'No archived conversations.' : 'No active conversations. Start a new session or restore one from Archived.'}</p>
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
 * The rows stay mounted while shut, because that is how [`Fold`] has something to animate
 * away from; it hides them from the reader and from the tab order in CSS once the collapse
 * has finished.
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
    <section className="session-group-section">
      {/* The full path in the tooltip, because two checkouts of one project share a basename
          and picking the wrong one is a mistake nothing later announces — the same trap the
          recents menu guards against. On both buttons: the one that starts a session here is
          exactly where that mistake would cost something. */}
      <div className="session-group-head">
        <button
          className="session-group-fold"
          aria-expanded={open}
          title={group.directory}
          onClick={() => onToggle(group.directory)}
        >
          <span className={`chevron ${open ? 'open' : ''}`} aria-hidden="true">
            ›
          </span>
          <span className="session-group-name">{group.project}</span>
          <span className="count">{group.sessions.length}</span>
        </button>
        {/* The same thing **New session** does, minus the folder picker — the directory is
            already known, and the picker's whole job is to find one out. Named for the
            project rather than "New session" so that a reader of the button list is told
            which of a dozen identical-looking pluses they have landed on. */}
        <button
          className="session-group-new"
          aria-label={`New session in ${group.project}`}
          title={`New session in ${group.directory}`}
          onClick={() => onNew(group.directory)}
        >
          <span aria-hidden="true">+</span>
        </button>
      </div>
      <Fold open={open}>
        {group.sessions.map((session) => (
          <Session
            key={`${session.directory}/${session.id}`}
            session={session}
            openId={openId}
            forked={forked.has(keyOf(session.directory, session.id))}
            onOpen={onOpen}
          />
        ))}
      </Fold>
    </section>
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
  const anchor = useRef<HTMLButtonElement>(null)
  return <div className={`session-row ${session.id === openId ? 'current' : ''}`}>
    <button className={`session ${session.id === openId ? 'current' : ''}`} onClick={() => onOpen(session)} onContextMenu={contextMenu('session', session.id)}>
      <span className="session-title" title={session.title}>
        {preferences?.pinned && <svg className="session-pin" width="13" height="15" viewBox="0 0 16 18" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" role="img" aria-label="Pinned" focusable="false">
          <path d="M5 2h6M6 2v6l-3 4h10l-3-4V2M8 12v4" />
        </svg>}
        {forked && <span className="fork-mark"><ForkIcon size={11} /></span>}{session.title}
      </span>
      <span className="session-where">{session.project}{session.branch && <span className="branch"> · {session.branch}</span>} · {ago(session.updated)}</span>
      {(info?.bot || info?.state) && <span className="session-badges">{info.bot && <span>{info.bot}</span>}{info.state && <span className={`session-state ${info.state.toLowerCase().replaceAll(' ', '-')}`}>{info.state}</span>}</span>}
    </button>
    <button ref={anchor} className="session-more" aria-label={`Actions for ${session.title}`} aria-haspopup="menu" aria-expanded={menu} onClick={() => setMenu(!menu)}>⋯</button>
    <PopMenu open={menu} anchor={anchor} label="Conversation actions" onClose={() => setMenu(false)}
      items={[{ id: 'pin', label: preferences?.pinned ? 'Unpin conversation' : 'Pin conversation' }, { id: 'archive', label: preferences?.archived ? 'Restore conversation' : 'Archive conversation' }]}
      onChoose={(id) => setConversation(key, id === 'pin' ? { pinned: !preferences?.pinned } : { archived: !preferences?.archived })} />
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
  const chevron = useRef<HTMLButtonElement>(null)

  // Read when the menu is opened rather than held and kept in step: the list changes in the
  // main process, and a copy up here would be one more thing that can be stale.
  const show = useCallback(() => {
    void window.bravebot.readRecents().then((found) => {
      setDirectories(found)
      setOpen(true)
    })
  }, [])

  const items: PopItem[] = directories.length
    ? directories.map((directory) => ({
        id: directory,
        label: directory.split('/').pop() || directory,
        // Two checkouts of one project share a basename, and picking the wrong one is a
        // mistake nothing later would announce.
        detail: directory,
      }))
    : [{ id: 'none', label: 'No projects opened yet', enabled: false }]

  return (
    <div className="new-split">
      <button className="new" onClick={() => onNew()} title="Open a project">
        <span className="plus">+</span> New session
      </button>
      <button
        ref={chevron}
        className="new-recent"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label="Projects opened before"
        title="Projects opened before"
        onClick={() => (open ? setOpen(false) : show())}
      >
        <span aria-hidden="true">⌄</span>
      </button>
      <PopMenu
        open={open}
        anchor={chevron}
        items={items}
        label="Projects opened before"
        onChoose={(id) => onNew(id)}
        onClose={() => setOpen(false)}
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
