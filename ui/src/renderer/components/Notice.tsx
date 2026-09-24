import { Modal } from './Modal'
import { Button } from './ui/button'
export function Notice({ title, body, onClose }: { title: string; body: string; onClose: () => void }): React.JSX.Element {
  return <Modal title={title} onClose={onClose} className="notice">
    <h2>{title}</h2><pre className="notice-body">{body}</pre>
    <div className="notice-actions"><Button className="approve" onClick={onClose}>Done</Button></div>
  </Modal>
}
