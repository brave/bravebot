import { SidebarRow, SidebarSearch } from './SidebarTools'
import { shortAgo } from '../time'
import { createContext, memo, useContext, useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { flushSync } from 'react-dom'
import type { SessionSummary } from '../../shared/protocol'
import type { ContextTarget } from '../../shared/commands'
import { keyOf } from '../../shared/forks'
import { projectLabel } from '../../shared/recents'
import { Fold } from './Fold'
import { ForkIcon } from './ForkIcon'
import { BotFace } from './BotAvatar'
import { IconButton } from './IconButton'
import { IconMenu } from './IconMenu'
import { conversationKey } from '../../shared/experience'
import { useConversationPreferences, useExperienceValue, setConversation } from '../experience'
import { ButtonMenu, Icon, Menu, ProgressRing } from '../nala'

/** What a row says about a session that is open somewhere: only what asks something of the reader. */
export type SessionStatus = 'working' | 'answer' | 'approval' | 'failed'

export interface SessionInfoValue {
  bot?: { name: string; avatar: string }
  status?: SessionStatus
}
export const SessionInfo = createContext<Record<string, SessionInfoValue>>({})

const STATUS_WORDS: Record<SessionStatus, string> = {
  working: 'Working',
  answer: 'Waiting for your answer',
  approval: 'Waiting for your approval',
  failed: 'The last turn failed',
}

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
   * Held by [`Sidebar`] rather than here: this half is written to disk, and the column has two
   * lists sharing one file. One owner of what is remembered means one write, rather than two
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
 * Which conversations are pinned or archived, as one string, so the list re-renders when one of
 * those flags changes and not when a draft somewhere gains a letter.
 */
const flagsOf = (conversations: Record<string, { pinned?: boolean; archived?: boolean }>): string =>
  Object.entries(conversations)
    .filter(([, preference]) => preference.pinned || preference.archived)
    .map(([key, preference]) => `${key}\u0000${preference.pinned ? 'p' : ''}${preference.archived ? 'a' : ''}`)
    .join('\n')

/**
 * How many rows a list draws before it asks to draw more. Each row mounts Leo elements, and a
 * store of a thousand sessions drawn at once blocks the window for longer than a frame budget
 * allows. The search still runs over every session; this only limits what is drawn of the result.
 */
const PAGE = 100

interface Page {
  rows: SessionSummary[]
  /** Rows of the list left undrawn. */
  hidden: number
  /** How many of those the next page draws. */
  coming: number
  /** The first row the next page draws, which takes focus when it does. */
  first?: SessionSummary
}

/**
 * The first `shown` rows of a list, and past them, in place, every row `keep` asks for: the
 * open conversation and any asking something of the reader stay drawn wherever they fall.
 */
function page(list: SessionSummary[], shown: number, keep: (session: SessionSummary) => boolean): Page {
  if (list.length <= shown) return { rows: list, hidden: 0, coming: 0 }
  const rows = list.filter((session, index) => index < shown || keep(session))
  const coming = list.slice(shown, shown + PAGE).filter((session) => !keep(session))
  return { rows, hidden: list.length - rows.length, coming: coming.length, first: coming[0] }
}

/**
 * The left-hand column: one list across every project, newest first.
 *
 * Flat by default, because this is a chat list and a chat list has one column. The project
 * is the secondary line, the way a group chat names itself under the message. But a flat
 * list cannot answer "what have I been doing in *this* checkout" without typing the project
 * name, so View options can gather the same rows under headings instead.
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
  // Local rather than lifted into `App`: nothing outside this column reads the query, and `App`
  // looks a right-clicked session up in `sessions` by id. Filtering a copy it holds would make a
  // menu item fail on a row that is hidden a moment later.
  const [query, setQuery] = useState('')
  const flags = useExperienceValue((experience) => flagsOf(experience.conversations))
  const [showArchived, setShowArchived] = useState(false)
  const [archiveOpen, setArchiveOpen] = useState(true)
  const [limit, setLimit] = useState(PAGE)
  const [archiveLimit, setArchiveLimit] = useState(PAGE)
  const [groupLimits, setGroupLimits] = useState<Record<string, number>>({})
  const info = useContext(SessionInfo)

  const { active: matched, archived: archivedMatched } = useMemo(() => {
    const marks = new Map(flags.split('\n').filter(Boolean).map((line) => {
      const [key, mark] = line.split('\u0000')
      return [key!, mark!] as const
    }))
    const mark = (session: SessionSummary) => marks.get(conversationKey(session.directory, session.id)) ?? ''
    const found = matching(sessions, query)
    const pinnedFirst = (list: SessionSummary[]) =>
      list.sort((a, b) => Number(mark(b).includes('p')) - Number(mark(a).includes('p')))
    return {
      active: pinnedFirst(found.filter((session) => !mark(session).includes('a'))),
      archived: found.filter((session) => mark(session).includes('a')),
    }
  }, [sessions, query, flags])
  const keep = useCallback(
    (session: SessionSummary) => session.id === openId || info[conversationKey(session.directory, session.id)]?.status !== undefined,
    [openId, info],
  )
  const active = useMemo(() => page(matched, limit, keep), [matched, limit, keep])
  const archived = useMemo(() => page(archivedMatched, archiveLimit, keep), [archivedMatched, archiveLimit, keep])

  const toggleGroup = useCallback(
    (directory: string) => {
      const next = new Set(collapsed)
      if (!next.delete(directory)) next.add(directory)
      onCollapse(next)
    },
    [collapsed, onCollapse],
  )

  const groups = useMemo(() => (grouped ? grouping(matched) : []), [grouped, matched])
  // Each group first draws the rows it has among the list's first page, so grouping mounts no
  // more than the flat list does. Past that, each group pages on its own.
  const reach = useMemo(() => {
    const counts = new Map<string, number>()
    if (grouped) for (const session of matched.slice(0, limit)) counts.set(session.directory, (counts.get(session.directory) ?? 0) + 1)
    return counts
  }, [grouped, matched, limit])
  const moreInGroup = useCallback(
    (directory: string, shown: number) => setGroupLimits((limits) => ({ ...limits, [directory]: shown + PAGE })),
    [],
  )

  // A live query opens every group for as long as it runs. A person who typed something and
  // got a heading with nothing under it has been shown the opposite of what they asked for,
  // and quietly reopening beats making them undo a fold they set days ago — which is why
  // this reads through `collapsed` rather than clearing it.
  const searching = query.trim().length > 0
  const archivedShown = (showArchived || searching) && archivedMatched.length > 0

  return (
    <>
      <header className="sidebar-head">
        <NewSession onNew={onNew} />
        <SidebarSearch query={query} onQuery={setQuery} label="Filter sessions" placeholder="Search sessions">
          <IconMenu icon="filter" label="View options" className="view-options" data-test="view-options">
            <leo-menu-item data-role="menuitemcheckbox" aria-checked={grouped ? 'true' : 'false'} onClick={() => onGroup(!grouped)}>
              <span className="menu-icon-row">
                <Icon name="tabs-vertical-tree" />
                Group by project
                <span className="menu-check" aria-hidden="true">{grouped && <Icon name="check-normal" />}</span>
              </span>
            </leo-menu-item>
            <leo-menu-item data-role="menuitemcheckbox" aria-checked={showArchived ? 'true' : 'false'} onClick={() => setShowArchived(!showArchived)}>
              <span className="menu-icon-row">
                <Icon name="inbox" />
                Show archived
                <span className="menu-check" aria-hidden="true">{showArchived && <Icon name="check-normal" />}</span>
              </span>
            </leo-menu-item>
          </IconMenu>
        </SidebarSearch>
      </header>

      <div className="session-list">
        {sessions.length === 0 && (
          <p className="sidebar-empty">
            No sessions yet. Open a project to begin, or start one in a terminal with{' '}
            <code>bravebot</code> and it will appear here.
          </p>
        )}
        {/* Said separately, because the message above is a fact about the machine and would
            be a lie about a list that is merely filtered down to nothing. */}
        {sessions.length > 0 && matched.length === 0 && !archivedShown && (
          <p className="sidebar-empty">
            {searching ? `No conversation matches “${query}”.` : 'No active conversations. Start a new session, or show archived ones from View options.'}
          </p>
        )}
        {!grouped && (
          <>
            {active.rows.map((session) => (
              <Session
                key={`${session.directory}/${session.id}`}
                session={session}
                current={session.id === openId}
                forked={forked.has(keyOf(session.directory, session.id))}
                onOpen={onOpen}
              />
            ))}
            <ShowMore page={active} onMore={() => setLimit((shown) => shown + PAGE)} />
          </>
        )}
        {grouped &&
          groups.map((group) => (
            <Group
              key={group.directory}
              group={group}
              shown={Math.max(reach.get(group.directory) ?? 0, groupLimits[group.directory] ?? 0)}
              keep={keep}
              onMore={moreInGroup}
              open={searching || !collapsed.has(group.directory)}
              onToggle={toggleGroup}
              openId={openId}
              forked={forked}
              onOpen={onOpen}
              onNew={onNew}
            />
          ))}
        {archivedShown && (
          <section className="session-group-section session-archive" data-test="session-archive">
            <div className="session-group-head">
              <button type="button" className="session-group-fold" aria-expanded={archiveOpen || searching} onClick={() => setArchiveOpen(!archiveOpen)}>
                <Icon className={`chevron ${archiveOpen || searching ? 'open' : ''}`} name="carat-right" />
                <span className="session-group-name">Archived</span>
                <span className="count num">{archivedMatched.length}</span>
              </button>
            </div>
            <Fold open={archiveOpen || searching}>
              {archived.rows.map((session) => (
                <Session
                  key={`${session.directory}/${session.id}`}
                  session={session}
                  current={session.id === openId}
                  forked={forked.has(keyOf(session.directory, session.id))}
                  onOpen={onOpen}
                />
              ))}
              <ShowMore page={archived} onMore={() => setArchiveLimit((shown) => shown + PAGE)} />
            </Fold>
          </section>
        )}
      </div>
    </>
  )
}

