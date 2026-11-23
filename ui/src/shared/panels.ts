/**
 * Which context-column panels a person has turned off, and the one judgement about whether a
 * stored value is such a list (PANEL-20).
 *
 * Kept in `bravebot-ui.json` under `hidden`, apart from the terminal's `~/.bravebot/sidebar`,
 * because the two front ends draw different sections.
 */

import type { PanelName } from './state'

/**
 * The panels a person may turn off, in the order the column draws them.
 *
 * These are the Overview panels. `files` is left out because it is the whole Files tab, and
 * turning it off would leave that tab blank.
 */
export const HIDEABLE = ['plan', 'read', 'writes', 'confined', 'notes', 'asides'] as const satisfies readonly PanelName[]

const known = new Set<string>(HIDEABLE)

/**
 * The hidden panels, or `[]` for every panel on.
 *
 * Anything that is not an array, or an array holding any name that is not in `HIDEABLE`, is `[]`.
 * A panel a later version adds is therefore on, with no migration. Duplicates are dropped and the
 * result is in column order. Each stored name is only compared with the fixed list; the result is
 * built from `HIDEABLE`, so nothing read from the file is returned.
 */
export function parseHidden(value: unknown): PanelName[] {
  if (!Array.isArray(value)) return []
  if (!value.every((name) => typeof name === 'string' && known.has(name))) return []
  const chosen = new Set<unknown>(value)
  return HIDEABLE.filter((name) => chosen.has(name))
}

/**
 * The list after a person ticks or unticks `name` in the Choose sections menu: `name` hidden if it
 * was shown and shown if it was hidden, every other panel as it was, in column order.
 */
export function toggleHidden(hidden: readonly PanelName[], name: PanelName): PanelName[] {
  const wasHidden = hidden.includes(name)
  return HIDEABLE.filter((other) => other === name ? !wasHidden : hidden.includes(other))
}
