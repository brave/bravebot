import { Modal } from './Modal'
import { Button } from '../nala'

/** Asked the first time keeping the computer awake is turned on (CMD-12), and not again. */
export function CaffeinateConfirm({ onCancel, onEnable }: {
  onCancel: () => void
  onEnable: () => void
}): React.JSX.Element {
  return (
    <Modal title="Keep computer awake during turns?" size="sm" compact className="caffeinate-confirm"
      onClose={onCancel} actions={<>
        <Button kind="plain-faint" size="small" className="modal-leading" onClick={onCancel} data-test="caffeinate-cancel">
          Cancel
        </Button>
        <Button kind="filled" size="small" onClick={onEnable} data-test="caffeinate-enable">
          Enable
        </Button>
      </>}>
      <p>
        While a turn runs, the computer won’t go to sleep, so the turn can finish. The screen still
        dims and locks, but the computer keeps running, with your credentials in reach of the turn.
      </p>
      <p>It’s off again each time Brave Bot opens.</p>
    </Modal>
  )
}
