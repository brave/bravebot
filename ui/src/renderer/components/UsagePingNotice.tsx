import { memo } from 'react'
import { Button } from '../nala'
import { t } from '../i18n'
import { ComposerNotice } from './ComposerNotice'

/**
 * What the daily usage ping sends, said once before the first ping, in the tray shape the backend
 * notice uses: under the welcome's actions, or docked on the composer. The bridge records it as
 * shown when it appears, so it stays only until it is dismissed or this window closes.
 */
export const UsagePingNotice = memo(function UsagePingNotice({ onSettings, onDismiss, expanded = false }: {
  onSettings: () => void
  onDismiss: () => void
  expanded?: boolean
}): React.JSX.Element {
  return (
    <ComposerNotice
      prefix="usage-ping-notice"
      short={t.uiUsagePingNoticeShort()}
      long={t.uiUsagePingNotice()}
      more={t.uiUsagePingNoticeMore()}
      less={t.uiUsagePingNoticeLess()}
      dismissLabel={t.uiUsagePingNoticeDismiss()}
      onDismiss={onDismiss}
      expanded={expanded}
      actions={<Button size="tiny" kind="plain-faint" onClick={onSettings} data-test="usage-ping-notice-settings">{t.uiUsagePingNoticeSettings()}</Button>}
    />
  )
})
