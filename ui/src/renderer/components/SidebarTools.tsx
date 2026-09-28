import { useId, useRef, useState, type ReactNode } from 'react'
import { Icon, Input } from '../nala'

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
        aria-expanded={expanded} aria-controls={id} data-test="sidebar-search-toggle"
        onClick={() => expanded ? close() : setExpanded(true)}>
        <Icon name="search" style={{ '--leo-icon-size': '16px' } as React.CSSProperties} />
      </button>
      {children}
    </div>
    {expanded && <div className="sidebar-search" id={id}>
      <Input autofocus type="search" className="session-find" aria-label={label} placeholder={`${label}…`}
        value={query} data-test="sidebar-search"
        onChange={({ value }) => onQuery(value)}
        onKeyDown={({ innerEvent }) => {
          if ((innerEvent as unknown as KeyboardEvent).key === 'Escape') {
            innerEvent.stopPropagation()
            close()
          }
        }} />
      <button aria-label="Close search" title="Clear and close search" onClick={close}>
        <Icon name="close" style={{ '--leo-icon-size': '14px' } as React.CSSProperties} />
      </button>
    </div>}
  </>
}
