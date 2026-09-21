/**
 * The file this app remembers things in.
 *
 * One file, `bravebot-ui.json` under `userData`, with a key per remembered shape. A file rather
 * than `localStorage` for the measured reason the layout has always cited: the renderer is loaded
 * from `file://`, and Chromium discards storage for that origin between launches — writes work for
 * the life of the window and are gone by the next one.
 *
 * The main process owns the whole file, and every update below replaces exactly one key. That is
 * what keeps a promise the app has made since the recents list existed: the renderer can *read*
 * which projects were opened and cannot write to that list. It reaches this file only through the
 * channels for its own two preferences, and neither of those can name another key — not because
 * the renderer is not trusted to, but because a channel that could would be one more thing to
 * check on every future change.
 *
 * What lands on disk is always `parseState`'s output, never the object a caller passed. The same
 * discipline the four files this replaces each followed on their own: a bug here cannot leave a
 * value in the file that outlives the session that caused it.
 */

import { app } from 'electron'
import { readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { parseState, type StoredPanels, type StoredState } from '../shared/state'
import { parseLayout, type StoredLayout } from '../shared/layout'
import { parseView, type StoredView } from '../shared/view'
import { parseRecents } from '../shared/recents'
import { parseForks, type Fork } from '../shared/forks'
import { type Bot } from '../shared/bots'
import { BRAVE } from '../shared/theme'

const file = (): string => join(app.getPath('userData'), 'bravebot-ui.json')

/**
 * Everything remembered, or the defaults for anything that is not.
 *
 * Never throws. A file that cannot be read is a window that has never been arranged, which is a
 * state this app has to handle on every first launch anyway.
 */
export function readState(): StoredState {
  try {
    return parseState(JSON.parse(readFileSync(file(), 'utf8')))
  } catch {
    // No file here yet. There may be four older ones, though, and somebody's columns are not
    // worth losing to a rename.
    return inherited()
  }
}

/**
 * What the four files this replaces held, if they are still there.
 *
 * Read once, on the first launch after the change, and written to the new file by the first update
 * that follows. The old files are left where they are rather than deleted: they cost a few hundred
 * bytes, nothing reads them once this one exists, and a person who steps back to an older build
 * still finds their columns where they left them.
 */
function inherited(): StoredState {
  const legacy = (name: string): unknown => {
    try {
      return JSON.parse(readFileSync(join(app.getPath('userData'), name), 'utf8'))
    } catch {
      return null
    }
  }
  return {
    layout: parseLayout(legacy('layout.json')),
    view: parseView(legacy('view.json')),
    // Panels are new with this file, so there is nothing to inherit — every panel is on.
    panels: { off: [] },
    // As is the theme, and there was never a file for it: an app that has never been themed is
    // one drawing itself in `brave`, which is every window before this feature existed.
    theme: BRAVE,
    recents: parseRecents(legacy('recents.json')).directories,
    forks: parseForks(legacy('forks.json')).forks,
    // Bots are newer than this file, so there is nothing to inherit. Listed all the same rather
    // than left to `parseState` to default, because `update` writes this literal's shape: a key
    // missing here is a key erased from disk by the next write of any other one.
    bots: [],
  }
}

/**
 * Write one key, leaving the rest of the file as it was found.
 *
 * Read-modify-write rather than holding the state in memory, so two windows — or this process and
 * a person with an editor open — cannot silently clobber each other's other keys. Best-effort: a
 * preference that cannot be written down is not worth an error on screen, which is the same
 * judgement each of the files this replaces made about itself.
 */
function update(change: Partial<StoredState>): void {
  const next = parseState({ ...readState(), ...change })
  try {
    writeFileSync(file(), `${JSON.stringify(next, null, 2)}\n`, 'utf8')
  } catch {
    // Nothing to say. The window is already arranged the way the person arranged it; only the
    // memory of it is lost.
  }
}

/** Remember where the columns were. */
export function putLayout(layout: StoredLayout): void {
  update({ layout })
}

/** Remember how the session list is arranged. */
export function putView(view: StoredView): void {
  update({ view })
}

/** Remember which panels the context column is showing. */
export function putPanels(panels: StoredPanels): void {
  update({ panels })
}

/** Remember which palette the window is painted in. */
export function putTheme(theme: string): void {
  update({ theme })
}

/** Remember the projects opened. Main-process only; there is no channel that reaches this. */
export function putRecents(recents: string[]): void {
  update({ recents })
}

/** Remember which session came out of which. Main-process only, for the same reason. */
export function putForks(forks: Fork[]): void {
  update({ forks })
}

/**
 * Remember the bots.
 *
 * Unlike the two above, this one *is* reached from a window — a bot's name and purpose are things
 * somebody types. What the window cannot do is compose the whole record: `bravebot:bots:write`
 * hands over four fields and `src/main/bots.ts` keeps the other two, so the half of a bot that is
 * a report of what the agent did stays as unreachable from the renderer as the recents list is.
 */
export function putBots(bots: Bot[]): void {
  update({ bots })
}