/**
 * The row at the foot of a capped list that draws the next page of it.
 *
 * Focus moves to the first row it drew, since this button is gone once nothing is left to draw.
 */
function ShowMore({ page: { hidden, coming, first }, onMore }: { page: Page; onMore: () => void }): React.JSX.Element | null {
  if (hidden <= 0) return null
  const more = (event: React.MouseEvent<HTMLButtonElement>): void => {
    const list = event.currentTarget.parentElement
    flushSync(onMore)
    const key = first && conversationKey(first.directory, first.id)
    const row = [...(list?.querySelectorAll<HTMLElement>(':scope > .session-row') ?? [])].find((item) => item.dataset.session === key)
    row?.querySelector<HTMLElement>('button.session')?.focus()
  }
  return (
    <button type="button" className="sidebar-row session-show-more" data-test="show-more-sessions" onClick={more}>
      <Icon name="carat-down" />
      <span className="sidebar-row-label num">{coming === hidden ? `Show ${coming} more` : `Show ${coming} more of ${hidden}`}</span>
    </button>
  )
}

/**
 * One checkout's sessions, under a heading that opens and shuts them.
 *
 * Most of the heading is the disclosure control rather than a chevron beside it: the name is
 * the biggest thing in reach, and a group that can be folded should not ask for a 10px arrow
 * to be hit. `aria-expanded` carries the state and the name stays put.
 *
 * The heading is a row of two buttons rather than one, because the second one starts a
 * session here and a button cannot be nested inside a button.
 *
 * The rows stay mounted while shut, because that is how [`Fold`] has something to animate
 * away from; it hides them from the reader and from the tab order once the collapse has
 * finished.
 */
