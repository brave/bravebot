import { useSyncExternalStore } from 'react'

/**
 * Something that finished, said once in the corner and then let go. Failures do not come here.
 *
 * `note` is for something the window chose not to do on a person's behalf, such as a folder left
 * out of a drop: not a failure, and not a success to tick either.
 */
export interface Toast {
  id: number
  title: string
  body?: string
  kind: 'done' | 'note'
}

/** Long enough to read a path; short enough that a second copy does not queue behind the first. */
const SHOWN_MS = 4000
/** A burst of copies shows the latest few, not a column of identical confirmations. */
const MOST = 3

let toasts: readonly Toast[] = []
let next = 1
const listeners = new Set<() => void>()
const tell = () => { for (const listener of listeners) listener() }

export function showToast(title: string, body?: string, kind: Toast['kind'] = 'done'): void {
  const id = next++
  toasts = [...toasts.filter((toast) => toast.title !== title || toast.body !== body), { id, title, body, kind }].slice(-MOST)
  tell()
  setTimeout(() => dismissToast(id), SHOWN_MS)
}

export function dismissToast(id: number): void {
  if (!toasts.some((toast) => toast.id === id)) return
  toasts = toasts.filter((toast) => toast.id !== id)
  tell()
}

export function useToasts(): readonly Toast[] {
  return useSyncExternalStore(
    (listener) => { listeners.add(listener); return () => { listeners.delete(listener) } },
    () => toasts,
  )
}
