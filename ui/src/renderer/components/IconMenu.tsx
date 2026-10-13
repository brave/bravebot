import { useRef, useState, type ReactNode } from 'react'
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
export function IconMenu({ icon, label, tooltip, shortcut, disabled, className, triggerClassName, size = 'small', placement = 'bottom-end', onOpen, children, 'data-test': dataTest }: {
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
  children: ReactNode
  'data-test'?: string
}): React.JSX.Element {
  const [open, setOpen] = useState(false)
  const trigger = useRef<HTMLElement>(null)
  const { onClose, restoreFocus } = useFocusReturn(trigger)
  useMenuItemRoles(trigger)
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
