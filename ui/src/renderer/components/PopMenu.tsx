import { Fragment, type ReactElement, useEffect, useRef } from 'react'
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from './ui/dropdown-menu'

export interface PopItem {
  id: string
  label: string
  detail?: string
  enabled?: boolean
  checked?: boolean
  separated?: boolean
}

/** An anchored application menu backed by the shadcn DropdownMenu primitive. */
export function PopMenu({
  open,
  trigger,
  items,
  label,
  onChoose,
  onOpenChange,
}: {
  open: boolean
  trigger: ReactElement
  items: readonly PopItem[]
  label: string
  onChoose: (id: string) => void
  onOpenChange: (open: boolean) => void
}): React.JSX.Element {
  const checkable = items.some((item) => item.checked !== undefined)
  const content = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (!open) return
    const frame = requestAnimationFrame(() => content.current?.querySelector<HTMLElement>('[role="menuitem"]:not([data-disabled]), [role="menuitemcheckbox"]:not([data-disabled])')?.focus())
    return () => cancelAnimationFrame(frame)
  }, [open])

  return <DropdownMenu open={open} onOpenChange={onOpenChange}>
    <DropdownMenuTrigger asChild>{trigger}</DropdownMenuTrigger>
    <DropdownMenuContent ref={content} className={`popmenu ${checkable ? 'checkable' : ''} min-w-48`} align="start" aria-label={label}>
      <DropdownMenuGroup>
        {items.map((entry) => {
          const content = <>
            <span className="popitem-label">{entry.label}</span>
            {entry.detail && <span className="popitem-detail text-xs text-muted-foreground">{entry.detail}</span>}
          </>
          return <Fragment key={entry.id}>
            {entry.separated && <DropdownMenuSeparator />}
            {entry.checked === undefined ? (
              <DropdownMenuItem
                className="popitem"
                disabled={entry.enabled === false}
                onSelect={() => onChoose(entry.id)}
              >
                {content}
              </DropdownMenuItem>
            ) : (
              <DropdownMenuCheckboxItem
                className="popitem"
                checked={entry.checked}
                disabled={entry.enabled === false}
                onSelect={() => onChoose(entry.id)}
              >
                {content}
              </DropdownMenuCheckboxItem>
            )}
          </Fragment>
        })}
      </DropdownMenuGroup>
    </DropdownMenuContent>
  </DropdownMenu>
}
