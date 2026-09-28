import { Modal } from './Modal'
import { Button } from '../nala'

export function Notice({ title, body, onClose }: { title: string; body: string; onClose: () => void }): React.JSX.Element {
  return (
    <Modal title={title} onClose={onClose} className="notice">
      <pre className="notice-body">{body}</pre>
      <div className="notice-actions">
        <Button size="small" kind="filled" onClick={onClose} data-test="notice-done">Done</Button>
      </div>
    </Modal>
  )
}
