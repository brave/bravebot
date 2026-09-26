import { memo, useEffect, useRef, useState } from 'react'
import { ChevronRightIcon, XIcon } from 'lucide-react'
import { cn } from '@/lib/utils'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import { Empty, EmptyDescription } from '@/components/ui/empty'
import { auditDescription, isRefusal, type AuditRecord, type TurnDetails } from '../turn-details'

const Evidence = memo(function Evidence({ record }: { record: AuditRecord }): React.JSX.Element {
  const { title, detail } = auditDescription(record.event)
  const blocked = isRefusal(record.event)
  const label = record.event.label as { integrity?: unknown; confidentiality?: unknown } | null
  return (
    <Card className={cn('audit-event rounded-lg bg-background', blocked && 'audit-refusal ring-warning')} size="sm">
      <CardHeader className="flex-row items-center justify-between gap-2">
        <div className={cn('audit-sequence text-muted-foreground', blocked && 'text-warning')}>
          Event {record.sequence}
          {blocked ? ' · Blocked' : ''}
        </div>
        {blocked && <Badge variant="destructive">Blocked</Badge>}
      </CardHeader>
      <CardContent className="flex flex-col gap-2">
        <CardTitle>{title}</CardTitle>
        {detail && <p>{detail}</p>}
        {label && (
          <dl className="audit-labels grid grid-cols-2 gap-x-2 gap-y-1">
            {typeof label.integrity === 'string' && <><dt className="text-muted-foreground">Integrity</dt><dd>{label.integrity}</dd></>}
            {typeof label.confidentiality === 'string' && <><dt className="text-muted-foreground">Confidentiality</dt><dd>{label.confidentiality}</dd></>}
          </dl>
        )}
        <Collapsible className="group/evidence">
          {/* A finger is a blunter instrument than a pointer, so every disclosure in here is at
              least a finger tall on a machine driven by one. */}
          <CollapsibleTrigger className="flex items-center gap-1 text-left pointer-coarse:min-h-11">
            <ChevronRightIcon className="size-3! transition-transform motion-reduce:transition-none group-data-open/evidence:rotate-90" />
            Recorded evidence
          </CollapsibleTrigger>
          <CollapsibleContent>
            <pre className="rounded-[5px] bg-code p-2 font-mono text-[11px]/[1.6] whitespace-pre-wrap wrap-anywhere">{JSON.stringify(record.event, null, 2)}</pre>
          </CollapsibleContent>
        </Collapsible>
      </CardContent>
    </Card>
  )
})

export function AuditInspector({ details, onClose }: { details?: TurnDetails; onClose: () => void }): React.JSX.Element {
  const close = useRef<HTMLButtonElement>(null)
  const [allOpen, setAllOpen] = useState(false)
  useEffect(() => { close.current?.focus({ preventScroll: true }) }, [])
  const refusals = details?.audit.filter((record) => isRefusal(record.event)) ?? []
  const incomplete = details?.status === 'interrupted' || (details && !details.started)
  return (
    <section
      className="audit-inspector flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto p-3.5 text-xs wrap-anywhere"
      id="turn-audit-inspector"
      aria-label="Turn audit"
      onKeyDown={(event) => {
        if (event.key === 'Escape') {
          event.stopPropagation()
          onClose()
        }
      }}
    >
      <div className="inspector-title flex items-center justify-between gap-2 text-[13px]">
        <strong>Audit · {details ? `Turn ${details.turn}` : 'Saved reply'}</strong>
        <Button
          ref={close}
          variant="ghost"
          size="icon-sm"
          className="audit-close flex-none text-muted-foreground pointer-coarse:min-h-11 pointer-coarse:min-w-11"
          onClick={onClose}
          aria-label="Close audit inspector"
        >
          <XIcon />
        </Button>
      </div>
      <p className="audit-status text-muted-foreground">
        {!details
          ? 'Saved conversation'
          : details.status === 'running'
            ? 'Live · This turn'
            : incomplete
              ? 'Capture incomplete'
              : 'Captured during this session'}
      </p>
      {!details ? (
        <Empty className="min-h-0 flex-none items-start gap-0 border-0 p-0 text-left">
          <EmptyDescription className="text-left text-inherit">
            Audit details aren’t available for this saved reply.
          </EmptyDescription>
        </Empty>
      ) : (
        <div className="flex flex-col gap-3">
          {incomplete && (
            <Alert>
              <AlertDescription>The event stream may be incomplete. Captured evidence is shown below.</AlertDescription>
            </Alert>
          )}
          {details.omitted > 0 && (
            <Alert className="audit-retention">
              <AlertDescription className="text-warning">
                {details.omitted.toLocaleString()} events omitted by the display retention limit.
              </AlertDescription>
            </Alert>
          )}
          {refusals.map((record) => <Evidence key={record.sequence} record={record} />)}
          {!refusals.length && (
            details.clean === false ? (
              <Alert variant="destructive">
                <AlertTitle>Policy refusal</AlertTitle>
                <AlertDescription>A policy refusal was reported, but its detailed evidence is unavailable.</AlertDescription>
              </Alert>
            ) : details.clean === true ? (
              <Empty className="min-h-0 flex-none items-start gap-0 border-0 p-0 text-left">
                <EmptyDescription className="text-left text-inherit">No policy refusals recorded for this turn.</EmptyDescription>
              </Empty>
            ) : (
              <p>
                {details.status === 'running'
                  ? 'Waiting for policy decisions. No refusals captured so far.'
                  : 'No refusal summary is available for this turn.'}
              </p>
            )
          )}
          <Collapsible className="audit-all border-t border-border pt-2" open={allOpen} onOpenChange={setAllOpen}>
            <CollapsibleTrigger className="flex w-full items-center gap-1 text-left pointer-coarse:min-h-11">
              <ChevronRightIcon className={cn('size-3! transition-transform motion-reduce:transition-none', allOpen && 'rotate-90')} />
              All captured events · {details.audit.length.toLocaleString()}
            </CollapsibleTrigger>
            <CollapsibleContent className="flex flex-col gap-2">
              {allOpen && details.audit.map((record) => <Evidence key={record.sequence} record={record} />)}
              {!details.audit.length && (
                <Empty className="min-h-0 flex-none items-start gap-0 border-0 p-0 text-left">
                  <EmptyDescription className="text-left text-inherit">No events available.</EmptyDescription>
                </Empty>
              )}
            </CollapsibleContent>
          </Collapsible>
          <p className="audit-status text-muted-foreground">Policy decisions describe individual operations.</p>
        </div>
      )}
    </section>
  )
}
