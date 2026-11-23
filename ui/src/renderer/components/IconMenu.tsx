import { useEffect, useRef, useState, type ReactNode } from 'react'
import { ButtonMenu, type IconName } from '../nala'
import { useFocusReturn, useMenuItemRoles } from '../hooks'
import { IconButton } from './IconButton'

/**
 * A menu opened from an icon button.
 *
 * Escape and a choice return focus to the trigger; a click elsewhere leaves it where the pointer
 * landed, which is where the reader's attention already went.
 *
 * Leo stamps `role="menuitem"` on every item each time the menu opens, so a checkable item
 * declares its role as `data-role` and gets it back here.
 */
export function IconMenu({ icon, label, tooltip, shortcut, disabled, className, triggerClassName, size = 'small', placement = 'bottom-end', onOpen, openRequest, children, 'data-test': dataTest }: {
  icon: IconName
  label: string
  tooltip?: string | false
  shortcut?: string
  disabled?: boolean
  className?: string
  triggerClassName?: string
  size?: 'tiny' | 'small'
  placement?: 'bottom-end' | 'bottom-start' | 'top-end' | 'top-start'
  onOpen?: (open: boolean) => void
  /**
   * Open the menu from outside it, as a menu-bar command does. Each new non-zero value opens it once,
   * with focus on its first item. Escape returns focus to the trigger, as it does after a click.
   */
  openRequest?: number
  children: ReactNode
  'data-test'?: string
}): React.JSX.Element {
  const [open, setOpen] = useState(false)
  const trigger = useRef<HTMLElement>(null)
  const { onClose, restoreFocus } = useFocusReturn(trigger)
  useMenuItemRoles(trigger)
  const handled = useRef(0)
  const opened = useRef(onOpen)
  opened.current = onOpen
  useEffect(() => {
    if (!openRequest || openRequest === handled.current) return
    handled.current = openRequest
    setOpen(true)
    opened.current?.(true)
    // Focus goes to the first item, so the arrow keys and Enter work at once. Not to the trigger:
    // Leo's ButtonMenu ignores an Escape pressed on its anchor. A frame later, because the items
    // are drawn only once the menu has opened.
    const frame = requestAnimationFrame(() =>
      trigger.current?.parentElement?.querySelector<HTMLElement>('leo-menu-item')?.focus())
    return () => cancelAnimationFrame(frame)
  }, [openRequest])
  return (
    <ButtonMenu
      className={`icon-menu${className ? ` ${className}` : ''}`}
      isOpen={open}
      placement={placement}
      positionStrategy="fixed"
      onClose={onClose}
      onChange={({ isOpen }) => {
        setOpen(isOpen)
        onOpen?.(isOpen)
        if (!isOpen) restoreFocus()
      }}
    >
      <IconButton
        ref={trigger}
        slot="anchor-content"
        icon={icon}
        label={label}
        tooltip={tooltip}
        shortcut={shortcut}
        size={size}
        className={triggerClassName}
        disabled={disabled}
        hasPopup="menu"
        expanded={open}
        data-test={dataTest}
      />
      {children}
    </ButtonMenu>
  )
}
