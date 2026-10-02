import { memo, useEffect, useLayoutEffect, useRef, useState, type RefObject } from 'react'
import type { FileAttachment } from '../../shared/files'
import { projectLabel } from '../../shared/recents'
import { useContextWindow } from '../context-window'
import { Button, ButtonMenu, Icon, ProgressRing, TextArea } from '../nala'
import { FileGlyph } from './FileGlyph'
import { IconButton } from './IconButton'
import { ModelPicker } from './ModelPicker'

/** How many lines the field grows to before it starts to scroll. */
const FIELD_MAX_ROWS = 6

export interface ComposerProps {
  /** The field, for the transcript to focus: ⌘L, a sent message, an answered card. */
  input: RefObject<HTMLElement | null>
  session: string
  model: string | null
  running: boolean
  /** A trust question is open, and nothing may be sent until it is answered. */
  askingTrust: boolean
  compacting: boolean
  contextTokens?: number
  archived?: number
  /** A card is waiting on the reader, which the placeholder says. */
  pending: boolean
  scope: 'bot' | 'conversation'
  draft: string
  onDraft: (draft: string) => void
  onSend: () => void
  onQueue: () => void
  onCancel: () => void
  /** Offered only where a plan run can start: a conversation, not a bot. */
  onPlan?: () => void
  onModel: (model: string) => void
  attachments: FileAttachment[]
  onAttach: () => void
  onRemoveAttachment: (id: string) => void
  onPreview: (path: string) => void
  queued: string[]
  queuePaused: boolean
  onResumeQueued: () => void
  onRemoveQueued: (index: number) => void
  backendReady: boolean | null
  onSetup: () => void
  onCheckBackend: () => void
  onDiagnostics: () => void
  /** Whether files can be attached. A bot view has no session yet, so nothing to attach them to. */
  canAttach?: boolean
  /** The strip under the box: which project this chat runs in, and its branch. */
  footer?: ComposerFooterProps
}

/** How the next message is handled: as an ordinary turn, or as one manifest run. */
type Mode = 'agent' | 'plan'

const PLAN_BLOCKED: Record<'unavailable' | 'bot' | 'attachments', string> = {
  unavailable: 'A plan run cannot start here.',
  bot: 'A bot answers in turns, so it has no plan mode.',
  attachments: 'A plan is fixed before anything is read, so it cannot take attached files. Remove them, and name the file in the task.',
}

/**
 * The choice between Agent and Plan for the next message.
 *
 * Plan goes back to Agent once that message is sent, so it reads as a choice about one message and
 * never as a mode the conversation is in.
 */
function ModeMenu({ mode, blocked, disabled, onMode }: {
  mode: Mode
  blocked: keyof typeof PLAN_BLOCKED | null
  disabled: boolean
  onMode: (mode: Mode) => void
}): React.JSX.Element {
  const [open, setOpen] = useState(false)
  const choose = (next: Mode) => { setOpen(false); onMode(next) }
  const label = mode === 'plan' ? 'Plan' : 'Agent'
  return (
    <ButtonMenu className="mode-menu" isOpen={open} placement="top-end" positionStrategy="fixed" onChange={({ isOpen }) => setOpen(isOpen)}>
      <Button slot="anchor-content" kind="plain-faint" size="tiny" className={`mode-trigger${mode === 'plan' ? ' planning' : ''}`}
        isDisabled={disabled} aria-haspopup="menu" aria-expanded={open} aria-label={`Mode: ${label}`} data-test="composer-mode"
        data-tooltip={mode === 'plan' ? 'The next message starts a plan run' : 'Choose how the next message is handled'}>
        {label}
        <Icon name="carat-down" slot="icon-after" />
      </Button>
      <leo-menu-item onClick={() => choose('agent')} data-test="mode-agent" aria-checked={mode === 'agent' ? 'true' : 'false'}>
        <span className="menu-icon-row">
          <span className="menu-text">
            <span className="menu-title">Agent</span>
            <span className="menu-subtitle">Works in turns, deciding each step after reading</span>
          </span>
          <span className="menu-check" aria-hidden="true">{mode === 'agent' && <Icon name="check-normal" />}</span>
        </span>
      </leo-menu-item>
      <leo-menu-item onClick={() => { if (!blocked) choose('plan') }} data-test="mode-plan"
        aria-disabled={blocked ? 'true' : undefined} aria-checked={mode === 'plan' ? 'true' : 'false'}
        data-tooltip={blocked ? PLAN_BLOCKED[blocked] : undefined}>
        <span className="menu-icon-row">
          <span className="menu-text">
            <span className="menu-title">Plan</span>
            <span className="menu-subtitle">{blocked ? PLAN_BLOCKED[blocked] : 'Plans the whole task, shows you the plan, then runs it'}</span>
          </span>
          <span className="menu-check" aria-hidden="true">{mode === 'plan' && <Icon name="check-normal" />}</span>
        </span>
      </leo-menu-item>
    </ButtonMenu>
  )
}

