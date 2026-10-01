/**
 * Applying the chosen appearance to the Electron shell.
 *
 * The preference itself lives in `bravebot-ui.json` via `src/main/state.ts`. This file only
 * tells Chromium which scheme to draw native chrome in — scrollbars, menus, vibrancy —
 * so the window and the page agree.
 */

import { nativeTheme } from 'electron'
import type { Appearance } from '../shared/theme'

/** Push the choice into Electron so native chrome follows the page. */
export function applyNativeAppearance(appearance: Appearance): void {
  nativeTheme.themeSource = appearance
}
