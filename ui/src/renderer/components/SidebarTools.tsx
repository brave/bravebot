import { useId, useRef, useState, type ReactNode } from 'react'

/** Keep search optional, while never hiding an active filter. */
export function SidebarTools({ action, children, query, onQuery, label }: {
  action: ReactNode
  children?: ReactNode
  query: string
  onQuery: (value: string) => void
  label: string
}): React.JSX.Element {
  const [expanded, setExpanded] = useState(false)
  const trigger = useRef<HTMLButtonElement>(null)
  const id = useId()
  const close = () => { onQuery(''); setExpanded(false); trigger.current?.focus() }
  return <>
    <div className="sidebar-actions">
      <div className="sidebar-create">{action}</div>
      <button ref={trigger} className="sidebar-search-toggle" aria-label={label} title={label}
        aria-expanded={expanded} aria-controls={id} onClick={() => expanded ? close() : setExpanded(true)}>
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" aria-hidden="true"><circle cx="10.5" cy="10.5" r="6.5" /><path d="m16 16 5 5" /></svg>
      </button>
      {children}
    </div>
    {expanded && <div className="sidebar-search" id={id}>
      <input autoFocus type="search" className="session-find" aria-label={label} placeholder={`${label}…`}
        value={query} onChange={event => onQuery(event.target.value)}
        onKeyDown={event => { if (event.key === 'Escape') { event.stopPropagation(); close() } }} />
      <button aria-label="Close search" title="Clear and close search" onClick={close}>×</button>
    </div>}
  </>
}
