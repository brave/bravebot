/**
 * The left column, and the choice of which list is in it.
 *
 * There are two: the sessions, which is every conversation the agent has a record of, and the
 * bots, which is the people who have one. They are separate tabs rather than one list with a mark
 * on some rows because they answer different questions — "what was I doing on Tuesday" and "who
 * works on this" — and a list that answers both answers neither well.
 *
 * ## Why the hidden one stays mounted
 *
 * Switching tabs hides a list with `display: none` rather than unmounting it, which is the rule
 * the context panels already follow and for the same reason: a filter somebody typed, a group they
 * folded, a form they were half way through are all things that should survive looking at
 * something else for a moment. A column that forgot them would make the tabs cost something to
 * press.
 *
 * ## Why what is remembered lives here
 *
 * The grouping and the folds used to be the session list's own, since nothing else read them. Two
 * lists sharing one file changes that: `view.json`'s key now holds which tab as well, and one
 * component reading and writing all of it is one write. Two components each writing their half of
 * the same key would race on every launch, which is the failure the `ready` ref below exists to
 * prevent within a single one.
 */

import { useCallback, useEffect, useRef, useState } from 'react'
import type { SessionSummary } from '../../shared/protocol'
import type { Bot } from '../../shared/bots'
import type { Doing } from './BotAvatar'
import type { Tab } from '../../shared/view'
import { Sessions } from './Sessions'
import { Bots } from './Bots'

interface Props {
  sessions: SessionSummary[]
  onNewBotConversation: (bot: Bot) => void
  onBotConversation: (bot: Bot, summary: SessionSummary) => void
  openId: string | undefined
  forked: ReadonlySet<string>
  onOpen: (summary: SessionSummary) => void
  onNew: (directory?: string) => void
  bots: Bot[]
  openSlug: string | null
  /** What that bot is doing, so its row's face can match the header's. */
  openDoing: Doing
  onSaveBot: (bot: { slug?: string; avatar?: string; model?: string | null; name: string; purpose: string; directory: string }) => Promise<boolean>
  onRetireBot: (slug: string, retired: boolean) => void
  onRemoveBot: (slug: string) => void
  onSettings: () => void
  build: string | null
}

export function Sidebar({
  sessions,
  onNewBotConversation,
  onBotConversation,
  openId,
  forked,
  onOpen,
  onNew,
  bots,
  openSlug,
  openDoing,
  onSaveBot,
  onRetireBot,
  onRemoveBot,
  build,
  onSettings,
}: Props): React.JSX.Element {
  const [tab, setTab] = useState<Tab>('sessions')
  const [grouped, setGrouped] = useState(false)
  // Which groups are shut, by directory. The shut ones rather than the open ones, so a checkout
  // that appears while the app is running arrives open rather than hidden.
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(() => new Set())

  // The stored value arrives asynchronously and so cannot seed `useState` — the same dance
  // `columns.ts` documents — which is why the column renders on the sessions tab for a frame
  // before adopting whichever was left. `ready` keeps that first frame from writing the default
  // back over what is on disk.
  const ready = useRef(false)
  useEffect(() => {
    void window.bravebot
      .readView()
      .then((view) => {
        setTab(view.tab)
        setGrouped(view.grouped)
        setCollapsed(new Set(view.collapsed))
      })
      .catch(() => undefined)
      .finally(() => (ready.current = true))
  }, [])
  useEffect(() => {
    if (!ready.current) return
    try {
      window.bravebot.writeView({ tab, grouped, collapsed: [...collapsed] })
    } catch {
      // The column is still arranged the way it was asked to be, this session.
    }
  }, [tab, grouped, collapsed])

  const show = useCallback((next: Tab) => setTab(next), [])

  return (
    <aside className="sessions" id="sessions-column">
      {/* The label stays put and `aria-pressed` carries which is on, the disclosure discipline
          every toggle in this window follows. No tooltips: the labels are the whole of what
          these do, and a popup could only repeat them. */}
      <div className="sidebar-tabs" role="group" aria-label="What the column shows">
        <button
          className="sidebar-tab"
          aria-pressed={tab === 'sessions'}
          onClick={() => show('sessions')}
        >
          Sessions
        </button>
        <button className="sidebar-tab" aria-pressed={tab === 'bots'} onClick={() => show('bots')}>
          Bots
        </button>
      </div>

      <div className="sidebar-body" hidden={tab !== 'sessions'}>
        <Sessions
          sessions={sessions}
          openId={openId}
          forked={forked}
          onOpen={onOpen}
          onNew={onNew}
          grouped={grouped}
          onGroup={setGrouped}
          collapsed={collapsed}
          onCollapse={setCollapsed}
        />
      </div>

      <div className="sidebar-body" hidden={tab !== 'bots'}>
        <Bots
          bots={bots}
          sessions={sessions}
          onNewConversation={onNewBotConversation}
          onConversation={onBotConversation}
          openSlug={openSlug}
          openDoing={openDoing}
          onSave={onSaveBot}
          onRetire={onRetireBot}
          onRemove={onRemoveBot}
        />
      </div>

      <button className="agent-settings-open" onClick={onSettings}>Agent settings</button>
      {build && (
        <footer className="build" title="The agent build these sessions are stamped with">
          {build}
        </footer>
      )}
    </aside>
  )
}