function Group({
  group,
  shown,
  keep,
  onMore,
  open,
  onToggle,
  openId,
  forked,
  onOpen,
  onNew,
}: {
  group: Group
  shown: number
  keep: (session: SessionSummary) => boolean
  onMore: (directory: string, shown: number) => void
  open: boolean
  onToggle: (directory: string) => void
  openId: string | undefined
  forked: ReadonlySet<string>
  onOpen: (summary: SessionSummary) => void
  onNew: (directory: string) => void
}): React.JSX.Element {
  const drawn = page(group.sessions, shown, keep)
  return (
    <section className="session-group-section">
      {/* The full path in the tooltip, because two checkouts of one project share a basename
          and picking the wrong one is a mistake nothing later announces. On both buttons: the
          one that starts a session here is exactly where that mistake would cost something. */}
      <div className="session-group-head">
        <button
          type="button"
          className="session-group-fold"
          aria-expanded={open}
          data-tooltip={group.directory}
          onClick={() => onToggle(group.directory)}
        >
          <Icon className={`chevron ${open ? 'open' : ''}`} name="carat-right" />
          <span className="session-group-name">{group.project}</span>
          <span className="count num">{group.sessions.length}</span>
        </button>
        {/* Named for the project rather than "New session", so a reader of the button list is
            told which of a dozen identical-looking pluses they have landed on. */}
        <IconButton
          icon="plus-add"
          size="tiny"
          className="session-group-new"
          label={`New session in ${group.project}`}
          tooltip={`New session in ${group.directory}`}
          onClick={() => onNew(group.directory)}
        />
      </div>
      <Fold open={open}>
        {drawn.rows.map((session) => (
          <Session
            key={`${session.directory}/${session.id}`}
            session={session}
            current={session.id === openId}
            forked={forked.has(keyOf(session.directory, session.id))}
            onOpen={onOpen}
          />
        ))}
        <ShowMore page={drawn} onMore={() => onMore(group.directory, shown)} />
      </Fold>
    </section>
  )
}

