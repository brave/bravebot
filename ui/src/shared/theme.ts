/**
 * How this window follows light and dark.
 *
 * Three choices: follow the system, force light, or force dark. That is the whole of Nala's
 * theme surface — Leo switches on `prefers-color-scheme` or a `data-theme="light|dark"`
 * ancestor — and it replaces the twenty-two named palettes this app used to carry.
 *
 * Old stored values (`brave`, `system`, or any palette name) all become `system`, so a
 * preference from a previous build still paints something rather than nothing.
 *
 * Nothing here imports `electron` or `react`: the main process writes the choice and the
 * renderer paints from it, and neither should have to reach through the other for the
 * definition of what an appearance is.
 */

export const APPEARANCES = ['system', 'light', 'dark'] as const

export type Appearance = (typeof APPEARANCES)[number]

/** The default: follow the OS between light and dark. */
export const SYSTEM: Appearance = 'system'

/** The longest an appearance name may be. Longer is not a choice; it is rubbish. */
const MAX_APPEARANCE_BYTES = 64

/**
 * Which appearance is chosen, out of the remembered state.
 *
 * Never null: no key, a key with rubbish in it, a blank, and every legacy palette name all
 * describe the same window — follow the system. Telling them apart would hand the caller a
 * decision it does not have to make.
 */
export function parseAppearance(value: unknown): Appearance {
  if (typeof value !== 'string') return SYSTEM
  const name = value.trim()
  if (name.length === 0 || name.length > MAX_APPEARANCE_BYTES) return SYSTEM
  if (name === 'light' || name === 'dark') return name
  // `brave` was the old default (system-following macOS palette). Every other legacy
  // palette name falls back the same way: follow the system rather than invent a mapping.
  return SYSTEM
}

/** @deprecated Prefer `parseAppearance`. Kept so older call sites and tests still compile while they move. */
export const parseChosenTheme = parseAppearance
