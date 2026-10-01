import { useEffect, useRef } from 'react'

/**
 * What a system Back gesture closes: whatever was opened last.
 *
 * A dialog, a menu, the model picker, the drawer and the context panel each put a way to close
 * themselves here while they are open, and Back takes the newest. Escape cannot stand in for this:
 * Leo's dialogs and menus close on a real Escape key through the browser's own handling, which a
 * key event made in script does not reach.
 *
 * Nothing on the desktop calls [`goBack`]. On Android the host calls it for each Back press and
 * leaves the app only when it answers false.
 */
const stack: (() => void)[] = []

/** Close the innermost open thing. False when nothing was open, so Back is the system's. */
export function goBack(): boolean {
  const top = stack.at(-1)
  if (!top) return false
  top()
  return true
}

/**
 * Offer `close` to Back while `active` holds.
 *
 * A dialog that must be answered passes a `close` that does nothing: Back is then consumed rather
 * than leaving the app behind a question, which is what Android does for a dialog that cannot be
 * cancelled.
 */
export function useBack(active: boolean, close: () => void): void {
  const latest = useRef(close)
  latest.current = close
  useEffect(() => {
    if (!active) return
    const entry = (): void => latest.current()
    stack.push(entry)
    return () => {
      const at = stack.lastIndexOf(entry)
      if (at >= 0) stack.splice(at, 1)
    }
  }, [active])
}
