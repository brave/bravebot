import { SCRIM } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'

export function Notice({ title, body, onClose }: { title: string; body: string; onClose: () => void }): React.JSX.Element {
  return (
    <Dialog open onOpenChange={(next) => { if (!next) onClose() }}>
      {/* The trust dialog's card, but wider and scrollable: this holds output from
          `bravebot doctor`, whose length nobody controls. */}
      <DialogContent
        className="modal notice flex max-h-[70vh] w-[560px] flex-col sm:max-w-none"
        showCloseButton={false}
        overlayClassName={SCRIM}
      >
        <DialogHeader>
          <DialogTitle className="text-[15px]">{title}</DialogTitle>
        </DialogHeader>
        <pre className="notice-body m-0 min-h-0 flex-1 overflow-auto rounded-sm bg-bubble-agent px-3 py-2.5 font-mono text-[11px] leading-[1.55] break-words whitespace-pre-wrap">{body}</pre>
        <DialogFooter className="notice-actions mx-0 mb-0 flex-row justify-end rounded-none border-0 bg-transparent p-0">
          <Button className="approve" onClick={onClose}>Done</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
