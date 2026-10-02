import { SidebarSearch } from './SidebarTools'
import { IconButton } from './IconButton'
import { Modal } from './Modal'
import { Alert, Button, Icon, Input, TextArea } from '../nala'
/**
 * The other list in the left column: the bots somebody has defined.
 *
 * A session is a conversation and is named after whatever was asked first. A bot is somebody who
 * has them, a name, a purpose and a memory, so the list here is a list of *people* where the one
 * next door is a list of *occasions*. That is the whole reason it is a separate tab rather than a
 * filter over the same rows.
 *
 * What a row shows is what tells two bots apart when there are eight of them: the face, the name,
 * and what it is for. Opening one shows its conversations and its details in the main pane.
 */

import { useEffect, useMemo, useRef, useState } from 'react'
import { activeBots, retiredBots, type Bot } from '../../shared/bots'
import { newAvatarSeed } from '../../shared/avatar'
import { BotAvatar, type Doing } from './BotAvatar'
import { Fold } from './Fold'
import { ModelPicker } from './ModelPicker'

/** What a window may say about a bot. Everything else about one is the main process's. */
export interface BotFormValue { slug?: string; avatar?: string; model?: string | null; name: string; purpose: string }

interface Props {
  bots: Bot[]
  onOpen: (bot: Bot) => void
  /** The slug of the bot on screen, if one is. */
  openSlug: string | null
  /** What that bot is doing, so its row's face can match the header's. */
  openDoing: Doing
  onSave: (bot: BotFormValue) => Promise<boolean>
  /** Put one away, or bring it back. */
  onRetire: (slug: string, retired: boolean) => void
  /** Take one away for good. Only ever reached from the archive below. */
  onRemove: (slug: string) => void
}

