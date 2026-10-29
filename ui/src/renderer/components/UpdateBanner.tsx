import { memo } from 'react'
import { Alert, Button } from '../nala'
import { IconButton } from './IconButton'

/**
 * A newer release is recorded (UPDATE-12): one sentence, a Download and a dismiss, at the top of
 * the sidebar. Never a popup and never focused, so it waits until somebody looks at it.
 *
 * Only the running version is named, since the window is not told the newer one (UPDATE-5).
 */
export const UpdateBanner = memo(function UpdateBanner({ running, onDownload, onDismiss }: {
  running: string
  onDownload: () => void
  onDismiss: () => void
}): React.JSX.Element {
  return (
    <Alert type="info" size="small" hasActions hasContentAfter className="update-banner" role="status" data-test="update-banner">
      A newer Brave Bot is available. You're on {running}.
      <IconButton slot="content-after" icon="close" label="Dismiss update notice" size="tiny" onClick={onDismiss} data-test="update-dismiss" />
      <div slot="actions">
        <Button kind="outline" size="small" onClick={onDownload} data-test="update-download">Download</Button>
      </div>
    </Alert>
  )
})
