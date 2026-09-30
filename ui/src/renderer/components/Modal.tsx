import { useEffect, useLayoutEffect, useRef } from 'react'
import { createPortal } from 'react-dom'
import { Dialog } from '../nala'

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
 * Where focus goes back to when a dialog closes: whatever opened it. When that was a menu item,
 * the menu is gone by then, so it is the control the menu hangs from.
 */
function returnTarget(): HTMLElement | null {
  const active = document.activeElement
  if (!(active instanceof HTMLElement)) return null
  const menu = active.closest('leo-buttonmenu')
  const anchor = menu?.querySelector<HTMLElement>('[slot="anchor-content"]')
  return anchor && active.closest('leo-menu-item, [role="menuitem"]') ? anchor : active
}

/** The four widths a dialog comes in: 440, 560, 760 and 1080, each short of the window's edge. */
export type ModalSize = 'sm' | 'md' | 'lg' | 'xl'

/**
 * One focus boundary for every modal.
 *
 * Built on Leo's Dialog, which uses the native `<dialog>` modal mode for focus
 * trapping and inertness — the hand-written trap and `#root.inert` toggling are gone.
 *
 * Every dialog has the same head: the title, then at most one line saying what it is for. The
 * primary action sits at the right-hand end of the footer; a secondary one that should stand
 * apart from it (Cancel, Stop all) carries `modal-leading` and goes to the left.
 */
export function Modal({
  title,
  subtitle,
  subtitleId,
  headerAction,
  size = 'md',
  onClose,
  children,
  actions,
  className = '',
}: {
  title: string
  /** One line under the title saying what the dialog is for. */
  subtitle?: React.ReactNode
  /** For a dialog whose description is referred to from elsewhere. */
  subtitleId?: string
  /** One icon control beside the title, for something about the whole dialog (Refresh). */
  headerAction?: React.ReactNode
  size?: ModalSize
  onClose?: () => void
  children: React.ReactNode
  /** Footer buttons, in Leo's actions slot: pinned under the body, which scrolls on its own. */
  actions?: React.ReactNode
  className?: string
}): React.JSX.Element {
  const previousFocus = useRef<HTMLElement | null>(returnTarget())

  const host = useRef<HTMLElement>(null)

  // Leo closes on any click whose coordinates fall outside the dialog, and a click made from the
  // keyboard (Space or Enter on a button) carries none, so it would close the dialog it is in.
  // The backdrop is the dialog element itself as the target, so that is what closes it here.
  const close = useRef(onClose)
  close.current = onClose
  useEffect(() => {
    const root = host.current
    if (!root) return
    const outside = (event: MouseEvent) => {
      const dialog = root.shadowRoot?.querySelector('dialog')
      if (!close.current || !dialog || event.composedPath()[0] !== dialog) return
      const box = dialog.getBoundingClientRect()
      if (event.clientX < box.left || event.clientX > box.right || event.clientY < box.top || event.clientY > box.bottom) close.current()
    }
    root.addEventListener('click', outside)
    return () => root.removeEventListener('click', outside)
  }, [])

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
      if (subtitleId && root.getAttribute('aria-describedby') !== subtitleId) root.setAttribute('aria-describedby', subtitleId)
      const dialog = root.shadowRoot?.querySelector('dialog')
      if (!dialog) return
      // Leo's own close button is an icon with no name. Name it for what it closes.
      const close = dialog.querySelector<HTMLElement>('.close-button button')
        ?? dialog.querySelector('.close-button leo-button')?.shadowRoot?.querySelector<HTMLElement>('button')
      if (close && close.getAttribute('aria-label') !== `Close ${title}`) close.setAttribute('aria-label', `Close ${title}`)
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
    const sync = () => label()
    sync()
    const shadow = root.shadowRoot
    if (!shadow) return
    const observer = new MutationObserver(sync)
    observer.observe(shadow, { childList: true, subtree: true })
    observer.observe(root, { childList: true, subtree: true })
    return () => observer.disconnect()
  }, [title, subtitleId])

  return createPortal(
    <Dialog
      ref={host}
      isOpen
      modal
      showClose={Boolean(onClose)}
      escapeCloses={Boolean(onClose)}
      backdropClickCloses={false}
      onClose={onClose}
      className={`modal modal-${size} ${className}`.trim()}
      data-test="modal"
    >
      <span slot="title" className="modal-title">{title}{headerAction && <span className="modal-header-action">{headerAction}</span>}</span>
      {subtitle && <span slot="subtitle" className="modal-subtitle" id={subtitleId}>{subtitle}</span>}
      {children}
      {/* Where this dialog's tooltips are drawn. A tooltip added among the dialog's own children makes
          Leo rebuild its slots and put focus back on the close button, so it goes in here. */}
      <div className="modal-tooltips" />
      {actions && <div slot="actions" className="modal-actions">{actions}</div>}
    </Dialog>,
    document.body,
  )
}
