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
 * The line under a reply: copy it, what it cost, and the way into its audit.
 *
 * Quiet until the row is hovered or is the last reply, except when a policy refused something in
 * the turn, which stays in view because it is the one thing here a reader has to know.
 */
export function TurnFooter({ details, onDisclosure, onAudit, copy, undo }: {
  details?: Details
  onDisclosure: (turn: number, field: TurnDisclosure, open: boolean) => void
  onAudit: OpenAudit
  /** The reply's own text, for the Copy button. Absent under an error, which has its own details. */
  copy?: string
  /** Offered on the latest turn's footer while the session can be put back to before it. */
  undo?: { running: boolean; onUndo: () => void }
}): React.JSX.Element {
  const blocked = details?.clean === false
  const label = blocked ? 'Policy blocked an action' : details ? 'Audit' : 'Audit unavailable'
  return <div className={`turn-footer${blocked ? ' has-refusal' : ''}${details?.statsOpen ? ' stats-open' : ''}`}>
    {copy !== undefined && <CopyButton text={() => copy} label="Copy message" data-test="copy-message" />}
    {undo && <IconButton icon="arrow-undo" label="Undo turn" size="tiny" disabled={undo.running}
      tooltip={undo.running ? 'Wait for the turn to finish' : 'Undo turn · Put back its files and conversation'}
      data-test="turn-undo" onClick={undo.onUndo} />}
    {details?.status === 'complete' ? <Collapse className="turn-statistics" isOpen={details.statsOpen}
      title={`${details.model ?? 'Model unavailable'} · ${details.tokens === undefined ? 'Usage unavailable' : `${compact.format(details.tokens)} tokens`}`}
      onToggle={({ open }) => { if (open !== details.statsOpen) onDisclosure(details.turn, 'statsOpen', open) }}
      data-test="turn-statistics">
      <div className="turn-statistics-body"><strong>Turn {details.turn}</strong><dl>
        <dt>Model used</dt><dd>{details.model ?? 'Unavailable'}</dd>
        <dt>Total tokens</dt><dd>{exact(details.tokens)}</dd>
        <dt>Output tokens</dt><dd>{exact(details.outputTokens)}</dd>
        <dt>Tool-calling rounds</dt><dd>{exact(details.steps)}</dd>
      </dl><p>Usage is summed across requests in this turn.</p></div>
    </Collapse> : details ? <span className="turn-usage-missing">Final usage unavailable</span> : null}
    <IconButton icon={blocked ? 'shield-alert' : 'shield-done'} label={label}
      tooltip={blocked ? 'Policy blocked an action · Open the audit' : details ? 'Audit this turn' : 'Audit unavailable'}
      size="tiny"
      className={`turn-audit-link${blocked ? ' has-refusal' : ''}`}
      dataset={{ 'audit-turn': details?.turn ?? 'saved' }}
      data-test="turn-audit"
      controls="turn-audit-inspector"
      onClick={(event) => onAudit(details?.turn ?? null, event.currentTarget as HTMLButtonElement)} />
    {blocked && <span className="turn-refusal" aria-hidden="true">Policy blocked an action</span>}
  </div>
}
