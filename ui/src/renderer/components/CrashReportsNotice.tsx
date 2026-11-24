import { memo } from 'react'
import { Button } from '../nala'
import { t } from '../i18n'
import { ComposerNotice } from './ComposerNotice'

/**
 * What a crash report holds and how to turn it off, said once by a launch that is sending them, in
 * the tray shape the usage ping notice uses: under the welcome's actions, or docked on the composer.
 * The bridge records it as shown, in the marker the terminal reads, when it appears, so it stays
 * only until it is dismissed or this window closes.
 */
export const CrashReportsNotice = memo(function CrashReportsNotice({ onSettings, onDismiss, expanded = false }: {
  onSettings: () => void
  onDismiss: () => void
  expanded?: boolean
}): React.JSX.Element {
  return (
    <ComposerNotice
      prefix="crash-reports-notice"
      short={t.uiCrashReportsNoticeShort()}
      long={t.uiCrashReportsNotice()}
      more={t.uiCrashReportsNoticeMore()}
      less={t.uiCrashReportsNoticeLess()}
      dismissLabel={t.uiCrashReportsNoticeDismiss()}
      onDismiss={onDismiss}
      expanded={expanded}
      actions={<Button size="tiny" kind="plain-faint" onClick={onSettings} data-test="crash-reports-notice-settings">{t.uiCrashReportsNoticeSettings()}</Button>}
    />
  )
})
