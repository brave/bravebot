import { useEffect, useRef, useState } from 'react'
import { PERMISSION_MODES, SANDBOX_MODES, type PermissionMode, type SandboxMode } from '../../shared/protocol'
import { Button, ButtonMenu, Icon, type IconName } from '../nala'
import { useFocusReturn, useMenuItemRoles } from '../hooks'
import { keyshortcuts } from './IconButton'

interface Choice { label: string; icon: IconName; says: string }

/** As the Chat menu writes `mode.cycle`'s accelerator. */
const SHORTCUT = '⌘⇧M'

const PERMISSIONS: Record<PermissionMode, Choice> = {
  ask: {
    label: 'Ask',
    icon: 'hand-raised',
    says: 'Asks before every write and command',
  },
  acceptEdits: {
    label: 'Accept edits',
    icon: 'edit-pencil',
    says: 'Writes without asking. Commands still ask',
  },
  plan: {
    label: 'Plan',
    icon: 'clipboard',
    says: 'Reads and plans only. Commands still ask',
  },
  auto: {
    label: 'Auto',
    icon: 'product-brave-leo',
    says: 'Writes freely. A model checks commands',
  },
}

const SANDBOXES: Record<SandboxMode, Choice> = {
  'workspace-write': {
    label: 'Workspace write',
    icon: 'shield',
    says: 'Can read most files, but can only make changes inside this workspace',
  },
  'read-only': {
    label: 'Read only',
    icon: 'eye-on',
    says: 'Can read most files, but can’t change anything',
  },
  strict: {
    label: 'Strict',
    icon: 'shield-done',
    says: 'Can only access the files named in the command. Anything else fails',
  },
}

/** On Windows none of these modes reads the machine, and Strict gives what Workspace write does. */
const SANDBOXES_ON_WINDOWS: Record<SandboxMode, Choice> = {
  'workspace-write': {
    ...SANDBOXES['workspace-write'],
    says: 'Programs read only the session, the system and what their command names, and write only in the session',
  },
  'read-only': {
    ...SANDBOXES['read-only'],
    says: 'Programs read only the session, the system and what their command names. Neither they nor the file tools write in the session',
  },
  strict: {
    ...SANDBOXES.strict,
    says: 'The same reach as Workspace write on Windows. Programs read only the session, the system and what their command names',
  },
}

/** The most the popup is allowed to grow to, and what it leaves clear above itself. */
const MENU_MAX_HEIGHT = 640
const MENU_EDGE_GAP = 16

/** The sandbox mode a session rests in, which the trigger does not name. */
const SANDBOX_AT_REST: SandboxMode = 'workspace-write'

export interface SessionControlsMenuProps {
  /** Absent where no session exists to hold a mode, as on a bot's first page. */
  permissionMode?: PermissionMode
  onPermissionMode?: (mode: PermissionMode) => void
  sandboxMode?: SandboxMode
  onSandboxMode?: (mode: SandboxMode) => void
}

/**
 * What the session asks before it acts and what a program it runs is held to, chosen from one
 * menu in the composer toolbar.
 *
 * The trigger names the approval mode. A sandbox mode that is not at rest is added after it, so
 * nothing that changes what the session does is hidden inside a closed menu.
 *
 * Neither is disabled while a turn runs: a change of the approval mode applies to the rest of
 * that turn (MODE-8) and a change of the sandbox mode from the next one.
 */
