import { Modal } from './Modal'
import { Button } from '../nala'

export function Notice({ title, subtitle, body, onClose }: { title: string; subtitle?: string; body: string; onClose: () => void }): React.JSX.Element {
  return (
    <Modal title={title} subtitle={subtitle} size="md" onClose={onClose} className="notice"
      actions={<Button kind="filled" onClick={onClose} data-test="notice-done">Done</Button>}>
      <pre className="notice-body">{body}</pre>
    </Modal>
  )
}
