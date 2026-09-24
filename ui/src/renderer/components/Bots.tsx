import { SidebarTools } from './SidebarTools'
import { BotMemory } from './BotMemory'
import { Modal } from './Modal'
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
import { activeBots, retiredBots, type Bot } from '../../shared/bots'
import { newAvatarSeed } from '../../shared/avatar'
import { BotAvatar, type Doing } from './BotAvatar'
import { ModelPicker } from './ModelPicker'
import { Button } from './ui/button'
import { Card, CardContent, CardFooter, CardHeader } from './ui/card'
import { DialogFooter } from './ui/dialog'
import { Empty, EmptyDescription } from './ui/empty'
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel } from './ui/field'
import { Input } from './ui/input'
import { Item, ItemContent, ItemDescription, ItemGroup, ItemTitle } from './ui/item'
import { Textarea } from './ui/textarea'
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from './ui/alert-dialog'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from './ui/collapsible'

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

  return (
    <>
      <header className="sessions-head">
        {/* The same control the session list's own opens with, so the two tabs begin the same
            way. No split beside it: a bot's folder is asked for once, in the form. */}
        <SidebarTools query={query} onQuery={setQuery} label="Search bots" action={<Button className="new" onClick={() => setEditing('new')}>
          <span className="plus" aria-hidden="true">
            +
          </span>
          New bot
        </Button>} />
      </header>

      <div className="session-list">
        {inUse.length === 0 && editing !== 'new' && (
          <Empty className="empty">
            <EmptyDescription>{query.trim() ? <>No bots match this search.</> : away.length === 0 ? (
              <>
                No bots yet. A bot is a name, a purpose and a memory, working in one checkout — and
                conversations you can continue or start afresh.
              </>
            ) : (
              // Said rather than left to the heading below, because "No bots yet" over a list of
              // archived ones would be the window contradicting itself in the same column.
              <>Every bot you have is in the archive. Bring one back, or make another.</>
            )}</EmptyDescription>
          </Empty>
        )}

        {inUse.map((bot) => <BotRow key={bot.slug} bot={bot} open={bot.slug === openSlug}
          doing={bot.slug === openSlug ? openDoing : bot.session === null ? 'waiting' : 'idle'}
          onOpen={() => setOverview(bot)} onEdit={() => setEditing(bot.slug)} />)}
        {editing && <Modal title={editing === 'new' ? 'Create bot' : 'Edit bot'} onClose={() => setEditing(null)} className="bot-editor">
          <h2>{editing === 'new' ? 'Create a bot' : 'Edit bot'}</h2>
          <BotForm bot={bots.find((bot) => bot.slug === editing)} onCancel={() => setEditing(null)}
            onSave={async (next) => { const saved = await onSave(next); if (saved) setEditing(null); return saved }}
            onArchive={editing === 'new' ? undefined : () => { onRetire(editing, true); setEditing(null) }} />
        </Modal>}
        {overview && <Modal title={overview.name} onClose={() => setOverview(null)} className="bot-overview">
          <Card className="bot-overview-summary">
            <CardHeader className="bot-overview-title"><BotAvatar seed={overview.avatar} size={56} doing="open" /><div><h2>{overview.name}</h2><p>{overview.directory}</p></div></CardHeader>
            <CardContent>
              <h3>Purpose</h3><p>{overview.purpose}</p>
              <p className="bot-note">This bot carries its purpose and saved memory into each conversation. Conversation history belongs to individual tasks.</p>
            </CardContent>
            <CardFooter className="bot-overview-actions">
              <Button type="button" className="primary" onClick={() => { onNewConversation(overview); setOverview(null) }}>New conversation</Button>
              <Button type="button" variant="outline" onClick={() => { setEditing(overview.slug); setOverview(null) }}>Edit bot and memory</Button>
            </CardFooter>
          </Card>
          <h3>Conversation history ({history.length})</h3>
          <p className="bot-note">All conversations for this bot, including archived conversations and drafts. Starting a new conversation keeps the earlier ones here.</p>
          {history.length > 0 && <Input className="bot-history-search" type="search" aria-label="Search bot conversations" placeholder="Search conversations…" value={historyQuery} onChange={event => setHistoryQuery(event.target.value)} />}
          <ItemGroup className="bot-conversations" aria-label="Bot conversation history">
            {filteredHistory.map(({ id, session, archived }) => session ?
              <Item asChild key={id}><Button type="button" variant="ghost" className="h-auto w-full items-start justify-start text-left" onClick={() => { onConversation(overview, session); setOverview(null) }}>
                <strong>{session.title}</strong><span>{id.startsWith('draft:') ? 'Draft' : new Date(session.updated * 1000).toLocaleDateString()}{archived ? ' · Archived' : ''}</span>
              </Button></Item> : <Item variant="outline" className="bot-history-unavailable" key={id}><ItemContent><ItemTitle>Unavailable conversation</ItemTitle><code>{id}</code><ItemDescription>The saved record is not currently available in the session list.</ItemDescription></ItemContent></Item>)}
            {history.length === 0 && <Empty className="bot-history-empty"><EmptyDescription>No conversations yet.</EmptyDescription></Empty>}
            {history.length > 0 && filteredHistory.length === 0 && <Empty className="bot-history-empty"><EmptyDescription>No conversations match “{historyQuery}”.</EmptyDescription></Empty>}
          </ItemGroup>
          <DialogFooter><Button type="button" onClick={() => setOverview(null)}>Done</Button></DialogFooter>
        </Modal>}
      </div>

      {/* Only when there is something in it. An empty archive is a heading about nothing, and the
          whole point of the section is to be out of the way.

          Outside the scrolling list rather than at the end of it, which is what makes it the foot
          of the column rather than whatever happens to be below the last bot. The tab already has
          a head that stays put while the rows move under it; this is the same bargain at the other
          end, and it means the archive is in the same place with three bots and with thirty. */}
      {away.length > 0 && (
        <Collapsible open={showing} onOpenChange={(open) => { setDeleting(null); setShowing(open) }} asChild>
        <section className="bot-archive">
          {/* The same folded heading the session list groups use, so a thing that opens and
              closes looks the same in both tabs. */}
          <div className="session-group-head">
            <CollapsibleTrigger asChild><Button
              variant="ghost"
              className="session-group-fold"
            >
              <span className={`chevron ${showing ? 'open' : ''}`} aria-hidden="true">
                ›
              </span>
              <span className="session-group-name">Archived</span>
              <span className="count">{away.length}</span>
            </Button></CollapsibleTrigger>
          </div>
          {/* The rows scroll on their own once there are enough of them. A fold pinned to the
              bottom of the column has no room to grow into, and one that pushed the list of bots
              off the top would be the archive taking the tab over. */}
          <CollapsibleContent forceMount className={`fold ${showing ? 'open' : ''}`}>
            <div className="fold-clip"><div className="bot-archive-rows">{away.map((bot) => (
              <ArchivedRow
                key={bot.slug}
                bot={bot}
                asking={deleting === bot.slug}
                onAsk={() => setDeleting(bot.slug)}
                onCancel={() => setDeleting(null)}
                onRestore={() => {
                  setDeleting(null)
                  onRetire(bot.slug, false)
                }}
                onDelete={() => {
                  setDeleting(null)
                  onRemove(bot.slug)
                }}
              />
            ))}</div></div>
          </CollapsibleContent>
        </section>
        </Collapsible>
      )}
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
  const where = bot.directory.split('/').pop() ?? bot.directory
  return (
    <Item className={`bot${open ? ' bot-open' : ''}`}>
      <Button variant="ghost" className="bot-open-button h-auto w-full justify-start text-left" onClick={() => onOpen(bot)}>
        <BotAvatar seed={bot.avatar} doing={doing} />
        <span className="bot-said">
          <span className="bot-name">{bot.name}</span>
          {/* The whole path in the tooltip, because the column clips it — the one case the
              tooltip rule here allows, which is text the layout took away. */}
          <span className="bot-where" title={bot.directory}>
            {where}
            {bot.session === null && ' · not spoken to yet'}
          </span>
        </span>
      </Button>
      <Button
        variant="ghost"
        size="icon-sm"
        className="bot-edit"
        aria-label={`Edit ${bot.name}`}
        title={`Edit ${bot.name}`}
        onClick={onEdit}
      >
        <span aria-hidden="true">⋯</span>
      </Button>
    </Item>
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
 *
 * It asks in an alert dialog so the irreversible action has a separate, explicit confirmation.
 * The dialog repeats the bot name and retention details, and focuses the safe cancel action first.
 *
 * What the words have to carry is that this is final, and they have to do it without overclaiming.
 * Saved sessions and project memory stay. App-owned memory revisions and the cached briefing
 * are deleted with the bot definition.
 */
function ArchivedRow({
  bot,
  asking,
  onAsk,
  onCancel,
  onRestore,
  onDelete,
}: {
  bot: Bot
  /** Whether this row is the one that has been asked about. */
  asking: boolean
  onAsk: () => void
  onCancel: () => void
  onRestore: () => void
  onDelete: () => void
}): React.JSX.Element {
  const where = bot.directory.split('/').pop() ?? bot.directory
  return (
    <Item size="sm" className={`bot-archived${asking ? ' bot-asking' : ''}`}>
      <span className="bot-said">
        <span className="bot-name">{bot.name}</span>
        <span className="bot-where" title={bot.directory}>{where}</span>
      </span>
      <Button variant="outline" type="button" className="bot-restore" title={`Bring ${bot.name} back, with its session, its memory and its face.`} onClick={onRestore}>Restore</Button>
      <Button variant="destructive" type="button" className="bot-delete" title={`Delete ${bot.name} for good. Local memory history is deleted. Project files and conversations are kept.`} onClick={onAsk}>Delete</Button>
      <AlertDialog open={asking} onOpenChange={(open) => { if (!open) onCancel() }}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete {bot.name}?</AlertDialogTitle>
            <AlertDialogDescription>Deletes local memory history and the bot definition. Project files and conversations stay.</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Keep bot</AlertDialogCancel>
            <AlertDialogAction variant="destructive" onClick={onDelete}>Delete bot</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </Item>
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
      className="bot-form"
      onSubmit={(event) => {
        event.preventDefault()
        if (ready) {
          void save({ slug: bot?.slug, avatar: bot ? undefined : avatar, model, name: name.trim(), purpose: purpose.trim(), directory })
        }
      }}
    >
      <FieldGroup>
        <div className={bot ? undefined : "bot-form-identity"}>
          {!bot && (
            <div className="bot-form-avatar">
              <BotAvatar seed={avatar} size={76} doing="waiting" />
              <Button
                variant="ghost"
                size="icon-sm"
                type="button"
                className="bot-avatar-refresh"
                aria-label="Refresh avatar"
                title="Try a new avatar appearance"
                onClick={() => setAvatar(newAvatarSeed(crypto.randomUUID()))}
              >
                <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                  <path d="M20 7v5h-5M4 17v-5h5" />
                  <path d="M6.1 7a7 7 0 0 1 11.6-1L20 12M4 12l2.3 6A7 7 0 0 0 17.9 17" />
                </svg>
              </Button>
            </div>
          )}

          <Field className="bot-field">
            <FieldLabel htmlFor="bot-name">Name</FieldLabel>
            <Input
              id="bot-name"
              value={name}
              autoFocus
              placeholder="Web dev bot"
              onChange={(event) => setName(event.target.value)}
              onKeyDown={(event) => event.key === 'Escape' && onCancel()}
            />
          </Field>
        </div>

        <Field className="bot-field">
          <FieldLabel htmlFor="bot-purpose">Purpose</FieldLabel>
          <Textarea
            id="bot-purpose"
            value={purpose}
            rows={4}
            placeholder="Build responsive pages, fix UI bugs, and improve accessibility. Follow the project’s existing styles and test your changes."
            onChange={(event) => setPurpose(event.target.value)}
            onKeyDown={(event) => event.key === 'Escape' && onCancel()}
          />
        </Field>

        <Field className="bot-field bot-form-model">
          <FieldLabel>Model</FieldLabel>
          <ModelPicker scope="bot" model={model} disabled={false} onChoose={setModel} />
        </Field>

        <Field className="bot-field">
          <FieldLabel>Project folder</FieldLabel>
          {bot ? (
            <div><p className="bot-fixed" title={bot.directory}>{bot.directory}</p>
            <FieldDescription className="bot-note">The project stays fixed to keep this bot’s memory and conversations together.</FieldDescription>
            <Button variant="outline" type="button" onClick={() => { void window.bravebot.chooseDirectory().then((folder) => { if (folder) void save({ name: `${name} copy`, purpose, model, directory: folder }) }) }}>Duplicate into another project</Button></div>
          ) : (
            <>
              <Button variant="outline" type="button" className="bot-choose" onClick={() => void choose()}>
                {directory || 'Choose a folder…'}
              </Button>
              {/* Said before the folder is picked rather than after, because it is the one consequence of
                  making a bot that touches something the person owns. */}
              <FieldDescription className="bot-note">
                A <code>.bravebot-ui</code> folder will appear in the checkout, holding this bot’s memory.
                It ignores itself, so it will not show up as a change.
              </FieldDescription>
            </>
          )}
        </Field>
      </FieldGroup>

      {bot && <BotMemory slug={bot.slug} />}

      <FieldError>{saveError}</FieldError>
      <DialogFooter className="bot-actions">
        {onArchive && (
          <Button
            variant="outline"
            type="button"
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
        <span className="bot-spacer" />
        <Button variant="outline" type="button" onClick={onCancel}>
          Cancel
        </Button>
        <Button type="submit" className="bot-save" disabled={!ready || saving}>
          {saving ? 'Saving…' : bot ? 'Save' : 'Create'}
        </Button>
      </DialogFooter>
    </form>
  )
}
