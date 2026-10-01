import { useState } from 'react'
import { Modal } from './Modal'
import { APPEARANCES, type Appearance } from '../../shared/theme'
import { applyAppearance } from '../theme'
import { Button, ControlItem, Icon, SegmentedControl, type IconName } from '../nala'
import { setExperience, useExperience } from '../experience'
import type { Experience } from '../../shared/experience'

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
const ICONS: Record<Appearance, IconName> = {
  system: 'theme-system',
  light: 'theme-light',
  dark: 'theme-dark',
}
type Density = Experience['density']

/**
 * Choosing System / Light / Dark for this window, and how tightly the session list is packed.
 *
 * Replaces the twenty-two named palettes. Moving previews; Escape puts the previous
 * choices back; Use keeps them in `bravebot-ui.json`.
 */
export function AppearancePicker(props: Props): React.JSX.Element {
  const { chosen, onKeep, onClose } = props
  const opened = chosen
  const [selected, setSelected] = useState(chosen)
  const density = useExperience().density
  const [openedDensity] = useState(density)

  const preview = (value: Appearance): void => {
    applyAppearance(value)
    setSelected(value)
  }

  const cancel = (): void => {
    applyAppearance(opened)
    if (density !== openedDensity) setExperience('density', openedDensity)
    onClose()
  }

  const keep = (): void => {
    applyAppearance(selected)
    onKeep(selected)
  }

  const keys = (event: React.KeyboardEvent): void => {
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault()
      const index = APPEARANCES.indexOf(selected)
      const offset = event.key === 'ArrowDown' ? 1 : -1
      preview(APPEARANCES[(index + offset + APPEARANCES.length) % APPEARANCES.length]!)
    } else if (event.key === 'Enter' && !(event.target instanceof HTMLElement && event.target.closest('leo-button'))) {
      event.preventDefault()
      keep()
    }
  }

  return (
    <Modal title="Appearance" size="sm" subtitle="How this window looks. Changes preview as you choose them." className="appearance-picker" onClose={cancel} actions={<>
      <Button kind="plain-faint" size="small" className="modal-leading" onClick={cancel} data-test="appearance-cancel">
        Cancel
      </Button>
      <Button kind="filled" size="small" onClick={keep} data-test="appearance-keep">
        Use
      </Button>
    </>}>
      <div className="appearance-field">
        <span className="appearance-label" id="appearance-theme">Theme</span>
        <div onKeyDownCapture={keys}>
          <SegmentedControl
            value={selected}
            size="small"
            aria-labelledby="appearance-theme"
            data-test="appearance-control"
            onChange={(detail) => {
              const next = detail.value
              if (next === 'system' || next === 'light' || next === 'dark') preview(next)
            }}
          >
            {APPEARANCES.map((appearance) => (
              <ControlItem key={appearance} value={appearance}>
                <Icon name={ICONS[appearance]} slot="icon-before" />
                {LABELS[appearance]}
              </ControlItem>
            ))}
          </SegmentedControl>
        </div>
        <p className="theme-aside">System follows the OS. Light and Dark stay put regardless of it.</p>
      </div>
      <div className="appearance-field">
        <span className="appearance-label" id="appearance-density">Density</span>
        <SegmentedControl
          value={density}
          size="small"
          aria-labelledby="appearance-density"
          data-test="density-control"
          onChange={(detail) => {
            const next = detail.value as Density
            if (next === 'comfortable' || next === 'compact') setExperience('density', next)
          }}
        >
          <ControlItem value="comfortable">Comfortable</ControlItem>
          <ControlItem value="compact">Compact</ControlItem>
        </SegmentedControl>
        <p className="theme-aside">Compact drops the project line from each session row.</p>
      </div>
    </Modal>
  )
}
