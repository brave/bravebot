import { useEffect, useState } from 'react'
import { APPEARANCES, type Appearance } from '../../shared/theme'
import type { CrashReportsReport, LanguageReport, LanguageSaved, UsagePingReport } from '../../shared/protocol'
import { t } from '../i18n'
import { Alert, Button, Dropdown, Toggle } from '../nala'
import { changeUsagePing, lockedLine, reportIn, type Switch } from '../usage-ping'
import { crashReportsLockedLine } from '../crash-reports'
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
      <PrivacyGroup />
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
          <h3>{t.uiSettingsDisplayLanguage()}</h3>
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
            <span slot="label" className="visually-hidden">{t.uiSettingsDisplayLanguage()}</span>
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

/** The two switches that say what this program sends. Each is kept where the terminal keeps it. */
function PrivacyGroup(): React.JSX.Element {
  return (
    <SettingsGroup title={t.uiSettingsPrivacy()}>
      <PrivacySwitch id="usage-ping" method="usage_ping" title={t.uiSettingsUsagePing()}
        explained={t.uiSettingsUsagePingExplained()} unread={t.uiSettingsUsagePingUnread()}
        unsaved={t.uiSettingsUsagePingUnsaved()}
        locked={(reason) => lockedLine(reason as UsagePingReport['reason'])} />
      <PrivacySwitch id="crash-reports" method="crash_reports" title={t.uiSettingsCrashReports()}
        explained={t.uiSettingsCrashReportsExplained()} unread={t.uiSettingsCrashReportsUnread()}
        unsaved={t.uiSettingsCrashReportsUnsaved()}
        locked={(reason) => crashReportsLockedLine(reason as CrashReportsReport['reason'])} />
    </SettingsGroup>
  )
}

/**
 * One privacy switch: on or off, read with `<method>.get` and kept with `<method>.set`. Locked, with
 * the line the bridge's reason gives, where something stronger than the switch decides.
 */
function PrivacySwitch({ id, method, title, explained, unread: unreadLine, unsaved: unsavedLine, locked: lockedFor }: {
  id: string
  method: string
  title: string
  explained: string
  unread: string
  unsaved: string
  locked: (reason: unknown) => string | null
}): React.JSX.Element {
  const [report, setReport] = useState<Switch<unknown> | null>(null)
  const [unread, setUnread] = useState(false)
  const [unsaved, setUnsaved] = useState(false)
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    void window.bravebot.request<Switch<unknown>>(`${method}.get`).then((answer) => {
      const read = reportIn(answer)
      if (!read) return setUnread(true)
      setReport(read)
    }, () => setUnread(true))
  }, [method])

  const change = async (enabled: boolean): Promise<void> => {
    if (!report) return
    setBusy(true)
    setUnsaved(false)
    setReport({ ...report, enabled })
    const next = await changeUsagePing(
      (on) => window.bravebot.request<Switch<unknown> & { saved: boolean }>(`${method}.set`, { enabled: on }), report, enabled)
    setReport(next.report)
    setUnsaved(next.unsaved)
    setBusy(false)
  }

  const lockedText = report && !report.changeable ? lockedFor(report.reason) : null
  return (
    <div className="settings-row" data-test={`${id}-row`}>
      <div className="settings-row-text">
        <h3>{title}</h3>
        <p>{explained}</p>
        {lockedText && <p data-test={`${id}-locked`}>{lockedText}</p>}
      </div>
      <div className="settings-row-control">
        <Toggle size="small" data-test={`${id}-control`} aria-label={title}
          checked={report?.enabled ?? false} disabled={!report || !report.changeable || busy}
          onChange={({ checked }) => { if (checked !== report?.enabled) void change(checked) }} />
      </div>
      {(unread || unsaved) && (
        <Alert type="error" role="alert" className="settings-row-alert" data-test={`${id}-error`}>
          {unread ? unreadLine : unsavedLine}
        </Alert>
      )}
    </div>
  )
}