/** Chosen from the project menu: a folder, no project, or the picker. */
export type ProjectChoice = { kind: 'folder'; directory: string } | { kind: 'none' } | { kind: 'pick' }

export interface ComposerFooterProps {
  /** The folder the chat runs in, or `null` for a bot's conversation with no project. */
  directory: string | null
  branch: string | null
  /**
   * What the project menu offers, or `null` once the conversation has started and the project is
   * fixed. `noProject` is offered to a bot only.
   */
  choices: { folders: string[]; noProject: boolean } | null
  onChoose: (choice: ProjectChoice) => void
}

/**
 * The message box, and what is docked to it: the backend notice and the queue above, the
 * attachments inside, the model and the context meter along its foot.
 *
 * Memoised, and given only stable callbacks, so a keystroke re-draws this and not the transcript
 * above it. The draft itself stays with `App`, which saves it per conversation and greys the Send
 * menu item on it.
 */
export const Composer = memo(function Composer(props: ComposerProps): React.JSX.Element {
  const {
    input, session, model, running, askingTrust, compacting, contextTokens, archived, pending, scope,
    draft, onDraft, onCancel, onPlan, onModel, attachments, onAttach, onRemoveAttachment, onPreview,
    queued, queuePaused, onResumeQueued, onRemoveQueued, backendReady, onSetup, onCheckBackend, onDiagnostics,
    canAttach = true, footer,
  } = props
  // Read by the key handler, which Leo may keep from the first render.
  const latest = useRef(props)
  latest.current = props

  // Leo's TextArea only works out its rows while somebody types, so a restored draft would sit
  // in a one-line box and scroll. Size the field Leo draws from its content instead, and move
  // between heights rather than jumping: measured at `auto` with the transition off, then set
  // back to where it was so the change to the new height is one the browser can animate.
  //
  // Measuring at `auto` and setting the height back is three layouts of the whole window, and
  // the transcript is in it: on a long one that is most of a frame per key. So the height is only
  // measured that way when it might have to shrink; while the text grows, or stays, the field's
  // own scroll height says all there is to say.
  const drafted = useRef(0)
  useLayoutEffect(() => {
    let frame = 0
    let tries = 0
    const fit = () => {
      const field = input.current?.shadowRoot?.querySelector('textarea')
      if (!field) { if (tries++ < 20) frame = requestAnimationFrame(fit); return }
      // Leo starts the field at three rows and only changes that while somebody types, so an empty
      // box would measure three lines tall. One row makes the measurement below the content's own.
      if (field.rows !== 1) field.rows = 1
      const lineHeight = parseFloat(getComputedStyle(field).lineHeight)
      const fieldMax = Number.isFinite(lineHeight) ? lineHeight * FIELD_MAX_ROWS : field.scrollHeight
      const shorter = draft.length < drafted.current
      drafted.current = draft.length
      if (!shorter && field.scrollHeight <= field.clientHeight && field.style.height) return
      const from = field.offsetHeight
      let to = Math.min(fieldMax, field.scrollHeight)
      if (shorter || !field.style.height) {
        field.style.transition = 'none'
        field.style.height = 'auto'
        to = Math.min(fieldMax, field.scrollHeight)
        field.style.height = `${from}px`
        void field.offsetHeight
      }
      if (to === from && field.style.height) return
      field.style.transition = 'height var(--motion-fast)'
      field.style.height = `${to}px`
    }
    fit()
    return () => cancelAnimationFrame(frame)
  }, [draft, session, input])

  const blocked = askingTrust || backendReady === false
  const canSend = !blocked && draft.trim().length > 0

  // Agent or Plan, for the next message only. Plan starts one manifest run and the menu goes back
  // to Agent, because a session may not hold the mode (MANIFEST-9).
  const [mode, setMode] = useState<Mode>('agent')
  const planBlocked = !onPlan ? 'unavailable' : scope === 'bot' ? 'bot' : attachments.length > 0 ? 'attachments' : null
  useEffect(() => { setMode('agent') }, [session])
  useEffect(() => { if (planBlocked) setMode('agent') }, [planBlocked])
  const modeNow = useRef(mode)
  modeNow.current = planBlocked ? 'agent' : mode
  const submit = () => {
    const now = latest.current
    if (modeNow.current === 'plan' && now.onPlan) {
      setMode('agent')
      now.onPlan()
    } else now.onSend()
  }

  return (
    <footer className="composer">
      <div className="composer-stack">
        {backendReady === false && <BackendTray onSetup={onSetup} onCheckBackend={onCheckBackend} onDiagnostics={onDiagnostics} />}
        {queued.length > 0 && (
          <div className="composer-tray queued-messages" role="group" aria-label="Queued messages">
            <div className="tray-head">
              <span className="tray-title">{queuePaused ? 'Queue paused' : 'Queued'} <span className="num">· {queued.length}</span></span>
              <span className="tray-note">{queuePaused ? 'Held until you resume' : 'Each starts a new turn after this one'}</span>
              {queuePaused && (
                <Button kind="plain" size="tiny" className="tray-action" aria-label="Resume queue"
                  isDisabled={running || backendReady === false} onClick={onResumeQueued}>Resume</Button>
              )}
            </div>
            {queued.map((text, index) => (
              <div className="tray-row" key={index}>
                <span className="tray-text" data-tooltip={text}>{text}</span>
                <IconButton icon="close" size="tiny" label={`Remove queued message ${index + 1}`} tooltip="Remove"
                  onClick={() => onRemoveQueued(index)} />
              </div>
            ))}
          </div>
        )}
        <div className={`composer-shell${footer ? ' with-footer' : ''}`}>
        <div className="composer-box">
          {attachments.length > 0 && (
            <div className="attachment-chips">
              {attachments.map((file) => (
                <span className="attachment-chip" key={file.id}>
                  <FileGlyph name={file.path} />
                  <button type="button" className="attachment-name" data-tooltip={`Preview ${file.path}`}
                    onClick={() => onPreview(file.path)}>{file.path.split('/').at(-1)}</button>
                  <IconButton icon="close" size="tiny" label={`Remove attachment ${file.path}`} tooltip="Remove"
                    onClick={() => onRemoveAttachment(file.id)} />
                </span>
              ))}
              <span className="attachment-trust" tabIndex={0}
                data-tooltip="Attached files are sent with your message as trusted context: the model reads them as you wrote them.">
                <Icon name="warning-triangle-outline" />Sent as trusted context
              </span>
            </div>
          )}
          <TextArea ref={input} mode="plain" minRows={1} maxRows={FIELD_MAX_ROWS} value={draft} aria-label="Message the agent"
            placeholder={pending ? 'Draft your next message while you review…' : 'Let’s do something great'}
            onInput={({ value }) => onDraft(value)}
            onKeyDown={({ innerEvent }) => {
              const event = innerEvent as unknown as KeyboardEvent
              const now = latest.current
              if (event.key === 'Escape' && now.running && !event.isComposing && !openElsewhere()) {
                event.preventDefault()
                now.onCancel()
                return
              }
              if (event.key === 'Enter' && !event.shiftKey && !event.metaKey && !event.ctrlKey && !event.altKey
                && !event.isComposing && event.keyCode !== 229) {
                event.preventDefault()
                // The round button is Stop for the whole time a reply is generating. Enter still
                // queues a follow-up, which is the path the button used to offer as "Queue message".
                if (!event.repeat && !now.askingTrust && now.backendReady !== false && now.draft.trim()) {
                  if (now.running) now.onQueue()
                  else submit()
                }
              }
            }} />
          <div className="composer-toolbar">
            <IconButton icon="attachment" label="Attach files" className="attach-files" onClick={onAttach}
              disabled={!canAttach || attachments.length >= 5}
              tooltip={!canAttach ? 'Send a first message, then attach files' : attachments.length >= 5 ? 'Five files at most' : 'Attach files'} />
            <span className="toolbar-spacer" />
            <ContextMeter session={session} model={model} tokens={contextTokens} archived={archived} compacting={compacting} />
            <ModelPicker compact session={session} scope={scope} key={session} model={model} disabled={running} onChoose={onModel} />
            {onPlan && <ModeMenu mode={planBlocked ? 'agent' : mode} blocked={planBlocked} disabled={running} onMode={setMode} />}
            <IconButton icon={running ? 'stop-filled' : 'arrow-up'} label={running ? 'Stop' : 'Send'}
              shortcut={running ? '⌘.' : '⌘↩'}
              kind={running ? 'outline' : 'filled'} size="medium" className={running ? 'send stop' : 'send'}
              onClick={() => { if (running) onCancel(); else submit() }}
              disabled={!running && !canSend}
              data-test={running ? 'stop-turn' : 'send-message'} />
          </div>
        </div>
        {footer && <ComposerFooter {...footer} disabled={running} />}
        </div>
      </div>
    </footer>
  )
})