/**
 * One row, whichever arrangement it is standing in.
 *
 * The same component under a heading as in the flat list, so the two paths cannot drift into
 * showing different things about a session. The project stays on the row even when the
 * heading above already says it: the row is what a person reads.
 *
 * The leading slot says only what asks something of the reader — working, waiting, failed — or
 * whose conversation this is. A row with nothing to say leaves it empty, so the few that do
 * stand out down the column.
 */
const Session = memo(function Session({
  session,
  current,
  forked,
  onOpen,
}: {
  session: SessionSummary
  current: boolean
  forked: boolean
  onOpen: (summary: SessionSummary) => void
}): React.JSX.Element {
  const key = conversationKey(session.directory, session.id)
  const preferences = useConversationPreferences(key)
  const info = useContext(SessionInfo)[key]
  const [menu, setMenu] = useState(false)
  const trigger = useRef<HTMLElement>(null)
  const choose = (id: 'pin' | 'archive') =>
    setConversation(key, id === 'pin' ? { pinned: !preferences?.pinned } : { archived: !preferences?.archived })
  const shut = ({ reason }: { reason: string }) => {
    setMenu(false)
    // Escape and a chosen item return focus to the button that opened the menu; a click elsewhere
    // leaves it where the pointer landed.
    if (reason !== 'blur') trigger.current?.focus()
  }
  const status = info?.status
  return <div className={`session-row${current ? ' current' : ''}${menu ? ' menu-open' : ''}`} data-session={key}>
    <button type="button" className={`session${current ? ' current' : ''}`} aria-current={current ? 'true' : undefined}
      onClick={() => onOpen(session)} onContextMenu={contextMenu('session', session.id)}>
      <span className="session-status" data-status={status} data-tooltip={status ? STATUS_WORDS[status] : info?.bot?.name}>
        {status === 'working' ? <ProgressRing className="session-spinner" />
          : status ? <Icon name="dot" className={`status-dot ${status === 'failed' ? 'error' : 'warning'}`} />
            : info?.bot ? <BotFace seed={info.bot.avatar} size={16} /> : null}
      </span>
      <span className="session-text">
        <span className="session-title">
          {preferences?.pinned && <span className="session-pin" role="img" aria-label="Pinned"><Icon name="pin" /></span>}
          {forked && <span className="fork-mark"><ForkIcon /></span>}
          <span className="session-name">{session.title}</span>
        </span>
        <span className="session-where">
          {session.manifest && <><span className="plan-run" data-tooltip="A plan run. It can be read and not continued.">Plan run</span> · </>}
          {info?.bot && <>{info.bot.name} · </>}
          {session.project}{session.branch && <span className="branch"> · {session.branch}</span>}
        </span>
      </span>
      <time className="session-time num" dateTime={new Date(session.updated * 1000).toISOString()}>{shortAgo(session.updated)}</time>
      {forked && <span className="offscreen">Forked.</span>}
      {status && <span className="offscreen">, {STATUS_WORDS[status]}</span>}
    </button>
    <div className="session-more-menu">
      <IconButton ref={trigger} icon="more-horizontal" size="tiny" className="session-more"
        label={`Actions for ${session.title}`} tooltip={false} hasPopup="menu" expanded={menu}
        onClick={() => setMenu((open) => !open)} />
      {menu && (
        <Menu isOpen target={trigger.current ?? undefined} placement="bottom-end" positionStrategy="fixed" onClose={shut}>
          <leo-menu-item onClick={() => choose('pin')}>
            <span className="menu-icon-row"><Icon name="pin" />{preferences?.pinned ? 'Unpin conversation' : 'Pin conversation'}</span>
          </leo-menu-item>
          <leo-menu-item onClick={() => choose('archive')}>
            <span className="menu-icon-row"><Icon name="inbox" />{preferences?.archived ? 'Restore conversation' : 'Archive conversation'}</span>
          </leo-menu-item>
        </Menu>
      )}
    </div>
  </div>
})

