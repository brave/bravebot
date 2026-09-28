import type { TurnDetails as Details, TurnDisclosure } from '../turn-details'
import { Button, Collapse } from '../nala'

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

export function TurnFooter({ details, onDisclosure, onAudit }: {
  details?: Details
  onDisclosure: (turn: number, field: TurnDisclosure, open: boolean) => void
  onAudit: OpenAudit
}): React.JSX.Element {
  return <div className="turn-footer">
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
    </Collapse> : details ? <span>Final usage unavailable</span> : null}
    <Button size="small" kind="plain" className={`turn-audit-link${details?.clean === false ? ' has-refusal' : ''}`}
      data-audit-turn={details?.turn ?? 'saved'}
      data-test="turn-audit"
      aria-controls="turn-audit-inspector"
      onClick={(event) => onAudit(details?.turn ?? null, event.currentTarget as HTMLButtonElement)}>
      {details?.clean === false ? 'Policy blocked an action' : details ? 'Audit' : 'Audit unavailable'} <span aria-hidden="true">↗</span>
    </Button>
  </div>
}
