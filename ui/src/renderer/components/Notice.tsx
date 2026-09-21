import { Modal } from './Modal'
export function Notice({ title, body, onClose }: { title: string; body: string; onClose: () => void }): React.JSX.Element {
  return <Modal title={title} onClose={onClose} className="notice">
    <h2>{title}</h2><pre className="notice-body">{body}</pre>
    <div className="notice-actions"><button className="approve" onClick={onClose}>Done</button></div>
  </Modal>
}
