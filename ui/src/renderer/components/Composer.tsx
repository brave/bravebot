import { memo, useLayoutEffect, useRef, type RefObject } from 'react'
import type { FileAttachment } from '../../shared/files'
import { useContextWindow } from '../context-window'
import { Button, Icon, ProgressRing, TextArea } from '../nala'
import { FileGlyph } from './FileGlyph'
import { IconButton } from './IconButton'
import { ModelPicker } from './ModelPicker'

/** Where the field stops growing and starts to scroll. */
const FIELD_MAX = 210

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
    draft, onDraft, onSend, onCancel, onModel, attachments, onAttach, onRemoveAttachment, onPreview,
    queued, queuePaused, onResumeQueued, onRemoveQueued, backendReady, onSetup, onCheckBackend, onDiagnostics,
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
      const shorter = draft.length < drafted.current
      drafted.current = draft.length
      if (!shorter && field.scrollHeight <= field.clientHeight && field.style.height) return
      const from = field.offsetHeight
      let to = Math.min(FIELD_MAX, field.scrollHeight)
      if (shorter || !field.style.height) {
        field.style.transition = 'none'
        field.style.height = 'auto'
        to = Math.min(FIELD_MAX, field.scrollHeight)
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

  return (
    <footer className="composer">
      <div className="composer-stack">
        {backendReady === false && <BackendTray onSetup={onSetup} onCheckBackend={onCheckBackend} onDiagnostics={onDiagnostics} />}
        {queued.length > 0 && (
          <div className="composer-tray queued-messages" role="group" aria-label="Queued messages">
            <div className="tray-head">
              <span className="tray-title">{queuePaused ? 'Queue paused' : 'Queued'} <span className="num">· {queued.length}</span></span>
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
          <TextArea ref={input} mode="plain" minRows={1} maxRows={8} value={draft} aria-label="Message the agent"
            placeholder={pending ? 'Draft your next message while you review…' : 'How can I help you today?'}
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
                  else now.onSend()
                }
              }
            }} />
          <div className="composer-toolbar">
            <IconButton icon="attachment" label="Attach files" className="attach-files" onClick={onAttach}
              disabled={attachments.length >= 5} tooltip={attachments.length >= 5 ? 'Five files at most' : 'Attach files'} />
            <span className="toolbar-spacer" />
            <ContextMeter session={session} model={model} tokens={contextTokens} archived={archived} compacting={compacting} />
            <ModelPicker compact session={session} scope={scope} key={session} model={model} disabled={running} onChoose={onModel} />
            <IconButton icon={running ? 'stop-filled' : 'arrow-up'} label={running ? 'Stop' : 'Send'}
              shortcut={running ? '⌘.' : '⌘↩'}
              kind={running ? 'outline' : 'filled'} className={running ? 'send stop' : 'send'}
              onClick={() => { if (running) onCancel(); else onSend() }}
              disabled={!running && !canSend}
              data-test={running ? 'stop-turn' : 'send-message'} />
          </div>
        </div>
      </div>
    </footer>
  )
})

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
