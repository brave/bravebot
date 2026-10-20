import { useCallback, useEffect, useState } from 'react'
import { applyAppearance } from './theme'
import { SYSTEM, parseAppearance, type Appearance } from '../shared/theme'

/**
 * Appearance (System / Light / Dark) and whether the window is the active one.
 *
 * Remembered in `bravebot-ui.json` like the columns; arrives asynchronously, so it cannot
 * seed `useState`.
 */
export function useAppearance() {
  const [chosen, setChosen] = useState<Appearance>(SYSTEM)

  /**
   * Paint the window from the remembered appearance at startup, and follow writes
   * that arrive through the main process (another window, a driver, the picker).
   */
  useEffect(() => {
    const take = (state: { chosen: Appearance }): void => {
      const appearance = parseAppearance(state.chosen)
      setChosen(appearance)
      applyAppearance(appearance)
    }
    void window.bravebot
      .readTheme()
      .then(take)
      .catch(() => undefined)
    return window.bravebot.onThemeChanged(take)
  }, [])

  useEffect(() => window.bravebot.onWindowActive((active) => {
    if (active) delete document.documentElement.dataset.windowInactive
    else document.documentElement.dataset.windowInactive = ''
  }), [])

  const writeAppearance = useCallback((appearance: Appearance) => {
    applyAppearance(appearance)
    window.bravebot.writeTheme(appearance)
    setChosen(appearance)
  }, [])

  return { chosen, writeAppearance }
}
