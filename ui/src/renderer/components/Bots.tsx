import { SidebarTools } from './SidebarTools'
import { BotMemory } from './BotMemory'
import {
  Dialog,
  DialogContent,
  DialogTitle,
} from '@/components/ui/dialog'
import { botHistory } from '../../shared/bot-history'
import { useExperience } from '../experience'
import type { SessionSummary } from '../../shared/protocol'
/**
 * The other list in the left column: the bots somebody has defined.
 *
 * A session is a conversation and is named after whatever was asked first. A bot is somebody who
 * has one — a name, a purpose, a memory, and one checkout — and the session behind it is resumed
 * rather than begun again, so the list here is a list of *people* where the one next door is a
 * list of *occasions*. That is the whole reason it is a separate tab rather than a filter over the
 * same rows.
 *
 * What a row shows is what tells two bots apart when there are eight of them: the face, the name,
 * and the checkout it works in. Not the last thing it said, which is the session list's business,
 * and not how long ago — a bot is not more or less itself for having been quiet.
 *
 * The form is here rather than in a window of its own for the reason the session list's folder
 * picker is: this is two fields and a folder, and a modal for it would be a ceremony around
 * something that takes one sentence to say.
 */

import { useCallback, useMemo, useState } from 'react'
import { RefreshCwIcon } from 'lucide-react'
import { activeBots, retiredBots, type Bot } from '../../shared/bots'
import { newAvatarSeed } from '../../shared/avatar'
import { projectLabel } from '../../shared/recents'
import { BotAvatar, type Doing } from './BotAvatar'
import { ModelPicker } from './ModelPicker'
import { Alert, AlertDescription } from '@/components/ui/alert'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { Button } from '@/components/ui/button'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import {
  Field,
  FieldGroup,
  FieldLabel,
} from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { cn, SCRIM } from '@/lib/utils'

interface Props {
  bots: Bot[]
  sessions: SessionSummary[]
  onNewConversation: (bot: Bot) => void
  onConversation: (bot: Bot, summary: SessionSummary) => void
  /** The slug of the bot whose session is on screen, if one is. */
  openSlug: string | null
  /** What that bot is doing, so its row's face can match the header's. */
  openDoing: Doing
  onSave: (bot: { slug?: string; avatar?: string; model?: string | null; name: string; purpose: string; directory: string }) => Promise<boolean>
  /** Put one away, or bring it back. */
  onRetire: (slug: string, retired: boolean) => void
  /** Take one away for good. Only ever reached from the archive below. */
  onRemove: (slug: string) => void
}

