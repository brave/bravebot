/**
 * Putting a palette on the window.
 *
 * Plain DOM rather than React, and deliberately: the picker previews a theme on every arrow key,
 * and a preview that went through a render would repaint the transcript to change the colour of
 * its background. The same reasoning `App.tsx` gives for setting the column widths as custom
 * properties on the root element — a drag has to be cheap, and so does this.
 *
 * What lands on the root is one attribute and twelve properties. The attribute is the switch:
 * `styles.css` derives its nineteen colour tokens from the properties inside a `:root[data-theme]`
 * block, and with no attribute that block does not apply and the window is the macOS palette it
 * has always been. That is what `brave` is here — not a theme that happens to match, but the
 * absence of one.
 *
 * ## The one rule about the offscreen window
 *
 * Nothing in this module may be called from `export.tsx`. The PDF is pinned light by source order:
 * `export.css` re-declares the tokens in a plain `:root` block that beats the dark `@media` rule in
 * `styles.css` only because they have the same specificity. `:root[data-theme]` outranks both. The
 * print window stays on `brave` because the attribute is never set there, and that is the whole of
 * the mechanism — a session exported at night, in Nord, still comes out white on paper.
 */

import { BRAVE, roleVariables, paintsBackground, type Theme } from '../shared/theme'

const DARK = '(prefers-color-scheme: dark)'

/** Whether the system is asking for dark, which is what an inherited role resolves against. */
function dark(): boolean {
  return window.matchMedia(DARK).matches
}

/**
 * Paint the window in a theme.
 *
 * `brave` clears everything rather than writing the app's own values back over themselves, so that
 * there is no state to get wrong: the window after choosing `brave` is the same document as the
 * window that has never been themed.
 */
export function applyTheme(theme: Theme): void {
  const root = document.documentElement
  if (theme.name === BRAVE) {
    root.removeAttribute('data-theme')
    root.removeAttribute('data-ground')
    for (const name of [...root.style]) {
      if (name.startsWith('--role-')) root.style.removeProperty(name)
    }
    return
  }
  for (const [name, value] of Object.entries(roleVariables(theme, dark()))) {
    root.style.setProperty(name, value)
  }
  root.setAttribute('data-theme', theme.name)
  // Whether the theme paints its own ground, which is the one decision that costs something
  // visible: the window is drawn over a native sidebar blur, and an opaque background covers it.
  // A palette that inherits its background — `"background": "none"`, or the key left out — keeps
  // the blur, so a three-line palette that only changes the accent does not flatten the window.
  if (paintsBackground(theme)) root.setAttribute('data-ground', 'own')
  else root.removeAttribute('data-ground')
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
  return () => query.removeEventListener('change', handler)
}
