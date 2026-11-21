import { Modal } from './Modal'
import { Button } from '../nala'

/**
 * Asked before Chat › Continue in Background… hands the open chat to a background session
 * (BG-2, CMD-34). The chat closes here when it is handed over, so the dialog says that and how to
 * reach it again. `busy` is true while the bridge starts the background session.
 */
export function BackgroundConfirm({ name, busy, onCancel, onConfirm }: {
  name: string
  busy: boolean
  onCancel: () => void
  onConfirm: () => void
}): React.JSX.Element {
  return (
    <Modal title="Continue in background" size="sm" compact className="background-confirm" onClose={onCancel}
      subtitle={<><strong className="background-confirm-name">{name}</strong> closes here and keeps its record in a background session.</>}
      actions={<>
        <Button kind="plain-faint" onClick={onCancel} data-test="background-cancel">Cancel</Button>
        <Button kind="filled" onClick={onConfirm} isDisabled={busy} data-test="background-confirm">Continue in Background</Button>
      </>}>
      <p>Run <code>bravebot sessions</code> in a terminal to see it, and <code>bravebot attach</code> to talk to it. It can’t be opened here while it runs.</p>
    </Modal>
  )
}
