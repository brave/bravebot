import { memo, useEffect, useRef, useState } from 'react'
import { auditDescription, isRefusal, type AuditRecord, type TurnDetails } from '../turn-details'

const Evidence = memo(function Evidence({ record }: { record: AuditRecord }): React.JSX.Element {
  const { title, detail } = auditDescription(record.event)
  const blocked = isRefusal(record.event)
  const label = record.event.label as { integrity?: unknown; confidentiality?: unknown } | null
  return <article className={`audit-event${blocked ? ' audit-refusal' : ''}`}>
    <div className="audit-sequence">Event {record.sequence}{blocked ? ' · Blocked' : ''}</div>
    <strong>{title}</strong>
    {detail && <p>{detail}</p>}
    {label && <dl className="audit-labels">
      {typeof label.integrity === 'string' && <><dt>Integrity</dt><dd>{label.integrity}</dd></>}
      {typeof label.confidentiality === 'string' && <><dt>Confidentiality</dt><dd>{label.confidentiality}</dd></>}
    </dl>}
    <details><summary>Recorded evidence</summary><pre>{JSON.stringify(record.event, null, 2)}</pre></details>
  </article>
})

export function AuditInspector({ details, onClose }: { details?: TurnDetails; onClose: () => void }): React.JSX.Element {
  const close = useRef<HTMLButtonElement>(null)
  const [allOpen, setAllOpen] = useState(false)
  useEffect(() => { close.current?.focus({ preventScroll: true }) }, [])
  const refusals = details?.audit.filter((record) => isRefusal(record.event)) ?? []
  const incomplete = details?.status === 'interrupted' || (details && !details.started)
  return <section className="audit-inspector" id="turn-audit-inspector" aria-label="Turn audit"
    onKeyDown={(event) => { if (event.key === 'Escape') { event.stopPropagation(); onClose() } }}>
    <div className="inspector-title"><strong>Audit · {details ? `Turn ${details.turn}` : 'Saved reply'}</strong>
      <button ref={close} className="audit-close" onClick={onClose} aria-label="Close audit inspector">×</button></div>
    <p className="audit-status">{!details ? 'Saved conversation' : details.status === 'running' ? 'Live · This turn' : incomplete ? 'Capture incomplete' : 'Captured during this session'}</p>
    {!details ? <p>Audit details aren’t available for this saved reply.</p> : <>
      {incomplete && <p>The event stream may be incomplete. Captured evidence is shown below.</p>}
      {details.omitted > 0 && <p className="audit-retention">{details.omitted.toLocaleString()} events omitted by the display retention limit.</p>}
      {refusals.map((record) => <Evidence key={record.sequence} record={record} />)}
      {!refusals.length && (details.clean === false ? <p>A policy refusal was reported, but its detailed evidence is unavailable.</p> :
        details.clean === true ? <p>No policy refusals recorded for this turn.</p> :
          <p>{details.status === 'running' ? 'Waiting for policy decisions. No refusals captured so far.' : 'No refusal summary is available for this turn.'}</p>)}
      <details className="audit-all" onToggle={(event) => setAllOpen(event.currentTarget.open)}><summary>All captured events · {details.audit.length.toLocaleString()}</summary>
        {allOpen && details.audit.map((record) => <Evidence key={record.sequence} record={record} />)}
        {!details.audit.length && <p>No events available.</p>}
      </details>
      <p className="audit-status">Policy decisions describe individual operations.</p>
    </>}
  </section>
}
