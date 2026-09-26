import { useEffect, Fragment } from 'react'
import { cn } from '@/lib/utils'
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'

/**
 * A menu that hangs off a control in the window.
 *
 * The app pops native menus for right-clicks — see `src/main/menu.ts` — and this exists
 * alongside them rather than instead of them, because an anchored picker and a context menu
 * are different objects. This one has to return focus to the button that opened it, and it
 * has to be able to say "no projects opened yet" in the app's own voice. `Menu.popup` gives
 * back no focus contract at all: once it returns, the button has no way to know the menu
 * closed. For a control somebody may be driving from the keyboard that is disqualifying.
 *
 * Both are fed from the same declarations, so the two mechanisms cannot drift about *what*
 * they offer even though they differ in how they draw it.
 */

export interface PopItem {
  id: string
  label: string
  /** The quieter second line — a full path under a folder's name. */
  detail?: string
  enabled?: boolean
  /**
   * A setting rather than an action: drawn with a tick, and announced as a checkbox.
   *
   * Choosing one still closes the menu, which is what a native menu does with a checkable
   * item and is the one behaviour worth copying here: a menu that stayed up would be the
   * only surface in the app that did, and the tick is visible again the moment it reopens.
   */
  checked?: boolean
  /** Drawn with a rule above it, where a group of rows is a different kind of thing. */
  separated?: boolean
}

export function PopMenu({
  open,
  items,
  label,
  onChoose,
  onOpenChange,
  trigger,
}: {
  open: boolean
  items: readonly PopItem[]
  label: string
  onChoose: (id: string) => void
  onOpenChange: (open: boolean) => void
  /** The control the menu hangs off. Must accept Base UI's `render` merge (a real element). */
  trigger: React.ReactElement
}): React.JSX.Element {
  // A tick column for the whole menu as soon as one row can carry a tick, which is what a
  // native menu does: labels stay in one line down the left whether or not the row above
  // them is a setting, and turning one on moves nothing.
  const checkable = items.some((item) => item.checked !== undefined)

  // Closed rather than followed: it is what AppKit does, and it is one behaviour instead
  // of a reflow loop chasing the anchor.
  useEffect(() => {
    if (!open) return
    const away = (): void => onOpenChange(false)
    window.addEventListener('resize', away)
    window.addEventListener('scroll', away, true)
    return () => {
      window.removeEventListener('resize', away)
      window.removeEventListener('scroll', away, true)
    }
  }, [open, onOpenChange])

  return (
    <DropdownMenu open={open} onOpenChange={onOpenChange}>
      <DropdownMenuTrigger render={trigger} />
      <DropdownMenuContent
        className={cn('popmenu w-auto min-w-[220px]', checkable && 'checkable')}
        side="bottom"
        align="start"
        sideOffset={4}
        aria-label={label}
      >
        {items.map((entry) => {
          const body = (
            <>
              <span className="popitem-label block">{entry.label}</span>
              {/* The path, in the typeface paths are read in, on one line. A folder chosen three
                  levels down would otherwise wrap the row to twice the height of its neighbours
                  and the labels would stop being a column. */}
              {entry.detail ? (
                <span className="popitem-detail block w-full truncate font-mono text-[10px] text-muted-foreground">
                  {entry.detail}
                </span>
              ) : null}
            </>
          )
          return (
            <Fragment key={entry.id}>
              {entry.separated ? <DropdownMenuSeparator /> : null}
              {entry.checked !== undefined ? (
                <DropdownMenuCheckboxItem
                  className="popitem flex-col items-start focus:[&_.popitem-detail]:text-accent-foreground focus:[&_.popitem-detail]:opacity-75"
                  checked={entry.checked}
                  disabled={entry.enabled === false}
                  // Native menus dismiss on a checkable choose; keep that contract so a second
                  // open sees a fresh popup rather than a toggle that closed an already-open one.
                  closeOnClick
                  onCheckedChange={() => onChoose(entry.id)}
                >
                  {body}
                </DropdownMenuCheckboxItem>
              ) : (
                <DropdownMenuItem
                  className="popitem flex-col items-start focus:[&_.popitem-detail]:text-accent-foreground focus:[&_.popitem-detail]:opacity-75"
                  disabled={entry.enabled === false}
                  onClick={() => onChoose(entry.id)}
                >
                  {body}
                </DropdownMenuItem>
              )}
            </Fragment>
          )
        })}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
