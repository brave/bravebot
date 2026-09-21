import { useEffect, useRef } from 'react'
import { createPortal } from 'react-dom'

/** One focus boundary for every modal. Background content cannot receive input. */
export function Modal({ title, onClose, children, className = '' }: {
  title: string; onClose?: () => void; children: React.ReactNode; className?: string
}): React.JSX.Element {
  const root = useRef<HTMLDivElement>(null)
  const previousFocus = useRef(document.activeElement as HTMLElement | null)
  const close = useRef(onClose)
  close.current = onClose
  useEffect(() => {
    const previous = previousFocus.current
    const app = document.getElementById('root')
    const wasInert = app?.inert ?? false
    if (app) app.inert = true
    const controls = () => [...(root.current?.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), textarea:not(:disabled), select, a[href], [tabindex="0"]') ?? [])].filter((node) => node.getClientRects().length)
    if (!root.current?.contains(document.activeElement)) (controls()[0] ?? root.current)?.focus()
    const key = (event: KeyboardEvent) => {
      // A saving form may disable its focused button and move focus to body. Keep
      // Escape and Tab within the topmost modal even through that transition.
      if ([...document.querySelectorAll('[role="dialog"]')].at(-1) !== root.current) return
      if (event.key === 'Escape' && close.current) { event.preventDefault(); event.stopPropagation(); close.current() }
      if (event.key !== 'Tab') return
      const items = controls(), first = items[0], last = items.at(-1)
      if (!first) { event.preventDefault(); root.current?.focus(); return }
      if (!root.current?.contains(document.activeElement)) { event.preventDefault(); first.focus(); return }
      if (event.shiftKey && (document.activeElement === first || document.activeElement === root.current)) { event.preventDefault(); last?.focus() }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus() }
    }
    document.addEventListener('keydown', key, true)
    return () => {
      document.removeEventListener('keydown', key, true)
      if (app) app.inert = wasInert
      if (previous?.isConnected) previous.focus()
    }
  }, [])
  return createPortal(<div className="scrim" onMouseDown={(event) => {
    if (event.target === event.currentTarget) onClose?.()
  }}><div ref={root} tabIndex={-1} role="dialog" aria-modal="true" aria-label={title} className={`modal ${className}`}>
    {children}
  </div></div>, document.body)
}