/**
 * The strip under the message box: the project on the left, the branch on the right.
 *
 * The project is a menu until the first message is sent, and plain text after that, because a
 * conversation cannot move to another folder once it has started. The branch is read off the
 * checkout and only shown.
 */
function ComposerFooter({ directory, branch, choices, onChoose, disabled }: ComposerFooterProps & { disabled: boolean }): React.JSX.Element {
  const label = directory === null ? 'No project' : projectLabel(directory)
  return (
    <div className="composer-footer" data-test="composer-footer">
      {choices ? (
        <ProjectMenu directory={directory} choices={choices} onChoose={onChoose} disabled={disabled} variant="footer" />
      ) : (
        <span className="composer-project" data-tooltip={directory ?? 'This bot’s own folder'}>
          <Icon name="folder-open" />{label}
        </span>
      )}
      <span className="toolbar-spacer" />
      {branch && (
        <span className="composer-branch" data-tooltip={`Branch ${branch}`}>
          <Icon name="fork-arrows" /><span className="composer-branch-name">{branch}</span>
        </span>
      )}
    </div>
  )
}

/**
 * The menu of projects a chat can move to before anything is said in it.
 *
 * One menu, opened from two places: the button in the composer's footer, and the project's name in
 * the greeting over a new chat. Both show the same list and make the same choice.
 */
