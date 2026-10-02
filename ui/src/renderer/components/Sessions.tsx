import { SidebarSearch } from './SidebarTools'
import { shortAgo } from '../time'
import { createContext, memo, useContext, useCallback, useMemo, useRef, useState } from 'react'
import type { SessionSummary } from '../../shared/protocol'
import type { ContextTarget } from '../../shared/commands'
import { keyOf } from '../../shared/forks'
import { Fold } from './Fold'
import { ForkIcon } from './ForkIcon'
import { BotFace } from './BotAvatar'
import { IconButton } from './IconButton'
import { IconMenu } from './IconMenu'
import { conversationKey } from '../../shared/experience'
import { useConversationPreferences, useExperienceValue, setConversation } from '../experience'
import { ButtonMenu, Icon, ProgressRing } from '../nala'

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
  /** Start a chat in the project used last. */
  onNewChat: () => void
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
  onNewChat,
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

  const { active, archived } = useMemo(() => {
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

  const toggleGroup = useCallback(
    (directory: string) => {
      const next = new Set(collapsed)
      if (!next.delete(directory)) next.add(directory)
      onCollapse(next)
    },
    [collapsed, onCollapse],
  )

  const groups = useMemo(() => (grouped ? grouping(active) : []), [grouped, active])

  // A live query opens every group for as long as it runs. A person who typed something and
  // got a heading with nothing under it has been shown the opposite of what they asked for,
  // and quietly reopening beats making them undo a fold they set days ago — which is why
  // this reads through `collapsed` rather than clearing it.
  const searching = query.trim().length > 0
  const archivedShown = (showArchived || searching) && archived.length > 0

  return (
    <>
      <header className="sidebar-head">
        <SidebarSearch query={query} onQuery={setQuery} label="Search chats" placeholder="Search">
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
          <IconButton icon="folder" label="New project…" tooltip="Open a project folder" className="new-project"
            data-test="new-project" onClick={() => onNew()} />
          <IconButton icon="plus-add" label="New chat" tooltip="New chat in the last project" shortcut="⌘N" className="new"
            data-test="new-session" onClick={onNewChat} />
        </SidebarSearch>
      </header>

      <div className="session-list">
        {sessions.length === 0 && (
          <p className="sidebar-empty">
            No chats yet. Open a project to begin, or start one in a terminal with{' '}
            <code>bravebot</code> and it will appear here.
          </p>
        )}
        {/* Said separately, because the message above is a fact about the machine and would
            be a lie about a list that is merely filtered down to nothing. */}
        {sessions.length > 0 && active.length === 0 && !archivedShown && (
          <p className="sidebar-empty">
            {searching ? `No conversation matches “${query}”.` : 'No active chats. Start a new chat, or show archived ones from View options.'}
          </p>
        )}
        {!grouped &&
          active.map((session) => (
            <Session
              key={`${session.directory}/${session.id}`}
              session={session}
              current={session.id === openId}
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
        {archivedShown && (
          <section className="session-group-section session-archive" data-test="session-archive">
            <div className="session-group-head">
              <button type="button" className="session-group-fold" aria-expanded={archiveOpen || searching} onClick={() => setArchiveOpen(!archiveOpen)}>
                <Icon className={`chevron ${archiveOpen || searching ? 'open' : ''}`} name="carat-right" />
                <span className="session-group-name">Archived</span>
                <span className="count num">{archived.length}</span>
              </button>
            </div>
            <Fold open={archiveOpen || searching}>
              {archived.map((session) => (
                <Session
                  key={`${session.directory}/${session.id}`}
                  session={session}
                  current={session.id === openId}
                  forked={forked.has(keyOf(session.directory, session.id))}
                  onOpen={onOpen}
                />
              ))}
            </Fold>
          </section>
        )}
      </div>
    </>
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
        {/* Named for the project rather than "New chat", so a reader of the button list is
            told which of a dozen identical-looking pluses they have landed on. */}
        <IconButton
          icon="plus-add"
          size="tiny"
          className="session-group-new"
          label={`New chat in ${group.project}`}
          tooltip={`New chat in ${group.directory}`}
          onClick={() => onNew(group.directory)}
        />
      </div>
      <Fold open={open}>
        {group.sessions.map((session) => (
          <Session
            key={`${session.directory}/${session.id}`}
            session={session}
            current={session.id === openId}
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
 * showing different things about a session. Three lines: where it runs and when it was last
 * active, what it is called, and the branch it was started on.
 *
 * The leading mark says what asks something of the reader — working, waiting, failed — and
 * otherwise whose conversation this is: a bot's face, or a folder for an ordinary chat.
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
  const anchor = useRef<HTMLElement>(null)
  const shutReason = useRef('explicit')
  const choose = (id: 'pin' | 'archive') => {
    setMenu(false)
    anchor.current?.focus()
    setConversation(key, id === 'pin' ? { pinned: !preferences?.pinned } : { archived: !preferences?.archived })
  }
  const status = info?.status
  return <div className={`session-row${current ? ' current' : ''}${menu ? ' menu-open' : ''}`}>
    <button type="button" className={`session${current ? ' current' : ''}`} aria-current={current ? 'true' : undefined}
      onClick={() => onOpen(session)} onContextMenu={contextMenu('session', session.id)}>
      <span className="session-meta">
        <span className="session-status" data-status={status} data-tooltip={status ? STATUS_WORDS[status] : info?.bot?.name}>
          {status === 'working' ? <ProgressRing className="session-spinner" />
            : status ? <Icon name="dot" className={`status-dot ${status === 'failed' ? 'error' : 'warning'}`} />
              : info?.bot ? <BotFace seed={info.bot.avatar} size={16} /> : <Icon name="folder-open" className="session-folder" />}
        </span>
        <span className="session-project">{session.project}</span>
        {session.manifest && <span className="plan-run" data-tooltip="A plan run. It can be read and not continued.">Plan run</span>}
        <time className="session-time num" dateTime={new Date(session.updated * 1000).toISOString()}>{shortAgo(session.updated)}</time>
      </span>
      <span className="session-title">
        {preferences?.pinned && <span className="session-pin" role="img" aria-label="Pinned"><Icon name="pin" /></span>}
        {forked && <span className="fork-mark"><ForkIcon /></span>}
        <span className="session-name">{session.title}</span>
      </span>
      {session.branch && <span className="session-where"><span className="branch">{session.branch}</span></span>}
      {info?.bot && <span className="offscreen">, with {info.bot.name}</span>}
      {forked && <span className="offscreen">Forked.</span>}
      {status && <span className="offscreen">, {STATUS_WORDS[status]}</span>}
    </button>
    <ButtonMenu className="session-more-menu" isOpen={menu} placement="bottom-end" positionStrategy="fixed"
      onClose={(detail) => { shutReason.current = detail.reason }}
      onChange={({ isOpen }) => {
        setMenu(isOpen)
        // Escape returns focus to the button that opened the menu; a click elsewhere leaves it
        // where the pointer landed.
        if (!isOpen && shutReason.current !== 'blur') anchor.current?.focus()
        if (!isOpen) shutReason.current = 'explicit'
      }}>
      <IconButton ref={anchor} slot="anchor-content" icon="more-vertical" size="tiny" className="session-more"
        label={`Actions for ${session.title}`} tooltip={false} hasPopup="menu" expanded={menu} />
      <leo-menu-item onClick={() => choose('pin')}>
        <span className="menu-icon-row"><Icon name="pin" />{preferences?.pinned ? 'Unpin conversation' : 'Pin conversation'}</span>
      </leo-menu-item>
      <leo-menu-item onClick={() => choose('archive')}>
        <span className="menu-icon-row"><Icon name="inbox" />{preferences?.archived ? 'Restore conversation' : 'Archive conversation'}</span>
      </leo-menu-item>
    </ButtonMenu>
  </div>
})

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

/**
 * The chat at the top of the list as it is drawn: pinned first, archived ones left out. This is
 * what switching to the Chats tab opens.
 */
export function firstChat(
  sessions: SessionSummary[],
  conversations: Record<string, { pinned?: boolean; archived?: boolean }>,
): SessionSummary | null {
  const flags = (session: SessionSummary) => conversations[conversationKey(session.directory, session.id)]
  const active = sessions.filter((session) => !flags(session)?.archived)
  return active.find((session) => flags(session)?.pinned) ?? active[0] ?? null
}
