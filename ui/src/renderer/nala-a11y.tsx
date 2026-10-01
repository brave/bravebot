/**
 * Leo's React wrappers hand only the Svelte component's own props to its custom element; every
 * other prop is written to the element as a JS property nothing reads. So `aria-label`,
 * `aria-expanded`, `aria-pressed`, `aria-controls` and the rest never become attributes, on the
 * host or on the button/input inside its shadow root. An icon-only `<Button aria-label="Close">`
 * therefore has no accessible name, and a toggle's state is invisible to assistive technology.
 * `title` is a real Leo prop, so it reaches the host but not the control inside.
 *
 * `withShadowAttrs` puts those attributes where they work: on the inner control, which is what
 * the accessibility tree exposes, and on the host, which is what tests and CSS selectors read.
 * Leo rebuilds its inner control when slotted content arrives, so they are re-applied when the
 * shadow tree changes. Call sites stay plain JSX.
 *
 * Input and TextArea also draw their field with a positive `tabindex="1"`, which puts every one
 * of them ahead of the whole page in the tab order; `flattenTabindex` sets it back to 0.
 */
import { forwardRef, useCallback, useEffect, useRef, type ComponentProps, type ComponentType, type ReactElement } from 'react'

type DistributiveOmit<T, K extends PropertyKey> = T extends unknown ? Omit<T, K> : never
type AttrProps = { [K in `aria-${string}`]?: string | number | boolean | undefined } & { title?: string }

const forwarded = (key: string): boolean => key.startsWith('aria-') || key === 'title'

function write(el: Element, name: string, value: unknown): void {
  // `aria-*="false"` is a value, not an absence; anything else empty is removed.
  if (value == null || value === '' || (value === false && !name.startsWith('aria-'))) el.removeAttribute(name)
  else el.setAttribute(name, String(value))
}

/** How many animation frames to wait for Leo to attach and render its shadow tree. */
const RENDER_FRAMES = 20

export function withShadowAttrs<C extends (props: any) => ReactElement>(Leo: C, innerSelector: string, options: { flattenTabindex?: boolean } = {}) {
  const Component = Leo as unknown as ComponentType<Record<string, unknown>>
  type Props = DistributiveOmit<ComponentProps<C>, 'ref'> & AttrProps

  return forwardRef<HTMLElement, Props>(function ShadowAttrs(props, forwardedRef) {
    const host = useRef<HTMLElement | null>(null)
    const attrs: Record<string, unknown> = {}
    const rest: Record<string, unknown> = {}
    for (const [key, value] of Object.entries(props as Record<string, unknown>)) {
      if (forwarded(key)) attrs[key] = value
      // `title` is also Leo's own prop (it draws the host tooltip), so it goes both ways.
      if (!key.startsWith('aria-')) rest[key] = value
    }
    const latest = useRef(attrs)
    latest.current = attrs
    const signature = JSON.stringify(Object.entries(attrs))

    const setRef = useCallback((node: HTMLElement | null) => {
      host.current = node
      if (typeof forwardedRef === 'function') forwardedRef(node)
      else if (forwardedRef) forwardedRef.current = node
    }, [forwardedRef])

    useEffect(() => {
      const node = host.current
      if (!node) return
      let frame = 0
      let tries = 0
      const observer = new MutationObserver(() => queueMicrotask(apply))
      let watching: ShadowRoot | null = null
      const applied = new Set<string>()

      function apply(): void {
        const current = host.current
        if (!current) return
        const inner = current.shadowRoot?.querySelector(innerSelector) ?? null
        // Leo's Input and TextArea hard-code tabindex="1" on the field they draw. A positive
        // tabindex jumps the field ahead of everything else on the page in the tab order.
        if (options.flattenTabindex && inner?.getAttribute('tabindex') === '1') inner.setAttribute('tabindex', '0')
        for (const name of applied) {
          if (name in latest.current) continue
          current.removeAttribute(name)
          inner?.removeAttribute(name)
          applied.delete(name)
        }
        for (const [name, value] of Object.entries(latest.current)) {
          applied.add(name)
          write(current, name, value)
          if (inner) write(inner, name, value)
        }
        if (current.shadowRoot && current.shadowRoot !== watching) {
          watching = current.shadowRoot
          observer.observe(watching, { childList: true, subtree: true })
        }
        // Leo has not drawn yet: try again next frame, for a little while.
        if (!inner && tries++ < RENDER_FRAMES) frame = requestAnimationFrame(apply)
      }

      apply()
      observer.observe(node, { childList: true })
      return () => {
        cancelAnimationFrame(frame)
        observer.disconnect()
      }
    }, [signature])

    return <Component ref={setRef} {...rest} />
  })
}

/**
 * Leo's ButtonMenu wraps whatever sits in its anchor slot in `<div role="button" tabindex="0"
 * aria-haspopup aria-expanded>`. Every anchor here is already a Button, so the pair reads as a
 * button inside a button: two tab stops, two announcements, and a name the outer one borrows
 * from the inner. The inner Button carries the name and the popup state (its props say so);
 * the wrapper is kept only as the element Leo focuses when a menu closes from the keyboard, so
 * it stays focusable by script (`tabindex="-1"`) and drops out of the accessibility tree.
 * Leo re-renders `aria-expanded` on the wrapper as the menu opens, so this watches for that.
 */
export function withPlainMenuAnchor<C extends (props: any) => ReactElement>(Leo: C) {
  const Component = Leo as unknown as ComponentType<Record<string, unknown>>
  type Props = DistributiveOmit<ComponentProps<C>, 'ref'>

  return forwardRef<HTMLElement, Props>(function PlainMenuAnchor(props, forwardedRef) {
    const host = useRef<HTMLElement | null>(null)
    const setRef = useCallback((node: HTMLElement | null) => {
      host.current = node
      if (typeof forwardedRef === 'function') forwardedRef(node)
      else if (forwardedRef) forwardedRef.current = node
    }, [forwardedRef])

    useEffect(() => {
      const node = host.current
      if (!node) return
      let frame = 0
      let tries = 0
      const observer = new MutationObserver(() => queueMicrotask(apply))
      let watching: ShadowRoot | null = null

      function apply(): void {
        const root = host.current?.shadowRoot
        if (!root) {
          if (tries++ < RENDER_FRAMES) frame = requestAnimationFrame(apply)
          return
        }
        if (root !== watching) {
          watching = root
          observer.observe(root, { childList: true, subtree: true, attributes: true, attributeFilter: ['role', 'tabindex', 'aria-expanded', 'aria-haspopup'] })
        }
        const anchor = root.querySelector<HTMLElement>('.leo-button-menu > div')
        if (!anchor) {
          if (tries++ < RENDER_FRAMES) frame = requestAnimationFrame(apply)
          return
        }
        // Idempotent, so the attribute changes made here do not loop the observer.
        if (anchor.getAttribute('role') !== 'presentation') anchor.setAttribute('role', 'presentation')
        if (anchor.getAttribute('tabindex') !== '-1') anchor.setAttribute('tabindex', '-1')
        if (anchor.hasAttribute('aria-expanded')) anchor.removeAttribute('aria-expanded')
        if (anchor.hasAttribute('aria-haspopup')) anchor.removeAttribute('aria-haspopup')
      }

      apply()
      return () => {
        cancelAnimationFrame(frame)
        observer.disconnect()
      }
    }, [])

    return <Component ref={setRef} {...(props as Record<string, unknown>)} />
  })
}
