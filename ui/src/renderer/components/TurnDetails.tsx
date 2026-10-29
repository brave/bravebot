import type { TurnDetails as Details, TurnDisclosure } from '../turn-details'
import { Collapse } from '../nala'
import { CopyButton } from './CopyButton'
import { IconButton } from './IconButton'

export type OpenAudit = (turn: number | null, trigger: HTMLButtonElement) => void

export function TurnNotices({ details, onDisclosure }: {
  details?: Details
  onDisclosure: (turn: number, field: TurnDisclosure, open: boolean) => void
}): React.JSX.Element | null {
  if (!details?.notices.length) return null
  return <Collapse className="turn-notices" isOpen={details.noticesOpen}
    title={`Turn notices · ${details.notices.length}`}
    onToggle={({ open }) => { if (open !== details.noticesOpen) onDisclosure(details.turn, 'noticesOpen', open) }}
    data-test="turn-notices">
    <ul>{details.notices.map((notice, index) => <li key={index}>{notice}</li>)}</ul>
  </Collapse>
}

const exact = (value?: number): string => value === undefined ? 'Unavailable' : value.toLocaleString()
const compact = new Intl.NumberFormat(undefined, { notation: 'compact', maximumFractionDigits: 1 })

/**
 * The line under a reply: copy it, the way into its audit, and what it cost.
 *
 * Quiet until the row is hovered or is the last reply, except when a policy refused something in
 * the turn, which stays in view because it is the one thing here a reader has to know. The model
 * line follows the audit control. Its tooltip holds the exact counts.
 */
export function TurnFooter({ details, onAudit, copy, undo }: {
  details?: Details
  onAudit: OpenAudit
  /** The reply's own text, for the Copy button. Absent under an error, which has its own details. */
  copy?: string
  /** Offered on the latest turn's footer while the session can be put back to before it. */
  undo?: { running: boolean; onUndo: () => void }
}): React.JSX.Element {
  const blocked = details?.clean === false
  const label = blocked ? 'Policy blocked an action' : details ? 'Audit' : 'Audit unavailable'
  return <div className={`turn-footer${blocked ? ' has-refusal' : ''}`}>
    {copy !== undefined && <CopyButton text={() => copy} label="Copy message" data-test="copy-message" />}
    {undo && <IconButton icon="arrow-undo" label="Undo turn" size="tiny" disabled={undo.running}
      tooltip={undo.running ? 'Wait for the turn to finish' : 'Undo turn · Put back its files and conversation'}
      data-test="turn-undo" onClick={undo.onUndo} />}
    {blocked && <span className="turn-refusal" aria-hidden="true">Policy blocked an action</span>}
    <IconButton icon={blocked ? 'shield-alert' : 'shield-done'} label={label}
      tooltip={blocked ? 'Policy blocked an action · Open the audit' : details ? 'Audit this turn' : 'Audit unavailable'}
      size="tiny"
      className={`turn-audit-link${blocked ? ' has-refusal' : ''}`}
      dataset={{ 'audit-turn': details?.turn ?? 'saved' }}
      data-test="turn-audit"
      controls="turn-audit-inspector"
      onClick={(event) => onAudit(details?.turn ?? null, event.currentTarget as HTMLButtonElement)} />
    {details?.status === 'complete' ? <Usage details={details} /> : details ? <span className="turn-usage-missing">Final usage unavailable</span> : null}
  </div>
}

/** The model and a compact total. A click opens the exact counts; a screen reader can read them without one. */
function Usage({ details }: { details: Details }): React.JSX.Element {
  const shown = `${details.model ?? 'Model unavailable'} · ${details.tokens === undefined ? 'Usage unavailable' : `${compact.format(details.tokens)} tokens`}`
  const tooltip = [
    `Turn ${details.turn}`,
    `Model used · ${details.model ?? 'Unavailable'}`,
    ...(details.service ? [`Answered by · ${details.service}`] : []),
    `Total tokens · ${exact(details.tokens)}`,
    `Output tokens · ${exact(details.outputTokens)}`,
    `Tool-calling rounds · ${exact(details.steps)}`,
    'Usage is summed across requests in this turn',
  ].join('\n')
  return <button type="button" className="turn-usage" data-tooltip={tooltip} data-tooltip-trigger="click" data-test="turn-usage">
    <span className="turn-usage-line">{shown}</span>
    <span className="visually-hidden">{tooltip}</span>
  </button>
}
