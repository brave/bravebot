import { memo, useEffect, useRef, useState } from 'react'
import { auditDescription, isRefusal, type AuditRecord, type TurnDetails } from '../turn-details'
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from './ui/accordion'
import { Alert, AlertDescription } from './ui/alert'
import { Badge } from './ui/badge'
import { Button } from './ui/button'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from './ui/collapsible'
import { Empty, EmptyDescription } from './ui/empty'
import { Item, ItemGroup } from './ui/item'
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip'

const Evidence = memo(function Evidence({ record }: { record: AuditRecord }): React.JSX.Element {
  const { title, detail } = auditDescription(record.event)
  const blocked = isRefusal(record.event)
  const label = record.event.label as { integrity?: unknown; confidentiality?: unknown } | null
  return <Item asChild variant="outline">
    <article className={`audit-event block p-3${blocked ? ' audit-refusal' : ''}`}>
      <div className="audit-sequence"><Badge variant={blocked ? 'destructive' : 'outline'}>Event {record.sequence}{blocked ? ' · Blocked' : ''}</Badge></div>
      <strong>{title}</strong>
      {detail && <p>{detail}</p>}
      {label && <dl className="audit-labels">
        {typeof label.integrity === 'string' && <><dt>Integrity</dt><dd>{label.integrity}</dd></>}
        {typeof label.confidentiality === 'string' && <><dt>Confidentiality</dt><dd>{label.confidentiality}</dd></>}
      </dl>}
      <Accordion type="single" collapsible>
        <AccordionItem value="evidence" className="border-0">
          <AccordionTrigger className="py-1 text-xs">Recorded evidence</AccordionTrigger>
          <AccordionContent className="pb-0"><pre>{JSON.stringify(record.event, null, 2)}</pre></AccordionContent>
        </AccordionItem>
      </Accordion>
    </article>
  </Item>
})

function AuditEmpty({ children }: { children: React.ReactNode }): React.JSX.Element {
  return <Empty className="min-h-0 flex-none gap-0 border-0 p-0 text-left">
    <EmptyDescription className="w-full text-left text-inherit">{children}</EmptyDescription>
  </Empty>
}

export function AuditInspector({ details, onClose }: { details?: TurnDetails; onClose: () => void }): React.JSX.Element {
  const close = useRef<HTMLButtonElement>(null)
  const [allOpen, setAllOpen] = useState(false)
  useEffect(() => { close.current?.focus({ preventScroll: true }) }, [])
  const refusals = details?.audit.filter((record) => isRefusal(record.event)) ?? []
  const incomplete = details?.status === 'interrupted' || (details && !details.started)
  return <section className="audit-inspector flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto px-3.5 pb-4" id="turn-audit-inspector" aria-label="Turn audit"
      onKeyDown={(event) => { if (event.key === 'Escape') { event.stopPropagation(); onClose() } }}>
      <div className="inspector-title flex items-center justify-between gap-2"><strong>Audit · {details ? `Turn ${details.turn}` : 'Saved reply'}</strong>
        <Tooltip><TooltipTrigger asChild>
          <Button variant="ghost" size="icon-sm" ref={close} className="audit-close" onClick={onClose} aria-label="Close audit inspector">×</Button>
        </TooltipTrigger><TooltipContent>Close audit inspector</TooltipContent></Tooltip></div>
      <p className="audit-status"><Badge variant="outline">{!details ? 'Saved conversation' : details.status === 'running' ? 'Live · This turn' : incomplete ? 'Capture incomplete' : 'Captured during this session'}</Badge></p>
      {!details ? <AuditEmpty>Audit details aren’t available for this saved reply.</AuditEmpty> : <>
        {incomplete && <Alert><AlertDescription>The event stream may be incomplete. Captured evidence is shown below.</AlertDescription></Alert>}
        {details.omitted > 0 && <Alert className="audit-retention"><AlertDescription className="text-inherit">{details.omitted.toLocaleString()} events omitted by the display retention limit.</AlertDescription></Alert>}
        <ItemGroup>{refusals.map((record) => <Evidence key={record.sequence} record={record} />)}</ItemGroup>
        {!refusals.length && (details.clean === false ? <Alert variant="destructive"><AlertDescription>A policy refusal was reported, but its detailed evidence is unavailable.</AlertDescription></Alert> :
          details.clean === true ? <AuditEmpty>No policy refusals recorded for this turn.</AuditEmpty> :
            <AuditEmpty>{details.status === 'running' ? 'Waiting for policy decisions. No refusals captured so far.' : 'No refusal summary is available for this turn.'}</AuditEmpty>)}
        <Collapsible className="audit-all" open={allOpen} onOpenChange={setAllOpen}>
          <CollapsibleTrigger className="flex h-auto w-full items-center justify-start gap-1 px-0 py-1 text-xs"><span className={`chevron ${allOpen ? 'open' : ''}`} aria-hidden="true">›</span>All captured events · {details.audit.length.toLocaleString()}</CollapsibleTrigger>
          <CollapsibleContent>
            {allOpen && <ItemGroup>{details.audit.map((record) => <Evidence key={record.sequence} record={record} />)}</ItemGroup>}
            {!details.audit.length && <AuditEmpty>No events available.</AuditEmpty>}
          </CollapsibleContent>
        </Collapsible>
        <p className="audit-status">Policy decisions describe individual operations.</p>
      </>}
    </section>
}
