import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { Tooltip } from '../nala'

/** How long the pointer rests before the first tooltip, and how long after one closes the next opens at once. */
const FIRST_DELAY = 1000
const WARM_FOR = 800

/** Leo's visible bubble takes the pointer, which would leave whatever lies under it unclickable. */
let passThrough: CSSStyleSheet | null = null
const passThroughSheet = (): CSSStyleSheet => {
  if (!passThrough) {
    passThrough = new CSSStyleSheet()
    passThrough.replaceSync('.tooltip, .tooltip.visible { pointer-events: none !important; }')
  }
  return passThrough
}

interface Shown {
  text: string
  shortcut: string | null
  placement: 'top' | 'bottom' | 'left' | 'right'
  box: DOMRect
  /** A modal dialog is in the top layer, above anything on the body, so its tooltips are drawn inside it, in the box it keeps for them. */
  host: Element
}

/**
 * The one tooltip in the window.
 *
 * A Leo Tooltip per control would mean one floating element per control, each re-positioning on
 * every scroll of every ancestor, and a transcript has hundreds of controls. This draws a single
 * Leo Tooltip over whichever `[data-tooltip]` element the pointer is resting on or the keyboard
 * has reached, anchored to a box laid exactly over it. The delay is the system's: a rest before
 * the first, then instant while moving along a toolbar.
 */
export function TooltipLayer(): React.JSX.Element | null {
  const [shown, setShown] = useState<Shown | null>(null)
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const current = useRef<Element | null>(null)
  const closedAt = useRef(0)

  useEffect(() => {
    const clear = () => { if (timer.current) clearTimeout(timer.current); timer.current = null }
    const hide = () => {
      clear()
      if (shown !== null) closedAt.current = Date.now()
      current.current = null
      setShown(null)
    }
    const read = (target: Element): Shown | null => {
      const text = target.getAttribute('data-tooltip')
      if (!text) return null
      const placement = target.getAttribute('data-tooltip-placement')
      return {
        text,
        shortcut: target.getAttribute('data-tooltip-shortcut'),
        placement: placement === 'bottom' || placement === 'left' || placement === 'right' ? placement : 'top',
        box: target.getBoundingClientRect(),
        host: target.closest('leo-dialog')?.querySelector(':scope > .modal-tooltips') ?? document.body,
      }
    }
    const show = (target: Element, immediate: boolean) => {
      if (target === current.current) return
      clear()
      current.current = target
      const warm = Date.now() - closedAt.current < WARM_FOR || shown !== null
      const open = () => {
        if (current.current !== target || !target.isConnected) return
        if (target.matches(':disabled, [aria-disabled="true"]')) return
        setShown(read(target))
      }
      if (immediate || warm) open()
      else timer.current = setTimeout(open, FIRST_DELAY)
    }
    const owner = (event: Event): Element | null => {
      for (const node of event.composedPath()) {
        if (node instanceof Element && node.hasAttribute('data-tooltip')) return node
        if (node === document.body) break
      }
      return null
    }
    const over = (event: PointerEvent) => {
      if (event.pointerType === 'touch') return
      const target = owner(event)
      if (target) show(target, false)
      else if (current.current) hide()
    }
    const focus = (event: FocusEvent) => {
      const target = owner(event)
      // Keyboard focus only. A click focuses the control as well, and the tooltip that follows
      // a click is one nobody asked for.
      const keyboard = document.querySelector(':focus-visible') !== null
        || (event.target instanceof Element && event.target.shadowRoot?.querySelector(':focus-visible') !== null)
      if (target && keyboard) show(target, true)
    }
    const blur = (event: FocusEvent) => { if (owner(event) === current.current) hide() }
    const key = (event: KeyboardEvent) => { if (event.key === 'Escape' && current.current) hide() }

    document.addEventListener('pointerover', over, true)
    document.addEventListener('pointerdown', hide, true)
    document.addEventListener('focusin', focus, true)
    document.addEventListener('focusout', blur, true)
    document.addEventListener('keydown', key, true)
    document.addEventListener('scroll', hide, true)
    window.addEventListener('blur', hide)
    window.addEventListener('resize', hide)
    return () => {
      clear()
      document.removeEventListener('pointerover', over, true)
      document.removeEventListener('pointerdown', hide, true)
      document.removeEventListener('focusin', focus, true)
      document.removeEventListener('focusout', blur, true)
      document.removeEventListener('keydown', key, true)
      document.removeEventListener('scroll', hide, true)
      window.removeEventListener('blur', hide)
      window.removeEventListener('resize', hide)
    }
  }, [shown])

  // A control that goes away under the pointer takes its tooltip with it.
  useEffect(() => {
    if (!shown) return
    const check = setInterval(() => {
      if (!current.current?.isConnected) { current.current = null; setShown(null) }
    }, 250)
    return () => clearInterval(check)
  }, [shown])

  const tooltip = useRef<HTMLElement>(null)
  useLayoutEffect(() => {
    const root = tooltip.current?.shadowRoot
    if (!root) return
    const sheet = passThroughSheet()
    if (!root.adoptedStyleSheets.includes(sheet)) root.adoptedStyleSheets = [...root.adoptedStyleSheets, sheet]
  })

  if (!shown) return null
  const { box } = shown
  return createPortal(
    <div
      className="tooltip-anchor"
      aria-hidden="true"
      style={{ left: box.left, top: box.top, width: box.width, height: box.height }}
    >
      <Tooltip ref={tooltip} visible mode="mini" placement={shown.placement} positionStrategy="fixed" offset={6}>
        <div slot="content" className="tooltip-text" data-veil="slot">
          {shown.text}
          {shown.shortcut && <kbd className="tooltip-shortcut">{shown.shortcut}</kbd>}
        </div>
        <div className="tooltip-target" style={{ width: box.width, height: box.height }} />
      </Tooltip>
    </div>,
    shown.host,
  )
}
