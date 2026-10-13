import { useCallback, useEffect, useLayoutEffect, useRef, useState, type RefObject } from 'react'

/**
 * A function whose identity never changes and which always calls the latest `fn`.
 *
 * For handing callbacks to memoised children: a new arrow on every render would re-render every
 * row of a transcript on every keystroke in the composer.
 */
export function useEvent<A extends unknown[], R>(fn: (...args: A) => R): (...args: A) => R {
  const latest = useRef(fn)
  useLayoutEffect(() => { latest.current = fn })
  return useCallback((...args: A) => latest.current(...args), [])
}

/**
 * `value`, but only once it has held for `delay` milliseconds.
 *
 * For spinners and skeletons: an answer that arrives in 40ms should not flash a loading state on
 * its way past. Falling back to false is immediate.
 */
export function useDelayedFlag(value: boolean, delay = 150): boolean {
  const [shown, setShown] = useState(false)
  useEffect(() => {
    if (!value) { setShown(false); return }
    const timer = setTimeout(() => setShown(true), delay)
    return () => clearTimeout(timer)
  }, [value, delay])
  return value && shown
}

/**
 * The previous `value` for as long as it serialises the same.
 *
 * For derived data rebuilt on every render of `App` — the session list, the per-row status —
 * which would otherwise hand memoised children a new object each keystroke. Only for plain,
 * JSON-shaped values.
 */
export function useStableValue<T>(value: T): T {
  const held = useRef<{ json: string; value: T } | null>(null)
  const json = JSON.stringify(value)
  if (!held.current || held.current.json !== json) held.current = { json, value }
  return held.current.value
}

/**
 * Sizes the field Leo's TextArea draws to its content, between `minRows` and `maxRows`, and lets it
 * scroll past that.
 *
 * Leo works out its rows from the newlines in the text and only while somebody types, so a long
 * line that wraps, or a value set from outside, is cut off at `minRows`. The field is measured
 * again when its value or its width changes.
 */
export function useFitTextArea(host: React.RefObject<HTMLElement | null>, value: string, minRows: number, maxRows: number): void {
  useLayoutEffect(() => {
    const node = host.current
    if (!node) return
    let frame = 0
    let tries = 0
    let observed: Element | null = null
    const observer = new ResizeObserver(() => fit())
    function fit(): void {
      const field = node?.shadowRoot?.querySelector('textarea')
      if (!field) { if (tries++ < 20) frame = requestAnimationFrame(fit); return }
      if (observed !== field) { observer.observe(field); observed = field }
      const line = parseFloat(getComputedStyle(field).lineHeight)
      if (!Number.isFinite(line)) return
      const chrome = field.offsetHeight - field.clientHeight
      field.style.height = 'auto'
      const wanted = Math.min(Math.max(field.scrollHeight, line * minRows), line * maxRows) + chrome
      field.style.height = `${wanted}px`
    }
    fit()
    return () => { cancelAnimationFrame(frame); observer.disconnect() }
  }, [host, value, minRows, maxRows])
}

/**
 * Puts back the role of every checkable item in the menu `trigger` opens.
 *
 * Leo stamps `role="menuitem"` on every item each time a `ButtonMenu` opens, so a checkable item
 * declares its role as `data-role` and gets it back here.
 */
export function useMenuItemRoles(trigger: RefObject<HTMLElement | null>): void {
  useEffect(() => {
    const menu = trigger.current?.parentElement
    if (!menu) return
    const restore = (): void => {
      for (const item of menu.querySelectorAll<HTMLElement>('leo-menu-item[data-role]')) {
        if (item.getAttribute('role') !== item.dataset.role) item.setAttribute('role', item.dataset.role!)
      }
    }
    restore()
    const watch = new MutationObserver(restore)
    watch.observe(menu, { subtree: true, childList: true, attributes: true, attributeFilter: ['role'] })
    return () => watch.disconnect()
  }, [])
}

/**
 * What a `ButtonMenu` needs to decide whether closing returns focus to `trigger`.
 *
 * Escape and a choice return it; a click elsewhere (reason `blur`) leaves focus where the pointer
 * landed. Pass `onClose` to the menu and call `restoreFocus` when it reports closed.
 */
export function useFocusReturn(trigger: RefObject<HTMLElement | null>): {
  onClose: (detail: { reason: string }) => void
  restoreFocus: () => void
} {
  const reason = useRef('explicit')
  return {
    onClose: (detail) => { reason.current = detail.reason },
    restoreFocus: () => {
      if (reason.current !== 'blur') trigger.current?.focus()
      reason.current = 'explicit'
    },
  }
}
