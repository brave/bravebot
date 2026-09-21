/**
 * Palettes somebody wrote, and the directory they go in.
 *
 * Which theme is *chosen* is not here — that is a key in `bravebot-ui.json` like every other
 * preference this window keeps, and `src/main/state.ts` owns it. This file is only the other half:
 * the twenty-two palettes compiled in, plus any JSON dropped into `themes/` beside that file.
 *
 * A directory of this app's own rather than the agent's. The format is a port of the agent's, and
 * a palette written for one will be recognised by the other, but nothing here reads or writes
 * anything under `~/.bravebot`: this app drives `bravebot-rpc` as a subprocess and is not
 * otherwise installed alongside it, so a window that could not paint itself until the terminal had
 * been run once — or that lost somebody's colours when it was uninstalled — would be depending on
 * something it was never promised.
 *
 * Everything here is best-effort. No `themes` directory is the ordinary state of a machine where
 * nobody has written a palette, and it means the built-in list, which is already twenty-two long.
 */

import { mkdirSync, readFileSync, readdirSync, watch, type FSWatcher } from 'node:fs'
import { join } from 'node:path'
import { app } from 'electron'
import { BRAVE, BUILTINS, parseUserTheme, sortBraveFirst, type Theme } from '../shared/theme'

/** Where a palette somebody wrote goes: `themes/`, beside `bravebot-ui.json` under `userData`. */
export function themesDirectory(): string {
  return join(app.getPath('userData'), 'themes')
}

/**
 * Every theme on offer: the compiled-in set, with anything in `themes/` merged over it.
 *
 * A file that takes the name of a built-in replaces it, which is how somebody adjusts a scheme
 * they otherwise like without having to invent a name for their version of it. `brave.json` is
 * ignored: `brave` means "leave this window as the system has it", and a file that redefined it
 * would leave no way back.
 */
export function readThemes(): Theme[] {
  const themes = [...BUILTINS]
  for (const theme of readUserThemes()) {
    const at = themes.findIndex((existing) => existing.name === theme.name)
    if (at === -1) themes.push(theme)
    else themes[at] = theme
  }
  return sortBraveFirst(themes)
}

function readUserThemes(): Theme[] {
  let entries: string[]
  try {
    entries = readdirSync(themesDirectory())
  } catch {
    return []
  }
  const themes: Theme[] = []
  for (const entry of entries) {
    if (!entry.endsWith('.json')) continue
    const name = entry.slice(0, -'.json'.length)
    if (name.length === 0 || name === BRAVE) continue
    let contents: string
    try {
      contents = readFileSync(join(themesDirectory(), entry), 'utf8')
    } catch {
      continue
    }
    const theme = parseUserTheme(name, contents)
    if (theme) themes.push(theme)
  }
  return themes
}

/**
 * Call back when the palettes on disk change.
 *
 * Writing a palette is an editing loop — save the file, look at the window, adjust a colour — and
 * without this every turn of it would mean quitting the app. Debounced because a save arrives as
 * more than one event, and best-effort because watching a directory that does not exist is not an
 * error worth reporting: it is a machine on which nobody has written a palette.
 */
export function watchThemes(onChange: () => void): () => void {
  const watchers: FSWatcher[] = []
  let pending: NodeJS.Timeout | null = null
  const fire = (): void => {
    if (pending) clearTimeout(pending)
    pending = setTimeout(onChange, 120)
  }
  try {
    // Made before it is watched, and made at all: `watch` cannot attach to a directory that does
    // not exist, so without this the first palette somebody ever writes would be the one edit the
    // window did not follow. It is also the path the picker prints under "add your own", and an
    // instruction naming a folder that is not there is a worse instruction.
    mkdirSync(themesDirectory(), { recursive: true })
    watchers.push(watch(themesDirectory(), fire))
  } catch {
    // A directory that cannot be made is not worth an error on screen; it means the built-in list.
  }
  return () => {
    if (pending) clearTimeout(pending)
    for (const watcher of watchers) watcher.close()
  }
}
