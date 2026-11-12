import { memo, useState } from 'react'
import { Button, Icon } from '../nala'
import { t } from '../i18n'
import { IconButton } from './IconButton'

/**
 * What the daily usage ping sends, said once before the first ping, in the tray shape the backend
 * notice uses: under the welcome's actions, or docked on the composer. The bridge records it as
 * shown when it appears, so it stays only until it is dismissed or this window closes.
 */
export const UsagePingNotice = memo(function UsagePingNotice({ onSettings, onDismiss, expanded: startExpanded = false }: {
  onSettings: () => void
  onDismiss: () => void
  expanded?: boolean
}): React.JSX.Element {
  const [expanded, setExpanded] = useState(startExpanded)
  return (
    <div className={`composer-tray composer-notice usage-ping-notice${expanded ? ' expanded' : ''}`} role="status" data-test="usage-ping-notice">
      <Icon name="info-outline" />
      <span className="notice-text" id="usage-ping-notice-text">
        {expanded ? t.uiUsagePingNotice() : t.uiUsagePingNoticeShort()}{' '}
        {/* A bare button rather than a Leo one, so it sits in the sentence without raising the line. */}
        <button
          type="button"
          className="notice-more"
          aria-expanded={expanded}
          aria-controls="usage-ping-notice-text"
          onClick={() => setExpanded((open) => !open)}
          data-test="usage-ping-notice-more"
        >
          {expanded ? t.uiUsagePingNoticeLess() : t.uiUsagePingNoticeMore()}
        </button>
      </span>
      <span className="notice-actions">
        <Button size="tiny" kind="plain-faint" onClick={onSettings} data-test="usage-ping-notice-settings">{t.uiUsagePingNoticeSettings()}</Button>
        <IconButton icon="close" size="tiny" label={t.uiUsagePingNoticeDismiss()} tooltip={t.uiUsagePingNoticeDismiss()} onClick={onDismiss} data-test="usage-ping-notice-dismiss" />
      </span>
    </div>
  )
})
