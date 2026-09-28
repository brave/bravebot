import { createPortal } from 'react-dom'
import { Dialog } from '../nala'

/**
 * One focus boundary for every modal.
 *
 * Built on Leo's Dialog, which uses the native `<dialog>` modal mode for focus
 * trapping and inertness — the hand-written trap and `#root.inert` toggling are gone.
 * Props stay the same so callers do not have to move.
 */
export function Modal({
  title,
  onClose,
  children,
  className = '',
}: {
  title: string
  onClose?: () => void
  children: React.ReactNode
  className?: string
}): React.JSX.Element {
  return createPortal(
    <Dialog
      isOpen
      modal
      showClose={Boolean(onClose)}
      escapeCloses={Boolean(onClose)}
      backdropClickCloses={Boolean(onClose)}
      onClose={onClose}
      className={`modal ${className}`.trim()}
      data-test="modal"
    >
      <span slot="title">{title}</span>
      {children}
    </Dialog>,
    document.body,
  )
}