export function ProjectMenu({ directory, choices, onChoose, disabled = false, variant }: {
  directory: string | null
  choices: NonNullable<ComposerFooterProps['choices']>
  onChoose: (choice: ProjectChoice) => void
  disabled?: boolean
  variant: 'footer' | 'title'
}): React.JSX.Element {
  const [open, setOpen] = useState(false)
  const label = directory === null ? 'No project' : projectLabel(directory)
  const choose = (choice: ProjectChoice) => { setOpen(false); onChoose(choice) }
  return (
    <ButtonMenu className={`project-menu recent-menu ${variant}`} isOpen={open} placement={variant === 'title' ? 'bottom' : 'top-start'}
      positionStrategy="fixed" onChange={({ isOpen }) => setOpen(isOpen)}>
      {variant === 'footer' ? (
        <Button slot="anchor-content" kind="plain-faint" size="tiny" className="project-trigger" isDisabled={disabled}
          aria-haspopup="menu" aria-expanded={open} aria-label={`Project: ${label}`} data-tooltip={directory ?? 'This bot’s own folder'}
          data-test="project-trigger">
          <Icon name="folder-open" slot="icon-before" />
          {label}
          <Icon name="carat-down" slot="icon-after" />
        </Button>
      ) : (
        <button type="button" slot="anchor-content" className="fresh-project" disabled={disabled}
          aria-haspopup="menu" aria-expanded={open} aria-label={`Project: ${label}. Choose another`}
          data-tooltip={directory ?? 'This bot’s own folder'} data-test="greeting-project">
          {label}
        </button>
      )}
      {choices.noProject && (
        <leo-menu-item onClick={() => choose({ kind: 'none' })}>
          <span className="menu-icon-row"><Icon name="message-bubble" />No project</span>
        </leo-menu-item>
      )}
      {choices.folders.map((folder) => (
        <leo-menu-item key={folder} onClick={() => choose({ kind: 'folder', directory: folder })}>
          <span className="menu-icon-row">
            <Icon name="folder" />
            <span className="menu-text">
              <span className="menu-title">{projectLabel(folder)}</span>
              <span className="menu-subtitle menu-path">{folder}</span>
            </span>
          </span>
        </leo-menu-item>
      ))}
      <leo-menu-item onClick={() => choose({ kind: 'pick' })} data-test="project-pick">
        <span className="menu-icon-row"><Icon name="folder-open" />Select project…</span>
      </leo-menu-item>
    </ButtonMenu>
  )
}

