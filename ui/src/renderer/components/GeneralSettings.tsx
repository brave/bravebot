import { useEffect, useState } from 'react'
import { APPEARANCES, type Appearance } from '../../shared/theme'
import type { LanguageReport, LanguageSaved } from '../../shared/protocol'
import { t } from '../i18n'
import { Alert, Button, Dropdown } from '../nala'
import { SettingsGroup } from './SettingsGroup'

const LABELS: Record<Appearance, () => string> = {
  system: () => t.uiAppearanceSystem(),
  light: () => t.uiAppearanceLight(),
  dark: () => t.uiAppearanceDark(),
}

const AUTOMATIC = 'auto'

/** How this window looks and reads. Each choice is kept as soon as it is made. */
export function GeneralSettings({ chosen, running, onAppearance }: {
  chosen: Appearance
  running: boolean
  onAppearance: (appearance: Appearance) => void
}): React.JSX.Element {
  return (
    <div className="settings-body">
      <SettingsGroup title={t.uiSettingsAppearance()}>
        <div className="settings-row">
          <div className="settings-row-text">
            <h3>{t.uiSettingsTheme()}</h3>
            <p>{t.uiSettingsThemeExplained()}</p>
          </div>
          <div className="settings-row-control">
            <Dropdown value={chosen} size="small" data-test="appearance-control"
              onChange={(detail) => {
                const next = detail.value
                if (next === 'system' || next === 'light' || next === 'dark') onAppearance(next)
              }}>
              {/* Leo drops `aria-*` on the element, so the field is named through its label slot,
                  hidden because the heading beside it already says it. */}
              <span slot="label" className="visually-hidden">{t.uiSettingsTheme()}</span>
              {APPEARANCES.map((appearance) => (
                <leo-option key={appearance} value={appearance}>{LABELS[appearance]()}</leo-option>
              ))}
            </Dropdown>
          </div>
        </div>
      </SettingsGroup>
      <LanguageGroup running={running} />
    </div>
  )
}

/** The catalog the window is read in, kept where `/language` keeps it, applied from the next start. */
function LanguageGroup({ running }: { running: boolean }): React.JSX.Element {
  const [report, setReport] = useState<LanguageReport | null>(null)
  const [unread, setUnread] = useState(false)
  const [selected, setSelected] = useState(AUTOMATIC)
  const [busy, setBusy] = useState(false)
  const [unsaved, setUnsaved] = useState(false)
  // The main process refused: a turn runs somewhere, perhaps in a session this page does not show.
  const [refused, setRefused] = useState(false)

  useEffect(() => {
    void window.bravebot.request<LanguageReport>('language.get').then((answer) => {
      if (!answer.ok || !Array.isArray(answer.ok.available)) return setUnread(true)
      setReport(answer.ok)
      setSelected(answer.ok.saved ?? AUTOMATIC)
    }, () => setUnread(true))
  }, [])

  const choose = async (next: string): Promise<void> => {
    const before = selected
    setSelected(next)
    setUnsaved(false)
    setBusy(true)
    try {
      const answer = await window.bravebot.request<LanguageSaved>('language.set', { tag: next })
      if (answer.ok?.saved) setSelected(answer.ok.tag)
      else {
        setSelected(before)
        setUnsaved(true)
      }
    } catch {
      setSelected(before)
      setUnsaved(true)
    } finally {
      setBusy(false)
    }
  }

  return (
    <SettingsGroup title={t.uiSettingsLanguage()}>
      <div className="settings-row" data-test="language-row">
        <div className="settings-row-text">
          <h3>{t.uiSettingsLanguage()}</h3>
          <p>{t.uiSettingsLanguageExplained()}</p>
          <p data-test="language-note">
            {report?.overridden ? t.uiSettingsLanguageOverridden() : t.uiSettingsLanguageNextStart()}
          </p>
          {(running || refused) && <p data-test="language-restart-running">{t.uiSettingsRestartRunning()}</p>}
        </div>
        <div className="settings-row-control">
          <Dropdown value={selected} size="small" data-test="language-control" disabled={!report || busy}
            onChange={(detail) => {
              if (typeof detail.value === 'string' && detail.value !== selected) void choose(detail.value)
            }}>
            <span slot="label" className="visually-hidden">{t.uiSettingsLanguage()}</span>
            <leo-option value={AUTOMATIC}>{t.uiSettingsLanguageAutomatic()}</leo-option>
            {report?.available.map(({ tag, name }) => (
              <leo-option key={tag} value={tag}>{name}</leo-option>
            ))}
          </Dropdown>
          <Button size="small" kind="outline" data-test="language-restart" isDisabled={!report || running}
            onClick={() => {
              void window.bravebot.relaunch().then((restarting) => setRefused(!restarting), () => setRefused(true))
            }}>{t.uiSettingsRestart()}</Button>
        </div>
        {(unread || unsaved) && (
          <Alert type="error" role="alert" className="settings-row-alert" data-test="language-error">
            {unread ? t.uiSettingsLanguageUnread() : t.uiSettingsLanguageUnsaved()}
          </Alert>
        )}
      </div>
    </SettingsGroup>
  )
}
