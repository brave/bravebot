import { APPEARANCES, type Appearance } from '../../shared/theme'
import type { Experience } from '../../shared/experience'
import { ControlItem, Icon, SegmentedControl, type IconName } from '../nala'
import { setExperience, useExperience } from '../experience'
import { SettingsGroup } from './SettingsGroup'

const LABELS: Record<Appearance, string> = { system: 'System', light: 'Light', dark: 'Dark' }
const ICONS: Record<Appearance, IconName> = { system: 'theme-system', light: 'theme-light', dark: 'theme-dark' }

/** How this window looks. Each choice applies and is kept as soon as it is made. */
export function GeneralSettings({ chosen, onAppearance }: {
  chosen: Appearance
  onAppearance: (appearance: Appearance) => void
}): React.JSX.Element {
  const density = useExperience().density
  return (
    <div className="settings-body">
      <SettingsGroup title="Appearance">
        <div className="settings-row">
          <div className="settings-row-text">
            <h3 id="appearance-theme">Theme</h3>
            <p>System follows the OS. Light and Dark stay put regardless of it.</p>
          </div>
          <div className="settings-row-control">
            <SegmentedControl value={chosen} size="small" aria-labelledby="appearance-theme" data-test="appearance-control"
              onChange={(detail) => {
                const next = detail.value
                if (next === 'system' || next === 'light' || next === 'dark') onAppearance(next)
              }}>
              {APPEARANCES.map((appearance) => (
                <ControlItem key={appearance} value={appearance}>
                  <Icon name={ICONS[appearance]} slot="icon-before" />
                  {LABELS[appearance]}
                </ControlItem>
              ))}
            </SegmentedControl>
          </div>
        </div>
        <div className="settings-row">
          <div className="settings-row-text">
            <h3 id="appearance-density">Density</h3>
            <p>Compact drops the branch line from each chat row.</p>
          </div>
          <div className="settings-row-control">
            <SegmentedControl value={density} size="small" aria-labelledby="appearance-density" data-test="density-control"
              onChange={(detail) => {
                const next = detail.value as Experience['density']
                if (next === 'comfortable' || next === 'compact') setExperience('density', next)
              }}>
              <ControlItem value="comfortable">Comfortable</ControlItem>
              <ControlItem value="compact">Compact</ControlItem>
            </SegmentedControl>
          </div>
        </div>
      </SettingsGroup>
    </div>
  )
}
