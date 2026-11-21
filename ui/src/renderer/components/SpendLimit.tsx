import { useEffect, useRef, useState } from 'react'
import type { SessionLimit } from '../../shared/protocol'
import { Button, Icon, Input } from '../nala'
import { call } from '../rpc'
import { Modal } from './Modal'

const compact = new Intl.NumberFormat(undefined, { notation: 'compact', maximumFractionDigits: 1 })

/** `1.2K / 50K`, with the unit named only when it is credits, which a bare count would not say. */
export function spentAgainst({ limit, spent }: SessionLimit): string | null {
  if (!limit) return null
  const figures = `${compact.format(spent)} / ${compact.format(limit.count)}`
  return limit.unit === 'credits' ? `${figures} credits` : figures
}

/** The sentence the tooltip and the accessible name give. */
export function spendSentence({ limit, spent }: SessionLimit): string {
  if (!limit) return `No spend limit. ${spent.toLocaleString()} tokens spent`
  return `Spend limit: ${spent.toLocaleString()} of ${limit.count.toLocaleString()} ${limit.unit} spent`
}

/**
 * The session's spend limit (TURN-8), in the composer toolbar: what the session has spent against
 * the limit, and a dialog to set or remove it.
 *
 * The figures are what `session.limit` answers, read again when the session changes, when a turn
 * ends and when a question is answered, since the limit question can lift or change the limit in
 * the middle of a turn. A limit set while a turn runs applies to that turn's next request.
 */
export function SpendLimit({ session, running, pending }: {
  session: string
  running: boolean
  pending: boolean
}): React.JSX.Element | null {
  const [figures, setFigures] = useState<SessionLimit | null>(null)
  const [editing, setEditing] = useState(false)
  const trigger = useRef<HTMLElement>(null)

  useEffect(() => {
    let current = true
    call<SessionLimit>('session.limit', { session })
      .then((read) => { if (current) setFigures(read) })
      // A session the bridge does not hold yet (a bot's first page) has no limit to show.
      .catch(() => { if (current) setFigures(null) })
    return () => { current = false }
  }, [session, running, pending])

  const shown = figures ? spentAgainst(figures) : null
  const sentence = figures ? spendSentence(figures) : ''
  const reached = !!figures?.limit && figures.spent >= figures.limit.count

  // Leo's React wrapper sets these once, when the host is made.
  useEffect(() => {
    const button = trigger.current
    if (!button) return
    button.setAttribute('aria-label', sentence)
    button.setAttribute('data-tooltip', sentence)
    button.setAttribute('data-limit', figures?.limit ? `${figures.limit.count} ${figures.limit.unit}` : 'none')
  }, [sentence, figures])

  if (!figures) return null

  return (
    <>
      <Button ref={trigger} kind="plain-faint" size="tiny"
        className={`spend-limit${reached ? ' reached' : ''}`} data-test="spend-limit"
        data-limit={figures.limit ? `${figures.limit.count} ${figures.limit.unit}` : 'none'}
        aria-label={sentence} data-tooltip={sentence} onClick={() => setEditing(true)}>
        <Icon name="coins" slot="icon-before" />
        {shown && <span className="spend-limit-figures">{shown}</span>}
      </Button>
      {editing && <SpendLimitDialog session={session} figures={figures}
        onDone={(read) => { if (read) setFigures(read); setEditing(false) }} />}
    </>
  )
}

/** Set or remove the limit. What the bridge refuses (a figure it cannot read) is said under the field. */
export function SpendLimitDialog({ session, figures, onDone }: {
  session: string
  figures: SessionLimit
  /** With what the bridge answered when the limit changed, or nothing when it was left alone. */
  onDone: (read: SessionLimit | null) => void
}): React.JSX.Element {
  const [text, setText] = useState(figures.limit
    ? (figures.limit.unit === 'credits' ? `${figures.limit.count} credits` : String(figures.limit.count))
    : '')
  const [busy, setBusy] = useState(false)
  const [refused, setRefused] = useState<string | null>(null)

  const ask = async (limit: string | null) => {
    if (busy) return
    setBusy(true)
    try {
      onDone(await call<SessionLimit>('session.limit', { session, limit }))
    } catch (error) {
      setRefused(error instanceof Error ? error.message : String(error))
      setBusy(false)
    }
  }

  return (
    <Modal title="Spend limit" size="sm" compact className="spend-limit-dialog"
      subtitle={`When this chat has spent this much, it asks before the next request. ${figures.spent.toLocaleString()} tokens spent so far.`}
      onClose={busy ? undefined : () => onDone(null)} actions={<>
        {figures.limit && (
          <Button kind="plain-faint" size="small" className="modal-leading" isDisabled={busy}
            onClick={() => void ask(null)} data-test="spend-limit-remove">Remove limit</Button>
        )}
        <Button kind="plain-faint" size="small" isDisabled={busy} onClick={() => onDone(null)} data-test="spend-limit-cancel">
          Cancel
        </Button>
        <Button kind="filled" size="small" isDisabled={busy || !text.trim()} onClick={() => void ask(text)} data-test="spend-limit-set">
          Set
        </Button>
      </>}>
      <Input value={text} placeholder="50k, 2m or 20 credits" disabled={busy} data-test="spend-limit-field"
        onInput={(event: { value?: unknown }) => { if (typeof event.value === 'string') { setText(event.value); setRefused(null) } }}>
        Limit in tokens or credits
      </Input>
      {refused && <p className="spend-limit-refused" role="alert" data-test="spend-limit-refused">{refused}</p>}
    </Modal>
  )
}
