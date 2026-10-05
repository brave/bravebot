/**
 * The left column, and the choice of which list is in it.
 *
 * There are two: the chats, which is every conversation the agent has a record of, and the
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

import { memo, useCallback, useEffect, useRef, useState } from 'react'
import type { SessionSummary } from '../../shared/protocol'
import type { Bot } from '../../shared/bots'
import type { Doing } from './BotAvatar'
import type { Tab } from '../../shared/view'
import { Sessions } from './Sessions'
import { Bots, type BotFormValue } from './Bots'
import { ControlItem, NavigationItem, SegmentedControl } from '../nala'

interface Props {
  sessions: SessionSummary[]
  openId: string | undefined
  forked: ReadonlySet<string>
  onOpen: (summary: SessionSummary) => void
  /** Start a chat in this folder, or ask for one with the picker when none is named. */
  onDelete: (summary: SessionSummary) => void
  onNew: (directory?: string) => void
  /** Start a chat in the project used last. */
  onNewChat: () => void
  /** Said when somebody switches the tab, so the first item of the other list can be opened. */
  onTab: (tab: Tab) => void
  bots: Bot[]
  /** The bot on screen, as a bot view or as one of its conversations. */
  openSlug: string | null
  /** What that bot is doing, so its row's face can match the header's. */
  openDoing: Doing
  onOpenBot: (bot: Bot) => void
  onSaveBot: (bot: BotFormValue) => Promise<boolean>
  onRetireBot: (slug: string, retired: boolean) => void
  onRemoveBot: (slug: string) => void
  onSettings: () => void
  build: string | null
}

export const Sidebar = memo(function Sidebar({
  sessions,
  openId,
  forked,
  onOpen,
  onDelete,
  onNew,
  onNewChat,
  onTab,
  bots,
  openSlug,
  openDoing,
  onOpenBot,
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

  // Read by the control's handler, which Leo may keep from the first render.
  const latest = useRef({ tab, onTab })
  latest.current = { tab, onTab }
  const show = useCallback((next: Tab) => {
    if (next === latest.current.tab) return
    setTab(next)
    latest.current.onTab(next)
  }, [])

  return (
    <aside className="sessions" id="sessions-column" data-build={build ?? undefined}>
      {/* The column's titlebar: the traffic lights, then the switch between the two lists. The
          strip drags the window; the control opts out. The label stays put and the selected
          item carries which is on, the disclosure discipline every toggle here follows. */}
      <div className="sidebar-titlebar">
        <SegmentedControl
          className="sidebar-tabs"
          size="small"
          value={tab}
          data-test="sidebar-tabs"
          onChange={({ value }) => { if (value === 'sessions' || value === 'bots') show(value) }}
        >
          <ControlItem value="sessions">Chats</ControlItem>
          <ControlItem value="bots">Bots</ControlItem>
        </SegmentedControl>
      </div>

      <div className="sidebar-body" hidden={tab !== 'sessions'}>
        <Sessions
          sessions={sessions}
          openId={openId}
          forked={forked}
          onOpen={onOpen}
          onDelete={onDelete}
          onNew={onNew}
          onNewChat={onNewChat}
          grouped={grouped}
          onGroup={setGrouped}
          collapsed={collapsed}
          onCollapse={setCollapsed}
        />
      </div>

      <div className="sidebar-body" hidden={tab !== 'bots'}>
        <Bots
          bots={bots}
          onOpen={onOpenBot}
          openSlug={openSlug}
          openDoing={openDoing}
          onSave={onSaveBot}
          onRetire={onRetireBot}
          onRemove={onRemoveBot}
        />
      </div>

      {/* The build the sessions are stamped with is in About; it rides here only as data, for
          the drivers that check a packaged app is the one they built. */}
      <footer className="sidebar-foot">
        <NavigationItem outsideList icon="settings" className="agent-settings-open" onClick={onSettings} data-test="agent-settings">
          Settings
        </NavigationItem>
      </footer>
    </aside>
  )
})