export function Bots({
  bots,
  onOpen,
  openSlug,
  openDoing,
  onSave,
  onRetire,
  onRemove,
}: Props): React.JSX.Element {
  // Local, for the reason the session filter is: nothing outside this list reads it.
  const [query, setQuery] = useState('')
  const [creating, setCreating] = useState(false)
  // Whether the archive is open. Local for the same reason, and closed to begin with: the archive
  // is where things go to stop being in the way, and one that opened itself every launch would be
  // in the way.
  const [showing, setShowing] = useState(false)
  // Which archived bot has been asked about, if any. One at a time — arming a second disarms the
  // first, so there is never a fold of rows all sitting a click away from being deleted.
  const [deleting, setDeleting] = useState<string | null>(null)

  const inUse = useMemo(() => activeBots(bots).filter((bot) => `${bot.name} ${bot.purpose}`.toLowerCase().includes(query.toLowerCase())), [bots, query])
  const away = useMemo(() => retiredBots(bots), [bots])

  return (
    <>
      <header className="sidebar-head">
        <SidebarSearch query={query} onQuery={setQuery} label="Search bots" placeholder="Search">
          <IconButton icon="plus-add" label="New bot" tooltip="Create a bot" className="new" data-test="new-bot" onClick={() => setCreating(true)} />
        </SidebarSearch>
      </header>

      <div className="session-list">
        {inUse.length === 0 && !creating && (
          <p className="sidebar-empty">
            {query.trim() ? <>No bots match this search.</> : away.length === 0 ? (
              <>
                No bots yet. A bot is a name, a purpose and a memory, with conversations you can
                start in any project or in none.
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
          onOpen={onOpen} />)}
        {creating && <Modal title="Create bot" size="lg" onClose={() => setCreating(false)} className="bot-editor"
          subtitle="A name, a purpose and a memory. Each conversation runs in a project you pick, or in none.">
          <BotForm onCancel={() => setCreating(false)}
            onSave={async (next) => { const saved = await onSave(next); if (saved) setCreating(false); return saved }} />
        </Modal>}
      </div>

      {/* Only when there is something in it. An empty archive is a heading about nothing, and the
          whole point of the section is to be out of the way. Outside the scrolling list, so the
          archive is in the same place with three bots and with thirty. */}
      {away.length > 0 && (
        <section className="bot-archive">
          <div className="session-group-head">
            <button
              type="button"
              className="session-group-fold"
              aria-expanded={showing}
              // Closing the archive puts down whatever was picked up in it. A row left armed
              // behind a closed fold would be a question nobody can see waiting for an answer.
              onClick={() => {
                setDeleting(null)
                setShowing(!showing)
              }}
            >
              <Icon className={`chevron ${showing ? 'open' : ''}`} name="carat-right" />
              <span className="session-group-name">Archived</span>
              <span className="count num">{away.length}</span>
            </button>
          </div>
          <Fold open={showing} className="bot-archive-rows">
            {away.map((bot) => (
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
            ))}
          </Fold>
        </section>
      )}
    </>
  )
}

/** One bot: its face, its name, and what it is for. */
function BotRow({
  bot,
  open,
  doing,
  onOpen,
}: {
  bot: Bot
  open: boolean
  doing: Doing
  onOpen: (bot: Bot) => void
}): React.JSX.Element {
  return (
    <div className={`bot${open ? ' bot-open' : ''}`}>
      <button type="button" className="bot-open-button" aria-current={open ? 'true' : undefined} onClick={() => onOpen(bot)}>
        <BotAvatar seed={bot.avatar} doing={doing} size={40} />
        <span className="bot-said">
          <span className="bot-name">{bot.name}</span>
          <span className="bot-purpose" data-tooltip={bot.purpose}>{bot.purpose}</span>
        </span>
      </button>
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
 *
 * It asks *in the row* rather than in a dialog, which is the same call the transcript makes about
 * the agent's own questions: a modal takes the thing being decided off the screen and replaces it
 * with a sentence about it. Here the sentence goes where the purpose was, so the name of
 * the bot is still in front of whoever is answering. The second press is a different button in a
 * different place, so nobody arrives at it by double-clicking the first.
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
  return (
    <div className={`bot-archived${asking ? ' bot-asking' : ''}`}>
      <span className="bot-said">
        <span className="bot-name">{bot.name}</span>
        {asking ? (
          // Keep the retention notice visible and wrapping at narrow sidebar widths.
          <span className="bot-warning">Deletes local memory history. Project files and conversations stay.</span>
        ) : (
          <span className="bot-purpose" data-tooltip={bot.purpose}>{bot.purpose}</span>
        )}
      </span>
      {asking ? (
        <>
          <Button kind="plain-faint" size="tiny" className="bot-keep" onClick={onCancel}>
            Keep
          </Button>
          <Button
            kind="outline"
            size="tiny"
            className="bot-delete bot-delete-armed"
            data-tooltip={`Delete ${bot.name} and its local memory history for good. Project files and conversations are kept.`}
            onClick={onDelete}
          >
            Delete
          </Button>
        </>
      ) : (
        <>
          <Button
            kind="outline"
            size="tiny"
            className="bot-restore"
            data-tooltip={`Bring ${bot.name} back, with its session, its memory and its face.`}
            onClick={onRestore}
          >
            Restore
          </Button>
          <Button
            kind="plain-faint"
            size="tiny"
            className="bot-delete"
            data-tooltip={`Delete ${bot.name} for good. Local memory history is deleted. Project files and conversations are kept.`}
            onClick={onAsk}
          >
            Delete
          </Button>
        </>
      )}
    </div>
  )
}

/**
 * Making a bot.
 *
 * No folder: a bot has a home folder of its own, and each conversation is started in a project
 * picked for it then. A bot's name, purpose and memory are edited in its details panel.
 */
function BotForm({
  onSave,
  onCancel,
}: {
  onSave: (bot: BotFormValue) => Promise<boolean>
  onCancel: () => void
}): React.JSX.Element {
  // Keep the preview's face for this draft, including while its name changes.
  const [avatar, setAvatar] = useState(() => newAvatarSeed(crypto.randomUUID()))
  const [model, setModel] = useState<string | null>(null)
  const [name, setName] = useState('')
  const [purpose, setPurpose] = useState('')

  const [saving, setSaving] = useState(false)
  const [saveError, setSaveError] = useState('')
  const formRef = useRef<HTMLFormElement>(null)
  // The host does not forward `type` or `aria-label` into the shadow control.
  useEffect(() => {
    // Leo rebuilds a button's shadow tree when its slot changes, which drops the `type` set
    // during the commit. A Leo button can be a submit button only by asking the form to submit
    // (below), so this only keeps the others from behaving like one.
    // Apply again after that.
    let frame = 0
    let timer = 0
    const apply = () => {
      formRef.current?.querySelectorAll('leo-button').forEach((host) => {
        const button = host.shadowRoot?.querySelector('button')
        if (!button) return
        button.type = host.classList.contains('bot-save') ? 'submit' : 'button'
      })
    }
    apply()
    frame = requestAnimationFrame(apply)
    timer = window.setTimeout(apply, 0)
    return () => {
      cancelAnimationFrame(frame)
      window.clearTimeout(timer)
    }
  }, [saving])
  const onEscape = ({ innerEvent }: { innerEvent: Event }) => {
    if ((innerEvent as KeyboardEvent).key === 'Escape') onCancel()
  }
  // The save control lives in Leo's shadow tree, so the browser does not treat
  // it as this form's submit button. Enter in the name field and the button's
  // own click ask the form to submit.
  const submitForm = () => formRef.current?.requestSubmit()
  const onNameKey = ({ innerEvent }: { innerEvent: Event }) => {
    const key = (innerEvent as KeyboardEvent).key
    if (key === 'Escape') onCancel()
    if (key === 'Enter') { innerEvent.preventDefault(); submitForm() }
  }
  const save = async (value: Parameters<typeof onSave>[0]) => {
    if (saving) return
    setSaving(true); setSaveError('')
    try {
      if (!await onSave(value)) setSaveError('Could not save this bot. Your changes are still here; try again.')
    } catch (error) { setSaveError(String(error)) }
    finally { setSaving(false) }
  }

  const ready = name.trim().length > 0 && purpose.trim().length > 0

  return (
    <form
      ref={formRef}
      className="bot-form"
      onSubmit={(event) => {
        event.preventDefault()
        const submitter = (event.nativeEvent as SubmitEvent).submitter
        if (submitter) {
          const root = submitter.getRootNode()
          const host = root instanceof ShadowRoot ? root.host : submitter
          if (!host.classList.contains('bot-save')) return
        }
        if (ready) {
          void save({ avatar, model, name: name.trim(), purpose: purpose.trim() })
        }
      }}
    >
      <div className="bot-form-identity">
        <div className="bot-form-avatar">
          <BotAvatar seed={avatar} size={76} doing="waiting" />
          <IconButton icon="refresh" label="Refresh avatar" tooltip="Try a new avatar appearance" kind="outline" size="tiny"
            className="bot-avatar-refresh" onClick={() => setAvatar(newAvatarSeed(crypto.randomUUID()))} />
        </div>

        <div className="bot-field">
          <Input
            autofocus
            value={name}
            placeholder="Web dev bot"
            onInput={({ value }) => setName(value)}
            onChange={({ value }) => setName(value)}
            onKeyDown={onNameKey}
          >Name</Input>
        </div>
      </div>

      <div className="bot-field">
        <TextArea
          value={purpose}
          minRows={4}
          maxRows={12}
          resizeable
          placeholder="Build responsive pages, fix UI bugs, and improve accessibility. Follow the project’s existing styles and test your changes."
          onInput={({ value }) => setPurpose(value)}
          onChange={({ value }) => setPurpose(value)}
          onKeyDown={onEscape}
        >Purpose</TextArea>
      </div>

      <div className="bot-field bot-form-model">
        <span>Model</span>
        <ModelPicker scope="bot" model={model} disabled={false} onChoose={setModel} />
      </div>

      <p className="bot-note">
        A conversation in a project adds a <code>.bravebot-ui</code> folder there, holding this bot’s
        memory for that project. It ignores itself, so it will not show up as a change.
      </p>

      {saveError && <Alert type="error" size="small" role="alert">{saveError}</Alert>}
      <div className="bot-actions">
        <span className="bot-spacer" />
        <Button kind="plain-faint" size="medium" onClick={onCancel}>
          Cancel
        </Button>
        <Button kind="filled" size="medium" type="submit" className="bot-save" isDisabled={!ready || saving} onClick={submitForm}>
          {saving ? 'Saving…' : 'Create'}
        </Button>
      </div>
    </form>
  )
}
