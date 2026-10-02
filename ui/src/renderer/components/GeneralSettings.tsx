import { APPEARANCES, type Appearance } from '../../shared/theme'
import { Dropdown } from '../nala'
import { SettingsGroup } from './SettingsGroup'

const LABELS: Record<Appearance, string> = { system: 'System', light: 'Light', dark: 'Dark' }

/** How this window looks. Each choice applies and is kept as soon as it is made. */
export function GeneralSettings({ chosen, onAppearance }: {
  chosen: Appearance
  onAppearance: (appearance: Appearance) => void
}): React.JSX.Element {
  return (
    <div className="settings-body">
      <SettingsGroup title="Appearance">
        <div className="settings-row">
          <div className="settings-row-text">
            <h3 id="appearance-theme">Theme</h3>
            <p>System follows the OS. Light and Dark stay put regardless of it.</p>
          </div>
          <div className="settings-row-control">
            <Dropdown value={chosen} size="small" aria-labelledby="appearance-theme" data-test="appearance-control"
              onChange={(detail) => {
                const next = detail.value
                if (next === 'system' || next === 'light' || next === 'dark') onAppearance(next)
              }}>
              {APPEARANCES.map((appearance) => (
                <leo-option key={appearance} value={appearance}>{LABELS[appearance]}</leo-option>
              ))}
            </Dropdown>
          </div>
        </div>
      </SettingsGroup>
    </div>
  )
}
