import { useState } from 'react'
import { Modal } from './Modal'
import { APPEARANCES, type Appearance } from '../../shared/theme'
import { applyAppearance } from '../theme'
import { Button, ControlItem, SegmentedControl } from '../nala'

interface Props {
  /** The appearance in force when the picker opened. */
  chosen: Appearance
  /** Keep the selection: persist it, and close. */
  onKeep: (appearance: Appearance) => void
  /** Close without keeping. The previous appearance is put back first. */
  onClose: () => void
}

const LABELS: Record<Appearance, string> = {
  system: 'System',
  light: 'Light',
  dark: 'Dark',
}

/**
 * Choosing System / Light / Dark for this window.
 *
 * Replaces the twenty-two named palettes. Moving previews; Escape puts the previous
 * choice back; Use keeps it in `bravebot-ui.json`.
 */
export function AppearancePicker(props: Props): React.JSX.Element {
  const { chosen, onKeep, onClose } = props
  const opened = chosen
  const [selected, setSelected] = useState(chosen)

  const preview = (value: Appearance): void => {
    applyAppearance(value)
    setSelected(value)
  }

  const cancel = (): void => {
    applyAppearance(opened)
    onClose()
  }

  const keep = (): void => {
    applyAppearance(selected)
    onKeep(selected)
  }

  return (
    <Modal title="Appearance" className="appearance-picker" onClose={cancel}>
      <SegmentedControl
        value={selected}
        size="small"
        data-test="appearance-control"
        onChange={(detail) => {
          const next = detail.value
          if (next === 'system' || next === 'light' || next === 'dark') preview(next)
        }}
      >
        {APPEARANCES.map((appearance) => (
          <ControlItem key={appearance} value={appearance}>
            {LABELS[appearance]}
          </ControlItem>
        ))}
      </SegmentedControl>
      <p className="theme-aside">
        System follows the OS. Light and Dark stay put regardless of it.
      </p>
      <div className="theme-actions">
        <Button kind="plain-faint" size="small" onClick={cancel} data-test="appearance-cancel">
          Cancel
        </Button>
        <Button kind="filled" size="small" onClick={keep} data-test="appearance-keep">
          Use
        </Button>
      </div>
    </Modal>
  )
}
