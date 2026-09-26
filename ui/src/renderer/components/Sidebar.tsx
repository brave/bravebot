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

import { useEffect, useRef, useState } from 'react'
import type { SessionSummary } from '../../shared/protocol'
import type { Bot } from '../../shared/bots'
import type { Doing } from './BotAvatar'
import type { Tab } from '../../shared/view'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
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

  return (
    <aside
      className={
        // A folded column must not be tabbable or read out, and a zero-width grid track does
        // neither on its own. Hiding waits for the fold to finish so there is something to watch
        // on the way out; on the way back it lifts at once.
        //
        // The contents keep the width the column will come back at, so a fold slides them under a
        // clip instead of reflowing them — session titles re-wrapping into narrower and narrower
        // shapes for 180ms on their way to being invisible. A no-op while the column is open,
        // where the two widths are the same number.
        'sessions flex h-full flex-col overflow-hidden bg-sidebar text-sidebar-foreground' +
        ' [&>*]:min-w-[var(--col-left-open)]' +
        ' [.app.left-folded_&]:invisible [.app.left-folded_&]:[transition:visibility_0s_linear_var(--panel-duration)]'
      }
      id="sessions-column"
    >
      <Tabs
        value={tab}
        onValueChange={(value) => setTab(value as Tab)}
        className="flex min-h-0 flex-1 flex-col gap-0"
      >
        {/* No tooltips: the labels are the whole of what these do, and a popup could only
            repeat them. */}
        {/* The tabs carry the traffic-light clearance: 16px of inset, three 12px lights, two 8px
            gaps and a little air after them. They are what is at the top of the column now, and
            both bodies get the same head below them, so the offset has to be above the pair
            rather than inside each. The strip itself drags the window; the tabs opt out. */}
        <TabsList
          variant="line"
          className="sidebar-tabs h-auto w-full flex-none justify-stretch gap-0.5 px-3 pt-[46px] [-webkit-app-region:no-drag]"
          aria-label="What the column shows"
        >
          <TabsTrigger value="sessions" className="sidebar-tab h-auto px-2 py-1 text-xs">
            Sessions
          </TabsTrigger>
          <TabsTrigger value="bots" className="sidebar-tab h-auto px-2 py-1 text-xs">
            Bots
          </TabsTrigger>
        </TabsList>

        {/* Each body owns the rest of the column, and the one not chosen is hidden rather than
            unmounted — a filter somebody typed and a form half filled in both survive a look at
            the other tab. */}
        <TabsContent
          value="sessions"
          keepMounted
          className={cn(
            'sidebar-body flex min-h-0 flex-1 flex-col',
            // Base UI's keepMounted exit transition can leave `hidden` unset; drive scripts
            // and the preserved filter state both need the inactive body truly not shown.
            tab !== 'sessions' && '!hidden',
          )}
        >
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

        <TabsContent
          value="bots"
          keepMounted
          className={cn(
            'sidebar-body flex min-h-0 flex-1 flex-col',
            tab !== 'bots' && '!hidden',
          )}
        >
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
      </Tabs>

      {/* Keeps the open width while the column folds, with room for both horizontal margins. */}
      <Button
        variant="ghost"
        className="agent-settings-open mx-4 my-2.5 min-w-[calc(var(--col-left-open)-32px)] shrink-0"
        onClick={onSettings}
      >
        Agent settings
      </Button>
      {build && (
        <footer
          className="build shrink-0 border-t border-border px-3.5 py-2 font-mono text-[10px] text-muted-foreground/70"
          title="The agent build these sessions are stamped with"
        >
          {build}
        </footer>
      )}
    </aside>
  )
}
