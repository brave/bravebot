import { useEffect, useRef, type ReactNode } from 'react'
import { Icon, Input } from '../nala'

/**
 * The search field at the head of a sidebar list, and whatever sits beside it.
 *
 * Always shown rather than behind a toggle: a filter that has to be opened first is one more
 * step every time, and an active query must never be hidden behind a closed control. Escape
 * clears it, the way a search field does everywhere else on the platform.
 */
export function SidebarSearch({ query, onQuery, label, placeholder, children }: {
  query: string
  onQuery: (value: string) => void
  /** The accessible name, which drivers and screen readers find it by. */
  label: string
  placeholder: string
  children?: ReactNode
}): React.JSX.Element {
  const field = useRef<HTMLElement>(null)
  const latest = useRef({ query, onQuery })
  latest.current = { query, onQuery }
  useEffect(() => {
    const host = field.current
    if (!host) return
    const keys = (event: KeyboardEvent): void => {
      if (event.key !== 'Escape' || !latest.current.query) return
      event.preventDefault()
      event.stopPropagation()
      latest.current.onQuery('')
    }
    host.addEventListener('keydown', keys)
    return () => host.removeEventListener('keydown', keys)
  }, [])
  return (
    <div className="sidebar-search-row">
      <Input
        ref={field}
        type="search"
        size="small"
        className="sidebar-search"
        aria-label={label}
        placeholder={placeholder}
        value={query}
        data-test="sidebar-search"
        onInput={({ value }) => onQuery(value)}
        onChange={({ value }) => onQuery(value)}
      >
        <Icon name="search" slot="left-icon" />
      </Input>
      {children}
    </div>
  )
}

/**
 * A full-width row button in the sidebar's head or foot: an icon, a label, and the shortcut
 * that does the same thing, in caption ink on the right.
 */
export function SidebarRow({ icon, label, hint, className, onClick, 'data-test': dataTest }: {
  icon: 'plus-add' | 'settings' | 'plug'
  label: string
  hint?: string
  className?: string
  onClick: () => void
  'data-test'?: string
}): React.JSX.Element {
  return (
    <button type="button" className={`sidebar-row${className ? ` ${className}` : ''}`} onClick={onClick} data-test={dataTest}
      aria-keyshortcuts={hint === '⌘N' ? 'Meta+N' : undefined}>
      <Icon name={icon} />
      <span className="sidebar-row-label">{label}</span>
      {hint && <kbd className="sidebar-row-hint" aria-hidden="true">{hint}</kbd>}
    </button>
  )
}
