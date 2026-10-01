import { forwardRef } from 'react'
import { Button, Icon, type IconName } from '../nala'

type Kind = 'plain-faint' | 'plain' | 'outline' | 'filled'

/**
 * Every icon-only control in the window.
 *
 * The accessible name is the label, set on the inner control by `withShadowAttrs`. The visible
 * name is a tooltip drawn by `TooltipLayer` from `data-tooltip`, which is where a shortcut goes as
 * well, so "Find ⌘F" is said once, in the platform's own form, and never as a native `title` that
 * would draw a second box on top of it.
 */
export const IconButton = forwardRef<HTMLElement, {
  icon: IconName
  label: string
  /** The accelerator, as it is written on the menu: "⌘F". */
  shortcut?: string
  /** What the tooltip says when that is not the accessible name, or `false` for none. */
  tooltip?: string | false
  /** Said after the name by a screen reader, for state the icon alone carries (a badge). */
  description?: string
  kind?: Kind
  size?: 'tiny' | 'small' | 'medium'
  pressed?: boolean
  expanded?: boolean
  controls?: string
  hasPopup?: 'menu' | 'dialog' | 'true'
  disabled?: boolean
  className?: string
  slot?: string
  placement?: 'top' | 'bottom' | 'left' | 'right'
  onClick?: (event: MouseEvent) => void
  'data-test'?: string
  /** Further `data-*` attributes, keyed without the prefix. */
  dataset?: Record<string, string | number>
}>(function IconButton({
  icon, label, shortcut, tooltip, description, kind = 'plain-faint', size = 'small', pressed, expanded, controls, hasPopup,
  disabled, className, slot, placement, onClick, 'data-test': dataTest, dataset,
}, ref) {
  return (
    <Button
      {...(dataset && Object.fromEntries(Object.entries(dataset).map(([key, value]) => [`data-${key}`, value])))}
      ref={ref}
      fab
      kind={kind}
      size={size}
      {...(slot ? { slot } : {})}
      className={`icon-button ${kind} ${size}${className ? ` ${className}` : ''}`}
      isDisabled={disabled}
      aria-label={label}
      aria-description={description}
      aria-pressed={pressed}
      aria-expanded={expanded}
      aria-controls={controls}
      aria-haspopup={hasPopup}
      aria-keyshortcuts={shortcut ? keyshortcuts(shortcut) : undefined}
      data-tooltip={tooltip === false ? undefined : (tooltip ?? label)}
      data-tooltip-shortcut={shortcut}
      data-tooltip-placement={placement}
      data-test={dataTest}
      onClick={onClick}
    >
      <Icon name={icon} slot="icon-before" />
    </Button>
  )
})

/** "⌘⇧F" as `aria-keyshortcuts` spells it: "Meta+Shift+F". */
export function keyshortcuts(shortcut: string): string {
  const names: Record<string, string> = { '⌘': 'Meta', '⇧': 'Shift', '⌥': 'Alt', '⌃': 'Control', '↩': 'Enter', '⎋': 'Escape', '←': 'ArrowLeft', '→': 'ArrowRight' }
  const parts: string[] = []
  for (const char of shortcut) parts.push(names[char] ?? (char === '.' ? 'Period' : char.toUpperCase()))
  return parts.join('+')
}
