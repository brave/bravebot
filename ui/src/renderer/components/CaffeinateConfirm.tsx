import { Modal } from './Modal'
import { Button } from '../nala'

/** Whether the setting is on, and whether the dialog is up. */
export interface KeepAwake { on: boolean; asking: boolean }
export const KEPT_OFF: KeepAwake = { on: false, asking: false }

/** The menu item was chosen: off if on, otherwise on, or the dialog if it was never agreed to. */
export async function keepAwakeChosen(now: KeepAwake, confirmed: () => Promise<boolean>): Promise<KeepAwake> {
  if (now.on) return KEPT_OFF
  return (await confirmed()) ? { on: true, asking: false } : { on: false, asking: true }
}

/** Enable was pressed in the dialog. */
export async function keepAwakeAgreed(confirm: () => Promise<void>): Promise<KeepAwake> {
  await confirm()
  return { on: true, asking: false }
}

/** Asked the first time keeping the computer awake is turned on (CMD-12), and not again. */
export function CaffeinateConfirm({ onCancel, onEnable }: {
  onCancel: () => void
  onEnable: () => void
}): React.JSX.Element {
  return (
    <Modal title="Keep computer awake during turns?" size="sm" className="caffeinate-confirm"
      onClose={onCancel} actions={<>
        <Button kind="plain-faint" size="small" onClick={onCancel} data-test="caffeinate-cancel">
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
