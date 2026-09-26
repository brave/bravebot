/**
 * Putting a palette on the window.
 *
 * Plain DOM rather than React, and deliberately: the picker previews a theme on every arrow key,
 * and a preview that went through a render would repaint the transcript to change the colour of
 * its background. The same reasoning `App.tsx` gives for setting the column widths as custom
 * properties on the root element — a drag has to be cheap, and so does this.
 *
 * What lands on the root is one class, optional attributes, and twelve properties. The class is
 * the dark-mode switch Tailwind/shadcn read (`@custom-variant dark (&:is(.dark *))`). The
 * `data-theme` attribute is the switch for named palettes: `globals.css` derives its colour
 * tokens from the `--role-*` properties inside a `:root[data-theme]` block, and with no
 * attribute that block does not apply and the window is the stock `brave` palette. That is
 * what `brave` is here — not a theme that happens to match, but the absence of one.
 *
 * ## The one rule about the offscreen window
 *
 * Nothing in this module may be called from `export.tsx`. The PDF is pinned light because the
 * export entry never imports this module, so `.dark` is never set there and named themes never
 * land. A session exported at night, in Nord, still comes out white on paper.
 */

import { BRAVE, roleVariables, paintsBackground, type Theme } from '../shared/theme'

const DARK = '(prefers-color-scheme: dark)'

/** Whether the system is asking for dark, which is what an inherited role resolves against. */
function dark(): boolean {
  return window.matchMedia(DARK).matches
}

/** Toggle the `.dark` class Tailwind's dark variant reads. */
function setDarkClass(on: boolean): void {
  document.documentElement.classList.toggle('dark', on)
}

/**
 * Paint the window in a theme.
 *
 * `brave` clears everything rather than writing the app's own values back over themselves, so that
 * there is no state to get wrong: the window after choosing `brave` is the same document as the
 * window that has never been themed. Under `brave`, dark mode follows the system; under a named
 * theme it follows that theme's computed scheme.
 */
export function applyTheme(theme: Theme): void {
  const root = document.documentElement
  if (theme.name === BRAVE) {
    root.removeAttribute('data-theme')
    root.removeAttribute('data-ground')
    for (const name of [...root.style]) {
      if (name.startsWith('--role-')) root.style.removeProperty(name)
    }
    setDarkClass(dark())
    return
  }
  const variables = roleVariables(theme, dark())
  for (const [name, value] of Object.entries(variables)) {
    root.style.setProperty(name, value)
  }
  root.setAttribute('data-theme', theme.name)
  // Whether the theme paints its own ground, which is the one decision that costs something
  // visible: the window is drawn over a native sidebar blur, and an opaque background covers it.
  // A palette that inherits its background — `"background": "none"`, or the key left out — keeps
  // the blur, so a three-line palette that only changes the accent does not flatten the window.
  if (paintsBackground(theme)) root.setAttribute('data-ground', 'own')
  else root.removeAttribute('data-ground')
  setDarkClass(variables['--role-scheme'] === 'dark')
}

/**
 * Re-resolve the theme when the system flips between light and dark. Returns an unsubscribe.
 *
 * Only a partial palette notices — a full one names all nine roles and looks the same either way —
 * but a palette that sets an accent and inherits the rest would otherwise keep resolving against
 * the appearance that was in force when it was applied, and go unreadable at sunset.
 */
export function watchAppearance(current: () => Theme): () => void {
  const query = window.matchMedia(DARK)
  const handler = (): void => applyTheme(current())
  query.addEventListener('change', handler)
  // Apply once so `.dark` matches the system before any theme is chosen.
  applyTheme(current())
  return () => query.removeEventListener('change', handler)
}
