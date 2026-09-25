import { cn } from 'cn'
import type { TurnDetails as Details, TurnDisclosure } from '../turn-details'
import { Badge } from './ui/badge'
import { Button } from './ui/button'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from './ui/collapsible'
import { Item } from './ui/item'

export type OpenAudit = (turn: number | null, trigger: HTMLButtonElement) => void

export function TurnNotices({ details, onDisclosure }: {
  details?: Details
  onDisclosure: (turn: number, field: TurnDisclosure, open: boolean) => void
}): React.JSX.Element | null {
  if (!details?.notices.length) return null
  return <Collapsible className="turn-notices" open={details.noticesOpen}
    onOpenChange={(open) => { if (open !== details.noticesOpen) onDisclosure(details.turn, 'noticesOpen', open) }}>
    <CollapsibleTrigger asChild><Button variant="ghost" className="h-auto w-full justify-start px-0 py-1 text-xs"><span className={cn('chevron', details.noticesOpen && 'open')} aria-hidden="true">›</span>Turn notices · <Badge variant="secondary">{details.notices.length}</Badge></Button></CollapsibleTrigger>
    <CollapsibleContent forceMount>
      <ul className="m-0 flex list-none flex-col gap-px p-0">{details.notices.map((notice, index) => <Item asChild size="sm" key={index}><li className="block border-0 px-0 py-1.5 text-xs">{notice}</li></Item>)}</ul>
    </CollapsibleContent>
  </Collapsible>
}

const exact = (value?: number): string => value === undefined ? 'Unavailable' : value.toLocaleString()
const compact = new Intl.NumberFormat(undefined, { notation: 'compact', maximumFractionDigits: 1 })

export function TurnFooter({ details, onDisclosure, onAudit }: {
  details?: Details
  onDisclosure: (turn: number, field: TurnDisclosure, open: boolean) => void
  onAudit: OpenAudit
}): React.JSX.Element {
  return <div className="turn-footer">
    {details?.status === 'complete' ? <Collapsible className="turn-statistics" open={details.statsOpen}
      onOpenChange={(open) => { if (open !== details.statsOpen) onDisclosure(details.turn, 'statsOpen', open) }}>
      <CollapsibleTrigger asChild><Button variant="ghost" className="h-auto max-w-full justify-start px-0 py-1 text-left text-xs whitespace-normal"><span className={cn('chevron', details.statsOpen && 'open')} aria-hidden="true">›</span>{details.model ?? 'Model unavailable'} · {details.tokens === undefined ? 'Usage unavailable' : `${compact.format(details.tokens)} tokens`}</Button></CollapsibleTrigger>
      <CollapsibleContent forceMount><div className="turn-statistics-body"><strong>Turn {details.turn}</strong><dl>
          <dt>Model used</dt><dd>{details.model ?? 'Unavailable'}</dd>
          <dt>Total tokens</dt><dd>{exact(details.tokens)}</dd>
          <dt>Output tokens</dt><dd>{exact(details.outputTokens)}</dd>
          <dt>Tool-calling rounds</dt><dd>{exact(details.steps)}</dd>
        </dl><p>Usage is summed across requests in this turn.</p></div></CollapsibleContent>
    </Collapsible> : details ? <Badge variant="outline">Final usage unavailable</Badge> : null}
    <Button variant="link" className={cn('turn-audit-link h-auto min-h-0 px-0 py-1 text-left text-xs text-muted-foreground hover:text-foreground', details?.clean === false && 'has-refusal text-warn')}
      data-audit-turn={details?.turn ?? 'saved'}
      aria-controls="turn-audit-inspector" onClick={(event) => onAudit(details?.turn ?? null, event.currentTarget)}>
      {details?.clean === false ? 'Policy blocked an action' : details ? 'Audit' : 'Audit unavailable'} <span aria-hidden="true">↗</span>
    </Button>
  </div>
}
