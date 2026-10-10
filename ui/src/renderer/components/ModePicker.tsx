import { useEffect, useRef, useState } from 'react'
import { Button, ButtonMenu, Icon, type IconName } from '../nala'
import { keyshortcuts } from './IconButton'

export interface ModeChoice { label: string; icon: IconName; says: string }

/**
 * A per-session setting with a few named values, chosen from a menu in the composer toolbar.
 *
 * `name` prefixes the classes the stylesheet draws it by (`permission-trigger`, `sandbox-menu`),
 * and `testId` prefixes the hooks the drivers find it by (`mode-trigger`, `sandbox-option-strict`).
 * `noun` is what the trigger's accessible name calls the setting.
 */
export function ModePicker<T extends string>({ name, testId, noun, modes, choices, mode, shortcut, onChoose }: {
  name: string
  testId: string
  noun: string
  modes: Record<T, ModeChoice>
  choices: readonly T[]
  mode: T
  shortcut?: string
  onChoose: (mode: T) => void
}): React.JSX.Element {
  const [open, setOpen] = useState(false)
  const trigger = useRef<HTMLElement>(null)
  const reason = useRef('explicit')
  const current = modes[mode]
  const tooltip = `${current.label}: ${current.says}`

  // Leo's React wrapper sets these once, when the host is made, so a change of mode would leave
  // the accessible name and the tooltip describing the mode it opened in.
  useEffect(() => {
    const button = trigger.current
    if (!button) return
    button.setAttribute('aria-label', `${noun}: ${current.label}`)
    button.setAttribute('aria-expanded', String(open))
    button.setAttribute('data-tooltip', tooltip)
    button.setAttribute('data-mode', mode)
  }, [current.label, open, tooltip, mode, noun])

  // Leo stamps `role="menuitem"` on every item when the menu opens. These are a radio group.
  useEffect(() => {
    const menu = trigger.current?.parentElement
    if (!menu) return
    const restore = (): void => {
      for (const item of menu.querySelectorAll<HTMLElement>('leo-menu-item[data-role]')) {
        if (item.getAttribute('role') !== item.dataset.role) item.setAttribute('role', item.dataset.role!)
      }
    }
    restore()
    const watch = new MutationObserver(restore)
    watch.observe(menu, { subtree: true, childList: true, attributes: true, attributeFilter: ['role'] })
    return () => watch.disconnect()
  }, [])

  return (
    <ButtonMenu
      className={`${name}-menu`}
      isOpen={open}
      placement="top-end"
      positionStrategy="fixed"
      onClose={(detail) => { reason.current = detail.reason }}
      onChange={({ isOpen }) => {
        setOpen(isOpen)
        if (!isOpen && reason.current !== 'blur') trigger.current?.focus()
        if (!isOpen) reason.current = 'explicit'
      }}
    >
      <Button ref={trigger} slot="anchor-content" kind="plain-faint" size="tiny"
        className={`${name}-trigger ${name}-${mode}`}
        aria-label={`${noun}: ${current.label}`} aria-haspopup="menu" aria-expanded={open}
        aria-keyshortcuts={shortcut ? keyshortcuts(shortcut) : undefined}
        data-tooltip={tooltip} data-tooltip-shortcut={shortcut}
        data-test={`${testId}-trigger`} data-mode={mode}>
        <Icon name={current.icon} slot="icon-before" />
        <span className={`${name}-current`}>{current.label}</span>
      </Button>
      {choices.map((choice) => (
        <leo-menu-item key={choice} data-role="menuitemradio" aria-checked={choice === mode ? 'true' : 'false'}
          data-test={`${testId}-option-${choice}`} onClick={() => { if (choice !== mode) onChoose(choice) }}>
          <span className={`menu-icon-row ${name}-option`}>
            <Icon name={modes[choice].icon} />
            <span className={`${name}-option-text`}>
              <span>{modes[choice].label}</span>
              <span className="menu-detail">{modes[choice].says}</span>
            </span>
            <span className="menu-check" aria-hidden="true">{choice === mode && <Icon name="check-normal" />}</span>
          </span>
        </leo-menu-item>
      ))}
    </ButtonMenu>
  )
}
