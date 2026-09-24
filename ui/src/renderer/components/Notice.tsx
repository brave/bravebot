import { Modal } from './Modal'
import { Button } from './ui/button'
import { DialogFooter, DialogHeader, DialogTitle } from './ui/dialog'
import { ScrollArea } from './ui/scroll-area'
export function Notice({ title, body, onClose }: { title: string; body: string; onClose: () => void }): React.JSX.Element {
  return <Modal title={title} onClose={onClose} className="notice">
    <DialogHeader><DialogTitle>{title}</DialogTitle></DialogHeader>
    <ScrollArea className="min-h-0 flex-1"><pre className="notice-body">{body}</pre></ScrollArea>
    <DialogFooter className="notice-actions flex-row"><Button className="approve" onClick={onClose}>Done</Button></DialogFooter>
  </Modal>
}