/**
 * Something else that Escape belongs to: the find bar, or a menu still open. Leo draws a
 * ButtonMenu's list only while it is open, so a list in its shadow root is an open menu.
 */
function openElsewhere(): boolean {
  if (document.querySelector('.find-bar')) return true
  return [...document.querySelectorAll('leo-buttonmenu')].some((menu) => menu.shadowRoot?.querySelector('[role="menu"]'))
}

/** The backend notice, in the tray shape the queue uses, docked on the box it stops. */
export function BackendTray({ onSetup, onCheckBackend, onDiagnostics }: {
  onSetup: () => void
  onCheckBackend: () => void
  onDiagnostics: () => void
}): React.JSX.Element {
  return (
    <div className="composer-tray composer-notice backend-status" role="status">
      <Icon name="warning-triangle-outline" />
      <span className="notice-text"><strong>Backend not set up</strong> · You can browse conversations and prepare drafts.</span>
      <span className="notice-actions">
        <Button size="tiny" kind="plain" onClick={onSetup}>Setup help</Button>
        <Button size="tiny" kind="plain" onClick={onCheckBackend}>Check again</Button>
        <Button size="tiny" kind="plain-faint" onClick={onDiagnostics}>Diagnostics</Button>
      </span>
    </div>
  )
}

/**
 * How full the model's context was at the last request, as a ring.
 *
 * The sentence is kept, in text a screen reader and a driver can find, and in the tooltip: a ring
 * alone says "most of it" and not how much. The fraction is drawn only against a window the
 * catalogue gave; without one the ring is an empty track rather than a guess.
 */
function ContextMeter({ session, model, tokens, archived, compacting }: {
  session: string
  model: string | null
  tokens?: number
  archived?: number
  compacting: boolean
}): React.JSX.Element {
  const size = useContextWindow(session, model, (tokens ?? 0) > 0)
  const said = compacting ? 'Summarising context…'
    : tokens === undefined ? 'Context measurement unavailable'
      : tokens === 0 ? 'Context not yet measured'
        : `${tokens.toLocaleString()} context tokens at last request`
  const fraction = size && tokens ? Math.min(1, tokens / size) : 0
  const share = size && tokens ? `${Math.round(fraction * 100)}% of ${size.toLocaleString()}` : null
  const level = fraction >= 0.95 ? 'full' : fraction >= 0.8 ? 'high' : ''
  const tooltip = [said, share, archived ? 'Earlier context summarised' : null].filter(Boolean).join(' · ')
    + '. The size of the model’s last request, not the tokens used so far.'
  return (
    <span className={`context-meter ${level}`} tabIndex={0} data-tooltip={tooltip} data-test="context-meter">
      <span className="meter-ring" aria-hidden="true">
        <ProgressRing mode={compacting ? 'indeterminate' : 'determinate'} progress={fraction} />
      </span>
      <span className="visually-hidden">
        <span>{said}</span>
        {share && <span> · {share}</span>}
        {!!archived && <span> · Earlier context summarised</span>}
      </span>
    </span>
  )
}
