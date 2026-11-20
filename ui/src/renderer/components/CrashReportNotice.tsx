import { memo, useState } from 'react'
import { Icon } from '../nala'
import { t } from '../i18n'
import { IconButton } from './IconButton'

/**
 * That crash reports are sent to Brave, said once while the bridge is reporting, in the same tray
 * shape as the usage-ping notice. The desktop has no crash-report switch, so the text names the
 * settings-file key and the environment variable instead of offering a settings button. The bridge
 * records it as shown when it appears, in the record the terminal also reads.
 */
export const CrashReportNotice = memo(function CrashReportNotice({ onDismiss, expanded: startExpanded = false }: {
  onDismiss: () => void
  expanded?: boolean
}): React.JSX.Element {
  const [expanded, setExpanded] = useState(startExpanded)
  return (
    <div className={`composer-tray composer-notice crash-report-notice${expanded ? ' expanded' : ''}`} role="status" data-test="crash-report-notice">
      <Icon name="info-outline" />
      <span className="notice-text" id="crash-report-notice-text">
        {expanded ? t.uiCrashReportsNotice() : t.uiCrashReportsNoticeShort()}{' '}
        {/* A bare button rather than a Leo one, so it sits in the sentence without raising the line. */}
        <button
          type="button"
          className="notice-more"
          aria-expanded={expanded}
          aria-controls="crash-report-notice-text"
          onClick={() => setExpanded((open) => !open)}
          data-test="crash-report-notice-more"
        >
          {expanded ? t.uiCrashReportsNoticeLess() : t.uiCrashReportsNoticeMore()}
        </button>
      </span>
      <span className="notice-actions">
        <IconButton icon="close" size="tiny" label={t.uiCrashReportsNoticeDismiss()} tooltip={t.uiCrashReportsNoticeDismiss()} onClick={onDismiss} data-test="crash-report-notice-dismiss" />
      </span>
    </div>
  )
})
