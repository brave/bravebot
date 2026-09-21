/**
 * Everything this app remembers between launches, in one shape, and the one judgement about
 * whether a file on disk is that.
 *
 * Until now each remembered thing had a file of its own — `layout.json`, `view.json`,
 * `recents.json`, `forks.json` — on the principle each of those validators states: one judgement
 * per shape, so a hand-edited grouping flag cannot cost somebody their column widths. That
 * principle survives here intact, and this file is the reason it can: `parseState` does not judge
 * anything itself. It calls the four validators that already existed, plus `parsePanels` below and
 * `parseChosenTheme` next door, and each key is theirs alone. What changed is where the bytes
 * live, not who decides what they mean — one file per shape has become one file with a key per
 * shape, and a bad value in any key still costs that key and nothing beside it.
 *
 * What that buys: somewhere to look. A person wondering what this app kept about them had four
 * files to find and no way to know there were not five.
 */

import { parseLayout, type StoredLayout } from './layout'
import { parseView, type StoredView } from './view'
import { parseRecents } from './recents'
import { parseForks, type Fork } from './forks'
import { parseBots, type Bot } from './bots'
import { parseChosenTheme } from './theme'

/** The panels in the context column, in the order they appear there. */
export const PANEL_NAMES = ['plan', 'read', 'writes', 'confined', 'files'] as const

export type PanelName = (typeof PANEL_NAMES)[number]

export interface StoredPanels {
  /**
   * The panels turned off, by name.
   *
   * The ones that are off rather than the ones that are on, for the reason `StoredView.collapsed`
   * gives about its own list: a panel added to this window after somebody last set their
   * preference should arrive *visible*, which is the column's default, rather than hidden behind a
   * file written before it existed.
   */
  off: PanelName[]
}

export interface StoredState {
  /** The column widths and folds, or `null` for a window that has never been arranged. */
  layout: StoredLayout | null
  /** How the session list is arranged. */
  view: StoredView
  /** Which panels the context column is showing. */
  panels: StoredPanels
  /** The projects opened before, newest first. Written by the main process alone. */
  recents: string[]
  /** Which session came out of which, newest first. Written by the main process alone. */
  forks: Fork[]
  /** The bots somebody has defined, by slug. Half written here, half by the main process. */
  bots: Bot[]
  /** Which palette the window is painted in, by name. `brave` is the app's own. */
  theme: string
}

function isPanelName(value: unknown): value is PanelName {
  return typeof value === 'string' && (PANEL_NAMES as readonly string[]).includes(value)
}

/**
 * Which panels are off.
 *
 * Names this build does not have are dropped rather than the preference refused — the same
 * treatment `StoredView` gives one bad path among good ones. A panel renamed or removed in a later
 * build would otherwise take somebody's whole arrangement with it. Nothing is coerced, and
 * duplicates collapse: a panel is off or it is not, and saying so twice means nothing.
 */
export function parsePanels(value: unknown): StoredPanels {
  if (typeof value !== 'object' || value === null) return { off: [] }
  const { off } = value as { off?: unknown }
  if (!Array.isArray(off)) return { off: [] }
  return { off: [...new Set(off.filter(isPanelName))] }
}

/**
 * The remembered state, always.
 *
 * Never null, for the reason `parseView` gives: no file, a truncated file and a file with nothing
 * usable in it all describe the same window, and telling them apart would hand the caller a
 * decision it does not have to make. Each key is delegated whole to the validator that owns that
 * shape, and `layout` keeps its own nullability because "never arranged" is a real state there.
 */
export function parseState(value: unknown): StoredState {
  const held = typeof value === 'object' && value !== null ? (value as Record<string, unknown>) : {}
  return {
    layout: parseLayout(held.layout),
    view: parseView(held.view),
    panels: parsePanels(held.panels),
    // The two lists are stored as plain arrays, which is what a person opening this file would
    // expect to read, and handed to their validators in the shape those already judge.
    recents: parseRecents({ directories: held.recents }).directories,
    forks: parseForks({ forks: held.forks }).forks,
    bots: parseBots({ bots: held.bots }).bots,
    // Judged by `parseChosenTheme`, which lives beside the palette format it names one of rather
    // than here — the same arrangement the four above have, where the validator sits with the
    // shape it understands.
    theme: parseChosenTheme(held.theme),
  }
}
