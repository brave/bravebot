import { useEffect, useLayoutEffect, useRef } from 'react'
import { createPortal } from 'react-dom'
import { Dialog } from '../nala'

// A wrapping <label> does not name Leo's inner field: the host is not a
// labelable element. Copy the label text onto that field so the accessible
// name matches the text beside it.
const nameWrappedFields = (root: HTMLElement) => {
  for (const label of root.querySelectorAll('label')) {
    const name = [...label.childNodes].filter((node) => node.nodeType === Node.TEXT_NODE).map((node) => node.textContent ?? '').join('').trim()
    if (!name) continue
    const field = label.querySelector('leo-input, leo-textarea')?.shadowRoot?.querySelector('input, textarea')
    if (field && field.getAttribute('aria-label') !== name) field.setAttribute('aria-label', name)
  }
}

const focusableControls = (dialog: HTMLElement): HTMLElement[] => {
  const selector = 'button:not(:disabled), a[href], input:not(:disabled), select:not(:disabled), textarea:not(:disabled)'
  const found: HTMLElement[] = []
  const visit = (node: Node) => {
    if (node instanceof ShadowRoot) {
      for (const child of node.children) visit(child)
      return
    }
    if (!(node instanceof HTMLElement)) return
    if (node.matches(selector) && node.getClientRects().length) found.push(node)
    if (node.shadowRoot) {
      visit(node.shadowRoot)
      return
    }
    for (const child of node.children) {
      if (child instanceof HTMLSlotElement) {
        for (const assigned of child.assignedElements()) visit(assigned)
      } else visit(child)
    }
  }
  visit(dialog)
  return found
}

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
  const previousFocus = useRef<HTMLElement | null>(
    document.activeElement instanceof HTMLElement ? document.activeElement : null,
  )

  const host = useRef<HTMLElement>(null)

  useEffect(() => () => {
    const previous = previousFocus.current
    if (previous?.isConnected) previous.focus({ preventScroll: true })
  }, [])

  // Leo's native <dialog> lives in the shadow root, so its slotted content is
  // not text inside the element Playwright names. The host is the dialog the
  // page queries, and the inner element gives up that role. A slot change
  // rebuilds the inner element, which drops the role until it is set again.
  useLayoutEffect(() => {
    const root = host.current
    if (!root) return
    const label = () => {
      if (root.getAttribute('role') !== 'dialog') root.setAttribute('role', 'dialog')
      if (root.getAttribute('aria-modal') !== 'true') root.setAttribute('aria-modal', 'true')
      if (root.getAttribute('aria-label') !== title) root.setAttribute('aria-label', title)
      const dialog = root.shadowRoot?.querySelector('dialog')
      if (!dialog) return
      if (dialog.getAttribute('role') !== 'presentation') dialog.setAttribute('role', 'presentation')
      if (dialog.dataset.tabWrap === '1') return
      dialog.dataset.tabWrap = '1'
      let tabbedAt = 0
      let shiftTab = false
      root.addEventListener('keydown', (event) => {
        if (event.key !== 'Tab') return
        tabbedAt = Date.now()
        shiftTab = event.shiftKey
      }, true)
      // The native dialog is its own tab stop. Only a Tab that lands on the
      // dialog itself is sent on to the first or last control.
      let moving = false
      dialog.addEventListener('focus', () => {
        if (moving) return
        if (Date.now() - tabbedAt <= 50) {
          const controls = focusableControls(dialog)
          const target = shiftTab ? controls.at(-1) : controls[0]
          if (!target) return
          moving = true
          target.focus()
          moving = false
          return
        }
        // Arrow keys select a tab, then the dialog element takes focus back.
        // Return focus to the selected tab once this focus event has finished.
        queueMicrotask(() => {
          if (root.shadowRoot?.activeElement !== dialog) return
          root.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]')?.focus()
        })
      })
    }
    const sync = () => { label(); nameWrappedFields(root) }
    sync()
    const shadow = root.shadowRoot
    if (!shadow) return
    const observer = new MutationObserver(sync)
    observer.observe(shadow, { childList: true, subtree: true })
    observer.observe(root, { childList: true, subtree: true })
    return () => observer.disconnect()
  }, [title])

  return createPortal(
    <Dialog
      ref={host}
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
