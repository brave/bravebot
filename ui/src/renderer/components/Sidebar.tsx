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
import { Button } from './ui/button'
import { SidebarContent } from './ui/sidebar'
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs'

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
    <aside className="sessions flex flex-col overflow-hidden bg-sidebar text-sidebar-foreground" id="sessions-column">
      <Tabs value={tab} onValueChange={(value) => show(value as Tab)} className="min-h-0 flex-1 gap-0">
        {/* Tabs add standard tab semantics; selection is carried by aria-selected. */}
        <TabsList className="sidebar-tabs" aria-label="What the column shows">
          <TabsTrigger value="sessions" className="sidebar-tab">
            Sessions
          </TabsTrigger>
          <TabsTrigger value="bots" className="sidebar-tab">
            Bots
          </TabsTrigger>
        </TabsList>

        <SidebarContent className="gap-0 overflow-hidden">
          <TabsContent value="sessions" forceMount hidden={tab !== 'sessions'} className="sidebar-body data-[state=inactive]:hidden">
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
          </TabsContent>

          <TabsContent value="bots" forceMount hidden={tab !== 'bots'} className="sidebar-body data-[state=inactive]:hidden">
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
          </TabsContent>
        </SidebarContent>
      </Tabs>

      <Button variant="ghost" className="agent-settings-open mx-3 my-2 min-w-[calc(var(--col-left-open)-24px)] shrink-0 justify-start text-muted-foreground hover:text-foreground" onClick={onSettings}>Agent settings</Button>
      {build && (
        <footer className="build px-3.5 pt-[7px] pb-2.5 font-mono text-[10px] leading-[1.4] text-ink-faint" title="The agent build these sessions are stamped with">
          {build}
        </footer>
      )}
    </aside>
  )
}