export function Bots({
  bots,
  sessions,
  onNewConversation,
  onConversation,
  openSlug,
  openDoing,
  onSave,
  onRetire,
  onRemove,
}: Props): React.JSX.Element {
  // Which bot's form is open, by slug, or `'new'` for one that does not exist yet. Local, and for
  // the reason the session filter is: nothing outside this list reads it, and a form half filled
  // in is not a preference anybody wants remembered.
  const [query, setQuery] = useState('')
  const [overviewSlug, setOverviewSlug] = useState<string | null>(null)
  const overview = bots.find(bot => bot.slug === overviewSlug) ?? null
  const [historyQuery, setHistoryQuery] = useState('')
  const preferences = useExperience()
  const history = overview ? botHistory(overview, sessions, preferences) : []
  const filteredHistory = history.filter(row => `${row.session?.title ?? ''} ${row.id}`.toLowerCase().includes(historyQuery.toLowerCase()))
  const setOverview = (bot: Bot | null) => { setOverviewSlug(bot?.slug ?? null); setHistoryQuery('') }
  const [editing, setEditing] = useState<string | null>(null)
  // Whether the archive is open. Local for the same reason, and closed to begin with: the archive
  // is where things go to stop being in the way, and one that opened itself every launch would be
  // in the way.
  const [showing, setShowing] = useState(false)
  // Which archived bot has been asked about, if any. One at a time — arming a second disarms the
  // first, so there is never a fold of rows all sitting a click away from being deleted.
  const [deleting, setDeleting] = useState<string | null>(null)

  const inUse = useMemo(() => activeBots(bots).filter((bot) => `${bot.name} ${bot.purpose} ${bot.directory}`.toLowerCase().includes(query.toLowerCase())), [bots, query])
  const away = useMemo(() => retiredBots(bots), [bots])
  const deletingBot = away.find((bot) => bot.slug === deleting) ?? null

  return (
    <>
      <header className="sessions-head flex-none px-3 pt-1.5 pb-2 [-webkit-app-region:drag] [&_:is(a,button,input,select,textarea)]:[-webkit-app-region:no-drag]">
        {/* The same control the session list's own opens with, so the two tabs begin the same
            way. No split beside it: a bot's folder is asked for once, in the form. */}
        <SidebarTools query={query} onQuery={setQuery} label="Search bots" findClass="bot-find" action={
          <Button
            className="new h-auto w-full justify-start rounded-lg border border-transparent bg-transparent px-2.5 py-1.5 text-left font-normal text-foreground hover:bg-foreground/10"
            onClick={() => setEditing('new')}
          >
            <span className="plus mr-1 font-semibold text-primary" aria-hidden="true">+</span>
            New bot
          </Button>
        } />
      </header>

      <div className="session-list min-h-0 flex-1 overflow-y-auto px-2 pt-1 pb-3">
        {inUse.length === 0 && editing !== 'new' && (
          <p className="empty px-1.5 py-2 text-xs text-muted-foreground/70">
            {query.trim() ? <>No bots match this search.</> : away.length === 0 ? (
              <>
                No bots yet. A bot is a name, a purpose and a memory, working in one checkout — and
                conversations you can continue or start afresh.
              </>
            ) : (
              // Said rather than left to the heading below, because "No bots yet" over a list of
              // archived ones would be the window contradicting itself in the same column.
              <>Every bot you have is in the archive. Bring one back, or make another.</>
            )}
          </p>
        )}

        {inUse.map((bot) => <BotRow key={bot.slug} bot={bot} open={bot.slug === openSlug}
          doing={bot.slug === openSlug ? openDoing : bot.session === null ? 'waiting' : 'idle'}
          onOpen={() => setOverview(bot)} onEdit={() => setEditing(bot.slug)} />)}
        {editing && (
          <Dialog open onOpenChange={(next) => { if (!next) setEditing(null) }}>
            {/* `scroll-padding` at both ends so the sticky footer of actions never lands on top of
                the field somebody has just tabbed into. */}
            <DialogContent
              className="modal bot-editor w-[min(680px,calc(100vw-48px))] max-w-none scroll-p-[28px_90px] p-7 sm:max-w-none max-h-[calc(100vh-64px)] overflow-y-auto"
              showCloseButton={false}
              overlayClassName={SCRIM}
            >
              <DialogTitle className="sr-only">{editing === 'new' ? 'Create bot' : 'Edit bot'}</DialogTitle>
              <h2 className="text-base font-medium">{editing === 'new' ? 'Create a bot' : 'Edit bot'}</h2>
              <BotForm bot={bots.find((bot) => bot.slug === editing)} onCancel={() => setEditing(null)}
                onSave={async (next) => { const saved = await onSave(next); if (saved) setEditing(null); return saved }}
                onArchive={editing === 'new' ? undefined : () => { onRetire(editing, true); setEditing(null) }} />
            </DialogContent>
          </Dialog>
        )}
        {overview && (
          <Dialog open onOpenChange={(next) => { if (!next) setOverview(null) }}>
            <DialogContent
              className="modal bot-overview w-[min(680px,calc(100vw-48px))] max-w-none p-7 sm:max-w-none max-h-[calc(100vh-64px)] overflow-y-auto"
              showCloseButton={false}
              overlayClassName={SCRIM}
            >
              <DialogTitle className="sr-only">{overview.name}</DialogTitle>
              <div className="bot-overview-title flex items-center gap-4"><BotAvatar seed={overview.avatar} size={56} doing="open" /><div><h2 className="text-base font-medium">{overview.name}</h2><p className="my-1 text-xs break-words text-muted-foreground">{overview.directory}</p></div></div>
              <h3 className="mt-6 text-[13px] font-medium">Purpose</h3><p>{overview.purpose}</p>
              <p className="bot-note text-[11px] leading-snug text-muted-foreground/70">This bot carries its purpose and saved memory into each conversation. Conversation history belongs to individual tasks.</p>
              <div className="bot-overview-actions mt-4 flex flex-wrap gap-2">
                <Button className="primary" onClick={() => { onNewConversation(overview); setOverview(null) }}>New conversation</Button>
                <Button variant="outline" onClick={() => { setEditing(overview.slug); setOverview(null) }}>Edit bot and memory</Button>
              </div>
              <h3 className="mt-6 text-[13px] font-medium">Conversation history ({history.length})</h3>
              <p className="bot-note text-[11px] leading-snug text-muted-foreground/70">All conversations for this bot, including archived conversations and drafts. Starting a new conversation keeps the earlier ones here.</p>
              {history.length > 0 && (
                <Input
                  className="bot-history-search w-full"
                  type="search"
                  aria-label="Search bot conversations"
                  placeholder="Search conversations…"
                  value={historyQuery}
                  onChange={event => setHistoryQuery(event.target.value)}
                />
              )}
              <div className="bot-conversations mb-5 grid max-h-75 gap-1.5 overflow-y-auto" aria-label="Bot conversation history">
                {filteredHistory.map(({ id, session, archived }) => session ?
                  <Button key={id} variant="ghost" className="grid h-auto justify-items-start gap-1 px-3 py-2.5 text-left font-normal" onClick={() => { onConversation(overview, session); setOverview(null) }}>
                    <strong className="text-[13px] font-medium break-words">{session.title}</strong><span className="text-xs text-muted-foreground">{id.startsWith('draft:') ? 'Draft' : new Date(session.updated * 1000).toLocaleDateString()}{archived ? ' · Archived' : ''}</span>
                  </Button> : <div className="bot-history-unavailable grid gap-1 rounded-lg border border-dashed border-border px-3 py-2.5 text-left" key={id}><strong className="text-[13px] font-medium break-words">Unavailable conversation</strong><code className="text-xs text-muted-foreground">{id}</code><span className="text-xs text-muted-foreground">The saved record is not currently available in the session list.</span></div>)}
                {history.length === 0 && <p>No conversations yet.</p>}
                {history.length > 0 && filteredHistory.length === 0 && <p>No conversations match “{historyQuery}”.</p>}
              </div>
              <Button variant="outline" onClick={() => setOverview(null)}>Done</Button>
            </DialogContent>
          </Dialog>
        )}
      </div>

      {/* Only when there is something in it. An empty archive is a heading about nothing, and the
          whole point of the section is to be out of the way.

          Outside the scrolling list rather than at the end of it, which is what makes it the foot
          of the column rather than whatever happens to be below the last bot. The tab already has
          a head that stays put while the rows move under it; this is the same bargain at the other
          end, and it means the archive is in the same place with three bots and with thirty. */}
      {away.length > 0 && (
        <Collapsible
          className="bot-archive flex-none border-t border-border px-2 pb-1"
          open={showing}
          onOpenChange={(next) => {
            // Closing the archive puts down whatever was picked up in it. A row left armed
            // behind a closed fold would be a question nobody can see waiting for an answer.
            setDeleting(null)
            setShowing(next)
          }}
        >
          {/* The same folded heading the session list groups use, so a thing that opens and
              closes looks the same in both tabs. */}
          {/* The session groups' heading, minus the two parts that do not carry over. Theirs is
              sticky because a long list scrolls a dozen of them past the eye; this one never moves.
              And theirs is in the accent, because a heading naming the checkout every row beneath
              it belongs to is something the eye should be able to find while scrolling. This is the
              opposite kind of heading: it names the place things go when nobody wants to look at
              them, so it is the quietest ink there is. */}
          <div className="session-group-head group/archive static flex w-full items-stretch bg-transparent hover:bg-foreground/8">
            <CollapsibleTrigger className="session-group-fold flex min-w-0 flex-1 items-center gap-1.5 border-0 bg-transparent pt-2 pr-1 pb-1.5 pl-2.5 text-left text-[11px] font-medium tracking-[0.05em] text-muted-foreground/70 uppercase group-hover/archive:text-muted-foreground">
              <span
                className={cn(
                  'chevron inline-block flex-none transition-transform duration-[180ms] ease-[cubic-bezier(0.32,0.72,0,1)] motion-reduce:transition-none',
                  showing && 'open rotate-90',
                )}
                aria-hidden="true"
              >
                ›
              </span>
              <span className="session-group-name truncate">Archived</span>
              <span className="count ml-auto flex-none rounded-lg bg-foreground/10 px-1.5 py-px text-[10px] text-muted-foreground/70">{away.length}</span>
            </CollapsibleTrigger>
          </div>
          {/* The rows scroll on their own once there are enough of them. A fold pinned to the
              bottom of the column has no room to grow into, and one that pushed the list of bots
              off the top would be the archive taking the tab over. */}
          <CollapsibleContent keepMounted className="bot-archive-rows max-h-[40vh] overflow-y-auto">
            {away.map((bot) => (
              <ArchivedRow
                key={bot.slug}
                bot={bot}
                onRestore={() => {
                  setDeleting(null)
                  onRetire(bot.slug, false)
                }}
                onAskDelete={() => setDeleting(bot.slug)}
              />
            ))}
          </CollapsibleContent>
        </Collapsible>
      )}

      <AlertDialog open={deleting !== null} onOpenChange={(open) => { if (!open) setDeleting(null) }}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete {deletingBot?.name ?? 'this bot'}?</AlertDialogTitle>
            <AlertDialogDescription>
              Deletes local memory history. Project files and conversations stay.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel className="bot-keep">Keep</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              className="bot-delete-armed"
              onClick={() => {
                if (deleting) onRemove(deleting)
                setDeleting(null)
              }}
            >
              Delete
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}

/**
 * One bot.
 *
 * Two buttons rather than a row with a control inside it, for the reason the session group heading
 * gives about its own plus: a button cannot be nested in a button, and the bigger of the two —
 * opening the bot — is the one that gets the whole row.
 */
function BotRow({
  bot,
  open,
  doing,
  onOpen,
  onEdit,
}: {
  bot: Bot
  open: boolean
  doing: Doing
  onOpen: (bot: Bot) => void
  onEdit: () => void
}): React.JSX.Element {
  const where = projectLabel(bot.directory)
  return (
    <div className={cn('bot group/bot flex items-center gap-0.5', open && 'bot-open')}>
      {/* The gap is tighter than it looks: the figure is drawn with air around it inside its own
          square, so a gap matched to the text would leave the name floating away from the face.
          An open bot wears the mark an open session does, so "this is the one on screen" reads the
          same in both tabs. */}
      <Button
        variant="ghost"
        className={cn(
          'bot-open-button h-auto min-w-0 flex-1 justify-start gap-1.5 rounded-lg bg-transparent px-2 py-1.5 text-left font-normal hover:bg-foreground/8',
          open && 'bg-bubble-user text-bubble-user-foreground hover:bg-bubble-user',
        )}
        onClick={() => onOpen(bot)}
      >
        <BotAvatar seed={bot.avatar} doing={doing} />
        <span className="bot-said flex min-w-0 flex-col gap-px">
          <span className="bot-name truncate">{bot.name}</span>
          {/* The whole path in the tooltip, because the column clips it — the one case the
              tooltip rule here allows, which is text the layout took away. */}
          <span className={cn('bot-where truncate text-xs', open ? 'opacity-85' : 'text-muted-foreground/70')} title={bot.directory}>
            {where}
            {bot.session === null && ' · not spoken to yet'}
          </span>
        </span>
      </Button>
      {/* Shown on hover or focus only. A row of eight bots with eight always-visible controls on
          the right is a column of furniture; the name is what the list is for. */}
      <Button
        variant="ghost"
        size="icon-sm"
        className="bot-edit w-6 flex-none bg-transparent p-0 text-muted-foreground/70 opacity-0 group-hover/bot:opacity-100 hover:bg-foreground/10 hover:text-foreground focus-visible:opacity-100"
        aria-label={`Edit ${bot.name}`}
        title={`Edit ${bot.name}`}
        onClick={onEdit}
      >
        <span aria-hidden="true">⋯</span>
      </Button>
    </div>
  )
}

/**
 * One bot that has been put away.
 *
 * Plainer than the row above it on purpose, and the missing piece is the face. Two reasons, and
 * they point the same way. A page gets a limited number of WebGL contexts — the whole of what
 * `BotAvatar`'s stage is arranged around — and an archive is exactly the list that can grow to
 * forty rows nobody is looking at, so spending one apiece there would cost the bots somebody *is*
 * looking at their faces. And a posture is a claim about what a bot is doing: the vocabulary has
 * no word for "not here", and a figure turning slowly beside a Restore button would be saying
 * something untrue quietly.
 *
 * Two things to do with an archived bot, and they are not the same size. Restore is free — it is
 * the archive's whole point, and undoing it is one more click. **Delete** is the only act in this
 * window that cannot be taken back, so it is the only control wearing the colour a deletion wears
 * in a diff, and it asks before it does anything.
 */
function ArchivedRow({
  bot,
  onRestore,
  onAskDelete,
}: {
  bot: Bot
  onRestore: () => void
  onAskDelete: () => void
}): React.JSX.Element {
  const where = projectLabel(bot.directory)
  // Padded to line up with `.bot-open-button` above it so the two lists share a left edge, and
  // dimmer, which is the whole of what "put away" has to say here.
  return (
    <div className="bot-archived flex items-center gap-1.5 rounded-lg px-2 py-1.5 text-muted-foreground hover:bg-foreground/8">
      <span className="bot-said flex min-w-0 flex-1 flex-col gap-px">
        <span className="bot-name truncate">{bot.name}</span>
        {/* The whole path in the tooltip, for the reason the row above gives: the column clips
            it, and this is text the layout took away. */}
        <span className="bot-where truncate text-xs text-muted-foreground/70" title={bot.directory}>
          {where}
        </span>
      </span>
      {/* Text rather than bordered buttons, unlike the form's: those sit in a row of their own at
          the end of a form and are the point of it; these sit beside a name in a list, and two
          boxes per row down a fold of them would make the archive louder than the bots in use. */}
      <Button
        type="button"
        variant="outline"
        size="sm"
        className="bot-restore flex-none border-0 bg-transparent px-1.5 py-0.5 text-[11px] font-normal text-muted-foreground/70 hover:bg-transparent hover:text-primary"
        title={`Bring ${bot.name} back, with its session, its memory and its face.`}
        onClick={onRestore}
      >
        Restore
      </Button>
      {/* The one act in this window that cannot be taken back, so it is the one control carrying
          its colour without being hovered first. Everything else in this column earns its colour
          on the way past, because everything else is reversible. */}
      <Button
        type="button"
        variant="destructive"
        size="sm"
        className="bot-delete flex-none border-0 bg-transparent px-1.5 py-0.5 text-[11px] font-normal"
        title={`Delete ${bot.name} for good. Local memory history is deleted. Project files and conversations are kept.`}
        onClick={onAskDelete}
      >
        Delete
      </Button>
    </div>
  )
}

/**
 * Making a bot, or changing one.
 *
 * The checkout is chosen once and shown afterwards rather than being editable: a bot's memory is a
 * file inside that checkout and its session was begun there, so moving one is not an edit to a
 * field, it is a different bot. Saying so by not offering the control beats offering it and
 * explaining afterwards.
 */
function BotForm({
  bot,
  onSave,
  onCancel,
  onArchive,
}: {
  bot?: Bot
  onSave: (bot: { slug?: string; avatar?: string; model?: string | null; name: string; purpose: string; directory: string }) => Promise<boolean>
  onCancel: () => void
  onArchive?: () => void
}): React.JSX.Element {
  // Keep the preview's face for this draft, including while its name changes.
  const [avatar, setAvatar] = useState(() => bot?.avatar ?? newAvatarSeed(crypto.randomUUID()))
  const [model, setModel] = useState<string | null>(bot?.model ?? null)
  const [name, setName] = useState(bot?.name ?? '')
  const [purpose, setPurpose] = useState(bot?.purpose ?? '')
  const [directory, setDirectory] = useState(bot?.directory ?? '')

  const choose = useCallback(async () => {
    // The same native picker the session list uses, and the same promise: this side never composes
    // a path, it is handed one somebody pointed at.
    const chosen = await window.bravebot.chooseDirectory()
    if (chosen) setDirectory(chosen)
  }, [])

  const [saving, setSaving] = useState(false)
  const [saveError, setSaveError] = useState('')
  const save = async (value: Parameters<typeof onSave>[0]) => {
    if (saving) return
    setSaving(true); setSaveError('')
    try {
      if (!await onSave(value)) setSaveError('Could not save this bot. Your changes are still here; try again.')
    } catch (error) { setSaveError(String(error)) }
    finally { setSaving(false) }
  }

  const ready = name.trim().length > 0 && purpose.trim().length > 0 && directory.length > 0

  return (
    <form
      className="bot-form flex flex-col gap-2.5"
      onSubmit={(event) => {
        event.preventDefault()
        if (ready) {
          void save({ slug: bot?.slug, avatar: bot ? undefined : avatar, model, name: name.trim(), purpose: purpose.trim(), directory })
        }
      }}
    >
      <FieldGroup>
        <div className={bot ? undefined : 'bot-form-identity grid grid-cols-[88px_minmax(0,1fr)] items-center gap-3'}>
          {!bot && (
            <div className="bot-form-avatar relative h-25 w-22">
              <BotAvatar seed={avatar} size={76} doing="waiting" />
              <Button
                type="button"
                variant="ghost"
                size="icon-xs"
                className="bot-avatar-refresh absolute right-0 bottom-0 size-8 rounded-md bg-transparent p-0 text-muted-foreground opacity-60 hover:text-foreground hover:opacity-100"
                aria-label="Refresh avatar"
                title="Try a new avatar appearance"
                onClick={() => setAvatar(newAvatarSeed(crypto.randomUUID()))}
              >
                <RefreshCwIcon />
              </Button>
            </div>
          )}

          <Field className="bot-field mt-4.5 min-w-0 [&>label]:text-[13px] [&>label]:font-semibold">
            <FieldLabel htmlFor="bot-name">Name</FieldLabel>
            <Input
              id="bot-name"
              className="w-full min-w-0 px-3 py-2.5 text-sm"
              value={name}
              autoFocus
              placeholder="Web dev bot"
              onChange={(event) => setName(event.target.value)}
              onKeyDown={(event) => event.key === 'Escape' && onCancel()}
            />
          </Field>
        </div>

        <Field className="bot-field mt-4.5 [&>label]:text-[13px] [&>label]:font-semibold">
          <FieldLabel htmlFor="bot-purpose">Purpose</FieldLabel>
          <Textarea
            id="bot-purpose"
            className="px-3 py-2.5 text-sm"
            value={purpose}
            rows={4}
            placeholder="Build responsive pages, fix UI bugs, and improve accessibility. Follow the project’s existing styles and test your changes."
            onChange={(event) => setPurpose(event.target.value)}
            onKeyDown={(event) => event.key === 'Escape' && onCancel()}
          />
        </Field>

        {/* The picker fills the field and its popover hangs below rather than above: in a form it
            has room underneath, unlike the copy pinned to the bottom of the composer. */}
        {/* No rules for `.model-popover` here: it is portalled to the document root, so it is not
            a descendant of this field and where it sits is the positioner's business now. */}
        <Field className="bot-field bot-form-model mt-4.5 [&_.model-picker]:max-w-none [&_.model-picker]:self-stretch [&_.model-trigger]:h-8 [&_.model-current]:text-xs [&>label]:text-[13px] [&>label]:font-semibold">
          <FieldLabel>Model</FieldLabel>
          <ModelPicker scope="bot" model={model} disabled={false} onChoose={setModel} />
        </Field>

        <Field className="bot-field mt-4.5 [&>label]:text-[13px] [&>label]:font-semibold">
          <FieldLabel>Project folder</FieldLabel>
          {bot ? (
            <div className="flex flex-col gap-2">
              <p className="bot-fixed m-0 rounded-lg border border-dashed border-border px-2 py-1.5 text-left text-xs break-words text-muted-foreground" title={bot.directory}>{bot.directory}</p>
              <p className="bot-note m-0 text-[11px] leading-snug text-muted-foreground/70">The project stays fixed to keep this bot’s memory and conversations together.</p>
              <Button
                type="button"
                variant="outline"
                onClick={() => {
                  void window.bravebot.chooseDirectory().then((folder) => {
                    if (folder) void save({ name: `${name} copy`, purpose, model, directory: folder })
                  })
                }}
              >
                Duplicate into another project
              </Button>
            </div>
          ) : (
            <Button
              type="button"
              variant="outline"
              className="bot-choose h-auto justify-start rounded-lg border border-dashed border-border bg-transparent px-2 py-1.5 text-left text-xs font-normal text-muted-foreground hover:border-primary hover:bg-transparent hover:text-foreground"
              onClick={() => void choose()}
            >
              <span className="truncate">{directory || 'Choose a folder…'}</span>
            </Button>
          )}
        </Field>
      </FieldGroup>

      {/* Said before the folder is picked rather than after, because it is the one consequence of
          making a bot that touches something the person owns. */}
      {!bot && (
        <p className="bot-note m-0 text-[11px] leading-snug text-muted-foreground/70 [&_code]:font-mono">
          A <code>.bravebot-ui</code> folder will appear in the checkout, holding this bot’s memory.
          It ignores itself, so it will not show up as a change.
        </p>
      )}

      {bot && <BotMemory slug={bot.slug} />}

      {saveError && (
        <Alert variant="destructive">
          <AlertDescription>{saveError}</AlertDescription>
        </Alert>
      )}
      {/* Sticky at the foot of the sheet, so Save is in reach however long the memory panel above
          it has grown. */}
      <div className="bot-actions sticky -bottom-7 z-2 flex items-center gap-1.5 bg-popover py-3">
        {onArchive && (
          <Button
            type="button"
            variant="outline"
            className="bot-archive-button"
            // The one thing worth saying about a bot leaving the list is what it does *not* do,
            // since a row disappearing looks like everything about it disappearing. It used to
            // say Forget, and the sentence here had to work quite hard: the definition went, and
            // with it the slug naming the memory file and the seed the face was drawn from, so
            // "its memory is left where it is" was true and no comfort at all. Now the sentence
            // is easy, because the thing it describes is.
            title="Put this bot away. It keeps its session, its memory and its face, and can be brought back from the archive."
            onClick={onArchive}
          >
            Archive
          </Button>
        )}
        <span className="bot-spacer flex-1" />
        <Button type="button" variant="outline" onClick={onCancel}>
          Cancel
        </Button>
        <Button type="submit" className="bot-save" disabled={!ready || saving}>
          {saving ? 'Saving…' : bot ? 'Save' : 'Create'}
        </Button>
      </div>
    </form>
  )
}
