import { memo, useEffect, useRef, useState } from 'react'
import { auditDescription, isRefusal, type AuditRecord, type TurnDetails } from '../turn-details'
import { Collapse } from '../nala'
import { IconButton } from './IconButton'

const Evidence = memo(function Evidence({ record }: { record: AuditRecord }): React.JSX.Element {
  const { title, detail } = auditDescription(record.event)
  const blocked = isRefusal(record.event)
  const label = record.event.label as { integrity?: unknown; confidentiality?: unknown } | null
  return <li className={`audit-event${blocked ? ' audit-refusal' : ''}`}>
    <span className="audit-dot" aria-hidden="true" />
    <div className="audit-sequence num">Event {record.sequence}{blocked ? ' · Blocked' : ''}</div>
    <strong>{title}</strong>
    {detail && <p>{detail}</p>}
    {label && <dl className="audit-labels">
      {typeof label.integrity === 'string' && <><dt>Integrity</dt><dd>{label.integrity}</dd></>}
      {typeof label.confidentiality === 'string' && <><dt>Confidentiality</dt><dd>{label.confidentiality}</dd></>}
    </dl>}
    <Collapse className="flat-collapse audit-evidence" isOpen={undefined}>
      <span slot="title" className="audit-collapse-title">Recorded evidence</span>
      <pre>{JSON.stringify(record.event, null, 2)}</pre>
    </Collapse>
  </li>
})

export function AuditInspector({ details, onClose }: { details?: TurnDetails; onClose: () => void }): React.JSX.Element {
  const close = useRef<HTMLElement>(null)
  const all = useRef<HTMLElement>(null)
  const [allOpen, setAllOpen] = useState(false)
  useEffect(() => { close.current?.focus({ preventScroll: true }) }, [])
  // Leo rebuilds a collapse's summary when what is slotted into it changes, which is what opening
  // this one does, and the summary being read from goes with it. Put focus back on the new one.
  useEffect(() => {
    const frame = requestAnimationFrame(() => {
      if (document.activeElement === document.body) all.current?.shadowRoot?.querySelector<HTMLElement>('summary')?.focus({ preventScroll: true })
    })
    return () => cancelAnimationFrame(frame)
  }, [allOpen])
  const refusals = details?.audit.filter((record) => isRefusal(record.event)) ?? []
  const incomplete = details?.status === 'interrupted' || (details && !details.started)
  return <section className="audit-inspector" id="turn-audit-inspector" aria-label="Turn audit"
    onKeyDown={(event) => { if (event.key === 'Escape') { event.stopPropagation(); onClose() } }}>
    <div className="inspector-title">
      <IconButton ref={close} icon="arrow-left" label="Close audit inspector" tooltip="Back to context" className="audit-close"
        onClick={onClose} data-test="audit-close" />
      <strong>Audit · {details ? `Turn ${details.turn}` : 'Saved reply'}</strong>
    </div>
    <p className="audit-status">{!details ? 'Saved conversation' : details.status === 'running' ? 'Live · This turn' : incomplete ? 'Capture incomplete' : 'Captured during this session'}</p>
    {!details ? <p>Audit details aren’t available for this saved reply.</p> : <>
      {incomplete && <p>The event stream may be incomplete. Captured evidence is shown below.</p>}
      {details.omitted > 0 && <p className="audit-retention">{details.omitted.toLocaleString()} events omitted by the display retention limit.</p>}
      {refusals.length > 0 && <ol className="audit-timeline">{refusals.map((record) => <Evidence key={record.sequence} record={record} />)}</ol>}
      {!refusals.length && (details.clean === false ? <p>A policy refusal was reported, but its detailed evidence is unavailable.</p> :
        details.clean === true ? <p>No policy refusals recorded for this turn.</p> :
          <p>{details.status === 'running' ? 'Waiting for policy decisions. No refusals captured so far.' : 'No refusal summary is available for this turn.'}</p>)}
      <Collapse ref={all} className="flat-collapse audit-all" isOpen={allOpen}
        onToggle={({ open }) => setAllOpen(open)} data-test="audit-all">
        <span slot="title" className="audit-collapse-title">All captured events <span className="num">· {details.audit.length.toLocaleString()}</span></span>
        {allOpen && details.audit.length > 0 && <ol className="audit-timeline">{details.audit.map((record) => <Evidence key={record.sequence} record={record} />)}</ol>}
        {!details.audit.length && <p>No events available.</p>}
      </Collapse>
      <p className="audit-status">Policy decisions describe individual operations.</p>
    </>}
  </section>
}
