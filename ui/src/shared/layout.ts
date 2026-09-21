/**
 * The remembered layout, and the one thing that decides whether a file on disk is one.
 *
 * The shape crosses three boundaries — the main process writes it, the preload types it,
 * the renderer reads it back — and each of those used to carry its own guard. Three guards
 * that must agree is three chances to disagree, so the check lives here and the other two
 * import it: a field cannot be added in two places and forgotten in the third.
 */

export interface StoredLayout {
  left: number
  right: number
  collapsed: { left: boolean; right: boolean }
}

const OPEN = { left: false, right: false } as const

/**
 * A layout, or nothing.
 *
 * Widths are the load-bearing part: without two finite numbers there is no layout and the
 * defaults are the right answer. `collapsed` is not — a file written before columns could
 * be folded has no such field, and refusing the whole thing over it would throw away a
 * remembered width to no purpose. Anything unconvincing there means both columns showing,
 * which is what every such file meant when it was written.
 *
 * Nothing is coerced. `collapsed: { left: 1 }` is not a fold, it is a file nobody wrote on
 * purpose, and guessing at it would be how a bad value survives a launch.
 */
export function parseLayout(value: unknown): StoredLayout | null {
  if (typeof value !== 'object' || value === null) return null
  const { left, right, collapsed } = value as {
    left?: unknown
    right?: unknown
    collapsed?: unknown
  }
  if (typeof left !== 'number' || !Number.isFinite(left)) return null
  if (typeof right !== 'number' || !Number.isFinite(right)) return null

  return { left, right, collapsed: folds(collapsed) }
}

function folds(value: unknown): { left: boolean; right: boolean } {
  if (typeof value !== 'object' || value === null) return { ...OPEN }
  const { left, right } = value as { left?: unknown; right?: unknown }
  if (typeof left !== 'boolean' || typeof right !== 'boolean') return { ...OPEN }
  return { left, right }
}
