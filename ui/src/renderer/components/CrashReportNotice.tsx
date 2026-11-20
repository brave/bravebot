import { memo } from 'react'
import { t } from '../i18n'
import { ComposerNotice } from './ComposerNotice'

/**
 * That crash reports are sent to Brave, said once while the bridge is reporting, in the same tray
 * shape as the usage-ping notice. The desktop has no crash-report switch, so the text names the
 * settings-file key and the environment variable instead of offering a settings button. The bridge
 * records it as shown when it appears, in the record the terminal also reads.
 */
export const CrashReportNotice = memo(function CrashReportNotice({ onDismiss, expanded = false }: {
  onDismiss: () => void
  expanded?: boolean
}): React.JSX.Element {
  return (
    <ComposerNotice
      prefix="crash-report-notice"
      short={t.uiCrashReportsNoticeShort()}
      long={t.uiCrashReportsNotice()}
      more={t.uiCrashReportsNoticeMore()}
      less={t.uiCrashReportsNoticeLess()}
      dismissLabel={t.uiCrashReportsNoticeDismiss()}
      onDismiss={onDismiss}
      expanded={expanded}
    />
  )
})
