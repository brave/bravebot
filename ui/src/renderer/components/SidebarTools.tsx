import { useId, useRef, useState, type ReactNode } from 'react'
import { Button } from './ui/button'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from './ui/collapsible'
import { InputGroup, InputGroupButton, InputGroupInput } from './ui/input-group'
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip'

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
  return <Collapsible open={expanded} onOpenChange={(open) => open ? setExpanded(true) : close()}>
    <div className="sidebar-actions">
      <div className="sidebar-create">{action}</div>
      <Tooltip><TooltipTrigger asChild><CollapsibleTrigger asChild>
        <Button variant="ghost" size="icon-sm" ref={trigger} className="sidebar-search-toggle" aria-label={label} aria-controls={id}>
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" aria-hidden="true"><circle cx="10.5" cy="10.5" r="6.5" /><path d="m16 16 5 5" /></svg>
        </Button>
      </CollapsibleTrigger></TooltipTrigger><TooltipContent>{label}</TooltipContent></Tooltip>
      {children}
    </div>
    <CollapsibleContent id={id} className="sidebar-search">
      <InputGroup className="h-auto border-0 bg-transparent shadow-none dark:bg-transparent">
        <InputGroupInput autoFocus type="search" className="session-find" aria-label={label} placeholder={`${label}…`}
          value={query} onChange={event => onQuery(event.target.value)}
          onKeyDown={event => { if (event.key === 'Escape') { event.stopPropagation(); close() } }} />
        <Tooltip><TooltipTrigger asChild><InputGroupButton size="icon-sm" aria-label="Close search" onClick={close}>×</InputGroupButton></TooltipTrigger>
          <TooltipContent>Clear and close search</TooltipContent></Tooltip>
      </InputGroup>
    </CollapsibleContent>
  </Collapsible>
}
