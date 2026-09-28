/**
 * Putting System / Light / Dark on the window.
 *
 * Leo (Nala) picks light or dark from `prefers-color-scheme` or the nearest ancestor with
 * `data-theme="light|dark"`. System clears the attribute so the media query wins; Light and
 * Dark set it so the window stays put regardless of the OS.
 *
 * Plain DOM rather than React: applying an appearance must not rebuild the transcript.
 *
 * ## The one rule about the offscreen window
 *
 * Nothing in this module may be called from `export.tsx`. The PDF is pinned light by
 * `data-theme="light"` on `export.html`, which is a separate entry point.
 */

import { SYSTEM, type Appearance } from '../shared/theme'

/** Paint the window in an appearance. */
export function applyAppearance(appearance: Appearance): void {
  const root = document.documentElement
  if (appearance === SYSTEM) {
    root.removeAttribute('data-theme')
    return
  }
  root.setAttribute('data-theme', appearance)
}
