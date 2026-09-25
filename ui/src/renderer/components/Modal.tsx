import { Dialog, DialogClose, DialogContent, DialogTitle } from './ui/dialog'
import { useEffect, useRef } from 'react'

export { DialogClose }

/** One focus boundary for every modal. Background content cannot receive input. */
export function Modal({ title, onClose, children, className = '' }: {
  title: string; onClose?: () => void; children: React.ReactNode; className?: string
}): React.JSX.Element {
  const previousFocus = useRef<HTMLElement | null>(
    typeof document === 'undefined' ? null : document.activeElement as HTMLElement | null,
  )
  useEffect(() => () => {
    if (previousFocus.current?.isConnected) previousFocus.current.focus()
  }, [])

  return <Dialog open onOpenChange={(open) => { if (!open) onClose?.() }}>
    <DialogContent
      className={`modal max-h-[calc(100dvh-40px)] w-[min(680px,calc(100vw-40px))] overflow-y-auto ${className}`}
      overlayClassName="scrim"
      showCloseButton={false}
      aria-describedby={undefined}
      onEscapeKeyDown={(event) => { if (!onClose) event.preventDefault() }}
      onPointerDownOutside={(event) => { if (!onClose) event.preventDefault() }}
    >
      <DialogTitle className="sr-only">{title}</DialogTitle>
      {children}
    </DialogContent>
  </Dialog>
}
