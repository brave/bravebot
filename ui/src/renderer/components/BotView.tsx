import { memo, useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { flushSync } from 'react-dom'
import type { Bot } from '../../shared/bots'
import type { BotConversation } from '../../shared/bot-history'
import type { SessionSummary } from '../../shared/protocol'
import { projectLabel } from '../../shared/recents'
import { shortAgo } from '../time'
import { Icon, Input } from '../nala'
import { PullRequestMark, PullRequestRoom } from './PullRequestMark'
import { pullRequestLink } from '../../shared/pull-request'

/**
 * How many conversations are drawn before the list asks to draw more. Each row is a button with
 * several spans, and a bot talked to for a year has more of them than a window needs at once. The
 * search still runs over all of them; this limits what is drawn of the result.
 */
const PAGE = 100

type Row = BotConversation

/**
 * The bot's recent conversations.
 *
 * Its own memoized component, so typing in the composer above it, which re-renders the page on
 * every key, does not redraw the rows: they change only with the list, the search, or the page.
 */
const BotHistory = memo(function BotHistory({ rows, home, name, query, onOpen }: {
  rows: Row[]
  home: string
  name: string
  query: string
  onOpen: (summary: SessionSummary) => void
}): React.JSX.Element {
  const [limit, setLimit] = useState(PAGE)
  useEffect(() => { setLimit(PAGE) }, [rows])
  const drawn = rows.slice(0, limit)
  const hidden = rows.length - drawn.length
  const list = useRef<HTMLDivElement>(null)
  const more = (): void => {
    flushSync(() => setLimit((old) => old + PAGE))
    list.current?.querySelectorAll<HTMLElement>(':scope > .bot-history-item')[drawn.length]?.querySelector<HTMLElement>('.bot-history-row')?.focus()
  }
  return (
    <div ref={list} className="bot-conversations" aria-label={`${name}’s conversations`} data-test="bot-conversations">
      {drawn.map(({ id, directory, session, archived }) => {
        const where = directory === home ? 'No project' : projectLabel(directory)
        // A conversation the bot recorded that the chat list no longer holds: said, not hidden.
        if (!session) return (
          <div key={`${directory}/${id}`} className="bot-history-item">
            <div className="bot-history-row unavailable">
              <span className="session-meta"><span className="session-project">{where}</span></span>
              <span className="session-title"><span className="session-name">Unavailable conversation</span></span>
              <span className="session-where">Not in the chat list now · <code>{id}</code></span>
            </div>
          </div>
        )
        const pr = pullRequestLink(session.pullRequest)
        return (
          <div key={`${directory}/${id}`} className="bot-history-item">
            <button type="button" className="bot-history-row" onClick={() => onOpen(session)}>
              <span className="session-meta">
                <span className="session-project">{where}</span>
                {archived && <span className="bot-history-archived">Archived</span>}
                <time className="session-time num" dateTime={new Date(session.updated * 1000).toISOString()}>{shortAgo(session.updated)}</time>
              </span>
              <span className="session-title"><span className="session-name">{session.title}</span></span>
              {(session.branch || pr) && <span className={`session-where${pr ? ' has-pr' : ''}`}>{session.branch && <span className="branch">{session.branch}</span>}{pr && <PullRequestRoom url={session.pullRequest} />}</span>}
            </button>
            {pr && <PullRequestMark url={session.pullRequest} />}
          </div>
        )
      })}
      {hidden > 0 && (
        <button type="button" className="session-show-more" data-test="show-more-conversations" onClick={more}>
          <Icon name="carat-down" />
          <span className="session-show-more-label num">{Math.min(PAGE, hidden) === hidden ? `Show ${hidden} more` : `Show ${Math.min(PAGE, hidden)} more of ${hidden}`}</span>
        </button>
      )}
      {rows.length === 0 && (
        <p className="bot-history-empty">
          <Icon name="message-bubble" />
          {query.trim() ? `No conversation matches “${query}”.` : 'No earlier conversations.'}
        </p>
      )}
    </div>
  )
})

/**
 * A bot's earlier conversations, in its home folder and in the projects it worked in before it
 * was summoned there instead (MEMORY-14). Reached from the header of the bot's conversation, which
 * is where talking to it happens, so this page has no composer.
 */
export function BotView({ notices, bot, history, filtering, onOpen }: {
  /** Errors and confirmations. */
  notices: React.ReactNode
  bot: Bot
  history: BotConversation[]
  /** Whether the header's search is open, which filters the list. */
  filtering: boolean
  onOpen: (summary: SessionSummary) => void
}): React.JSX.Element {
  const [query, setQuery] = useState('')
  useEffect(() => { setQuery('') }, [bot.slug])
  useEffect(() => { if (!filtering) setQuery('') }, [filtering])

  const shown = useMemo(() => {
    const terms = query.toLowerCase().split(/\s+/).filter(Boolean)
    return history.filter((row) => terms.every((term) =>
      `${row.session?.title ?? row.id} ${row.directory === bot.home ? 'No project' : projectLabel(row.directory)} ${row.session?.branch ?? ''} ${pullRequestLink(row.session?.pullRequest)?.text ?? ''}`.toLowerCase().includes(term)))
  }, [history, query, bot.home])

  // The list is given one function for as long as this page is open, so a new `onOpen` from above
  // does not redraw it.
  const open = useRef(onOpen)
  open.current = onOpen
  const openConversation = useCallback((summary: SessionSummary) => open.current(summary), [])

  return (
    <>
      <div className="entries bot-view-body" data-test="bot-page">
        <div className="bot-view-column">
          {filtering && (
            <Input autofocus type="search" size="small" className="bot-history-search" aria-label="Search conversations"
              placeholder="Search conversations" value={query} onInput={({ value }) => setQuery(value)}>
              <Icon name="search" slot="left-icon" />
            </Input>
          )}
          <h2 className="bot-view-title">Earlier conversations</h2>
          <BotHistory rows={shown} home={bot.home} name={bot.name} query={query} onOpen={openConversation} />
        </div>
      </div>
      <div className="toast-stack">{notices}</div>
    </>
  )
}
