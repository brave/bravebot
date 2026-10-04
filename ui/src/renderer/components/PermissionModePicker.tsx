import { useEffect, useRef, useState } from 'react'
import { PERMISSION_MODES, type PermissionMode } from '../../shared/protocol'
import { Button, ButtonMenu, Icon, type IconName } from '../nala'
import { keyshortcuts } from './IconButton'

/** As the Session menu writes `mode.cycle`'s accelerator. */
const SHORTCUT = '⌘⇧M'

const MODES: Record<PermissionMode, { label: string; icon: IconName; says: string }> = {
  ask: {
    label: 'Ask',
    icon: 'hand-raised',
    says: 'Ask before every write and command',
  },
  acceptEdits: {
    label: 'Accept edits',
    icon: 'edit-pencil',
    says: 'Write files without asking. Commands and credential writes still ask',
  },
  plan: {
    label: 'Plan',
    icon: 'clipboard',
    says: 'Write nothing, only read and plan. Commands still ask',
  },
}

/**
 * How much the session's next turn asks before it acts.
 *
 * Asking is the quiet resting state, drawn like the model beside it. The other two are drawn
 * marked for as long as they hold, since each changes what the next turn does without a card to
 * say so.
 *
 * Not disabled while a turn runs. The running turn keeps the mode it began with (MODE-8), and the
 * tooltip says a change applies from the next one.
 */
export function PermissionModePicker({ mode, running, onChoose }: {
  mode: PermissionMode
  running: boolean
  onChoose: (mode: PermissionMode) => void
}): React.JSX.Element {
  const [open, setOpen] = useState(false)
  const trigger = useRef<HTMLElement>(null)
  const reason = useRef('explicit')
  const current = MODES[mode]
  const tooltip = `${current.label}: ${current.says}${running ? '. A change applies from the next turn' : ''}`

  // Leo's React wrapper sets these once, when the host is made, so a change of mode would leave
  // the accessible name and the tooltip describing the mode it opened in.
  useEffect(() => {
    const button = trigger.current
    if (!button) return
    button.setAttribute('aria-label', `Permission mode: ${current.label}`)
    button.setAttribute('aria-expanded', String(open))
    button.setAttribute('data-tooltip', tooltip)
    button.setAttribute('data-mode', mode)
  }, [current.label, open, tooltip, mode])

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
      className="permission-menu"
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
        className={`permission-trigger permission-${mode}`}
        aria-label={`Permission mode: ${current.label}`} aria-haspopup="menu" aria-expanded={open}
        aria-keyshortcuts={keyshortcuts(SHORTCUT)}
        data-tooltip={tooltip} data-tooltip-shortcut={SHORTCUT}
        data-test="mode-trigger" data-mode={mode}>
        <Icon name={current.icon} slot="icon-before" />
        <span className="permission-current">{current.label}</span>
      </Button>
      {PERMISSION_MODES.map((choice) => (
        <leo-menu-item key={choice} data-role="menuitemradio" aria-checked={choice === mode ? 'true' : 'false'}
          data-test={`mode-option-${choice}`} onClick={() => { if (choice !== mode) onChoose(choice) }}>
          <span className="menu-icon-row permission-option">
            <Icon name={MODES[choice].icon} />
            <span className="permission-option-text">
              <span>{MODES[choice].label}</span>
              <span className="menu-detail">{MODES[choice].says}</span>
            </span>
            <span className="menu-check" aria-hidden="true">{choice === mode && <Icon name="check-normal" />}</span>
          </span>
        </leo-menu-item>
      ))}
    </ButtonMenu>
  )
}
