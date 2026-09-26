import type { ReactNode } from 'react'
import { SearchIcon } from 'lucide-react'
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
} from '@/components/ui/input-group'

/**
 * The create action and the filter box that share a row at the top of a sidebar list.
 *
 * The filter stays mounted even when empty: drive scripts assert `.session-find` survives a
 * tab switch, which is how they know the list was hidden rather than thrown away.
 */
export function SidebarTools({
  action,
  children,
  query,
  onQuery,
  label,
  findClass = 'session-find',
}: {
  action: ReactNode
  children?: ReactNode
  query: string
  onQuery: (value: string) => void
  label: string
  /** Class hook for drive scripts. Sessions use `session-find`; bots use `bot-find`. */
  findClass?: string
}): React.JSX.Element {
  return (
    <div className="sidebar-actions flex flex-col gap-2">
      <div className="flex items-stretch gap-1">
        <div className="sidebar-create min-w-0 flex-1">{action}</div>
        {children}
      </div>
      <InputGroup>
        <InputGroupAddon>
          <SearchIcon />
        </InputGroupAddon>
        {/* One shape for "type to narrow this list" wherever the window offers it — the file
            tree's own filter is the same box at the size that column speaks in. */}
        <InputGroupInput
          className={`${findClass} min-w-0 flex-1 text-xs`}
          type="search"
          title={label}
          aria-label={label}
          placeholder={`${label}…`}
          value={query}
          onChange={(event) => onQuery(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Escape' && query) {
              event.stopPropagation()
              onQuery('')
            }
          }}
        />
      </InputGroup>
    </div>
  )
}