export function SessionControlsMenu({
  permissionMode, onPermissionMode, sandboxMode, onSandboxMode,
}: SessionControlsMenuProps): React.JSX.Element | null {
  const [open, setOpen] = useState(false)
  const trigger = useRef<HTMLElement>(null)
  const { onClose, restoreFocus } = useFocusReturn(trigger)
  const sandboxes = window.bravebot.platform === 'win32' ? SANDBOXES_ON_WINDOWS : SANDBOXES

  const permissions = permissionMode && onPermissionMode
  const sandbox = sandboxMode && onSandboxMode

  const lead = permissions ? PERMISSIONS[permissionMode] : sandbox ? sandboxes[sandboxMode] : null
  const label = lead
    ? permissions && sandbox && sandboxMode !== SANDBOX_AT_REST ? `${lead.label} · ${sandboxes[sandboxMode].label}` : lead.label
    : ''
  const tooltip = [
    permissions ? `${PERMISSIONS[permissionMode].label}: ${PERMISSIONS[permissionMode].says}` : null,
    sandbox ? `${sandboxes[sandboxMode].label}: ${sandboxes[sandboxMode].says}` : null,
  ].filter((line): line is string => line !== null).join('\n')

  // Leo's React wrapper sets these once, when the host is made, so a change would leave the
  // accessible name and the tooltip describing what the menu opened with.
  useEffect(() => {
    const button = trigger.current
    if (!button) return
    button.setAttribute('aria-label', `Session controls: ${label}`)
    button.setAttribute('aria-expanded', String(open))
    button.setAttribute('data-tooltip', tooltip)
    for (const [name, value] of [['data-mode', permissionMode], ['data-sandbox', sandboxMode]] as const) {
      if (value) button.setAttribute(name, value)
      else button.removeAttribute(name)
    }
  }, [label, open, tooltip, permissionMode, sandboxMode])

  // Leo stamps `role="menuitem"` on every item when the menu opens. These are radio groups.
  useMenuItemRoles(trigger)

  if (!lead) return null

  const row = <T extends string>(
    key: T, current: T, choices: Record<T, Choice>, testId: string, onChoose: (key: T) => void,
  ) => (
    <leo-menu-item key={key} data-role="menuitemradio" aria-checked={key === current ? 'true' : 'false'}
      data-test={testId} onClick={() => { if (key !== current) onChoose(key) }}>
      <span className="menu-icon-row controls-option">
        <Icon name={choices[key].icon} />
        <span className="controls-option-text">
          <span>{choices[key].label}</span>
          <span className="menu-detail">{choices[key].says}</span>
        </span>
        <span className="menu-check" aria-hidden="true">{key === current && <Icon name="check-normal" />}</span>
      </span>
    </leo-menu-item>
  )

  const groups: React.JSX.Element[] = []
  if (permissions) {
    groups.push(<div key="permission" className="controls-heading" role="presentation">Approvals</div>)
    for (const choice of PERMISSION_MODES) {
      groups.push(row(choice, permissionMode, PERMISSIONS, `mode-option-${choice}`, onPermissionMode))
    }
  }
  if (sandbox) {
    groups.push(<div key="sandbox" className="controls-heading" role="presentation">Sandbox</div>)
    for (const choice of SANDBOX_MODES) {
      groups.push(row(choice, sandboxMode, sandboxes, `sandbox-option-${choice}`, onSandboxMode))
    }
  }

  return (
    <ButtonMenu
      className="controls-menu"
      isOpen={open}
      placement="top-end"
      positionStrategy="fixed"
      onClose={onClose}
      onChange={({ isOpen }) => {
        // The popup grows upward from the button, so the room above it is all it has.
        const host = trigger.current?.closest<HTMLElement>('.controls-menu')
        if (isOpen && trigger.current && host) {
          const room = Math.floor(trigger.current.getBoundingClientRect().top - MENU_EDGE_GAP)
          host.style.setProperty('--leo-menu-max-height', `${Math.max(0, Math.min(MENU_MAX_HEIGHT, room))}px`)
        }
        setOpen(isOpen)
        if (!isOpen) restoreFocus()
      }}
    >
      <Button ref={trigger} slot="anchor-content" kind="plain-faint" size="tiny"
        className={`controls-trigger${permissionMode ? ` permission-${permissionMode}` : ''}${sandboxMode === 'strict' ? ' sandbox-strict' : ''}`}
        aria-label={`Session controls: ${label}`} aria-haspopup="menu" aria-expanded={open}
        aria-keyshortcuts={permissions ? keyshortcuts(SHORTCUT) : undefined}
        data-tooltip={tooltip} data-tooltip-shortcut={permissions ? SHORTCUT : undefined}
        data-test="mode-trigger" data-mode={permissionMode} data-sandbox={sandboxMode}>
        <Icon name={lead.icon} slot="icon-before" />
        <span className="controls-current">{label}</span>
        <Icon name="carat-down" slot="icon-after" />
      </Button>
      {groups}
    </ButtonMenu>
  )
}