/**
 * The button that starts a session, and the list of places to start one in.
 *
 * A split control: the row itself does exactly what it always did — opens the folder picker —
 * and the chevron beside it offers the projects opened before. Anything else would have made
 * the common case slower to reach in order to make the second case possible.
 */
function NewSession({ onNew }: { onNew: (directory?: string) => void }): React.JSX.Element {
  const [open, setOpen] = useState(false)
  const [directories, setDirectories] = useState<string[]>([])
  const menu = useRef<HTMLElement>(null)
  const trigger = useRef<HTMLElement>(null)
  // A load that returns after the menu was shut must not open it again.
  const opening = useRef(0)

  // Read when the menu is opened rather than held and kept in step: the list changes in the
  // main process, and a copy up here would be one more thing that can be stale.
  const show = useCallback(() => {
    const ticket = ++opening.current
    void window.bravebot.readRecents().then((found) => {
      if (ticket !== opening.current) return
      setDirectories(found)
      setOpen(true)
    })
  }, [])

  // The column clips overflow so a fold can slide under it. This menu has to paint past that
  // edge, over the transcript, for as long as it is open.
  useEffect(() => {
    const column = menu.current?.closest('.sessions')
    column?.classList.toggle('recents-open', open)
    return () => column?.classList.remove('recents-open')
  }, [open])

  const choose = useCallback((event: Event) => {
    const item = event.composedPath().find(
      (node): node is HTMLElement => node instanceof HTMLElement && node.tagName === 'LEO-MENU-ITEM',
    )
    const directory = item?.dataset.directory
    if (directory) onNew(directory)
  }, [onNew])

  useEffect(() => {
    const host = menu.current
    if (!host) return
    host.addEventListener('click', choose)
    return () => host.removeEventListener('click', choose)
  }, [choose])

  useEffect(() => {
    const host = menu.current
    if (!host || !open) return
    const keys = (event: KeyboardEvent): void => {
      const items = [...host.querySelectorAll<HTMLElement>('leo-menu-item:not([aria-disabled="true"])')]
      let target: HTMLElement | undefined
      if (event.key === 'Home') target = items[0]
      else if (event.key === 'End') target = items.at(-1)
      else if (event.key.length === 1 && !event.metaKey && !event.ctrlKey && !event.altKey) {
        const letter = event.key.toLocaleLowerCase()
        target = items.find((item) => item.textContent?.trim().toLocaleLowerCase().startsWith(letter))
      }
      if (!target) return
      event.preventDefault()
      event.stopPropagation()
      target.focus()
    }
    host.addEventListener('keydown', keys)
    return () => host.removeEventListener('keydown', keys)
  }, [open, directories])

  const shut = (detail: { reason: string }): void => {
    if (detail.reason === 'cancel' || detail.reason === 'select') {
      trigger.current?.focus()
    }
  }

  return (
    <div className="new-split">
      <SidebarRow icon="plus-add" label="New session" hint="⌘N" className="new" onClick={() => onNew()} data-test="new-session" />
      <ButtonMenu
        ref={menu}
        className="new-recent recent-menu"
        isOpen={open}
        placement="bottom-end"
        positionStrategy="fixed"
        onChange={({ isOpen: next }) => {
          if (next) show()
          else {
            opening.current += 1
            setOpen(false)
          }
        }}
        onClose={shut}
      >
        <IconButton
          ref={trigger}
          slot="anchor-content"
          icon="carat-down"
          label="Projects opened before"
          tooltip="Start in a recent project"
          hasPopup="menu"
          expanded={open}
        />
        {directories.length === 0 ? (
          <leo-menu-item aria-disabled="true">No projects opened yet</leo-menu-item>
        ) : (
          directories.map((directory) => (
            <leo-menu-item key={directory} data-directory={directory}>
              <span className="menu-icon-row recent-row">
                <Icon name="folder" />
                <span className="recent-text">
                  <span className="recent-name">{projectLabel(directory)}</span>
                  {/* Two checkouts of one project share a basename, and picking the wrong one
                      is a mistake nothing later would announce. */}
                  <span className="recent-path">{directory}</span>
                </span>
              </span>
            </leo-menu-item>
          ))
        )}
      </ButtonMenu>
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
  if (terms.length === 0) return [...sessions]
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
 * their newest session appeared.
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
 * not read as "2 hours ago" here and "1 hour ago" in the terminal. For sentences; a row
 * uses `shortAgo` from `../time`.
 */
export function ago(then: number): string {
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
