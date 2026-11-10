/**
 * Every address, file and folder the main process hands to the system goes through here.
 *
 * On Linux Electron starts `xdg-open` (or `xdg-mime`) with this process's own environment. Those
 * scripts run the commands in `BROWSER` before they look at a desktop environment, and read the
 * handler that opens an address or a file from the `XDG_` directories. A program the person
 * approved with `run` may have written the file any of these name, and the opener then runs it
 * outside the sandbox (SANDBOX-30). The same names are checked for the terminal client's opener in
 * `crates/tui/src/opener.rs`, which also judges `HOME` and removes it from the opener's own start.
 * `HOME` is not deleted here: this process and the agent it starts read it, so under a GNOME
 * session on Linux the opener still reads a `HOME` the sandbox let a program write.
 *
 * The names are deleted from `process.env`, which Node writes through to the process's real
 * environment, immediately before each call, so the first call and every later one is covered
 * wherever in start-up it happens. A value the person set is deleted whether or not it is safe: the
 * system then uses its default handlers. `bridge.ts` copies `process.env` for the agent, so it is
 * the environment after the deletion the agent starts with. The agent applies its own check to
 * what it starts.
 */

import { shell } from 'electron'

/** The variables the opener scripts read to choose a program. Keep in step with `CODE_SELECTORS` in `crates/tui/src/opener.rs`. */
export const OPENER_SELECTORS = [
  'BROWSER',
  'XDG_DATA_HOME',
  'XDG_DATA_DIRS',
  'XDG_CONFIG_HOME',
  'XDG_CONFIG_DIRS',
] as const

/** Delete each opener selector from `env`, and return the names that were set. */
export function removeOpenerSelectors(env: NodeJS.ProcessEnv = process.env): string[] {
  const removed: string[] = []
  for (const name of OPENER_SELECTORS) {
    if (Object.prototype.hasOwnProperty.call(env, name)) {
      delete env[name]
      removed.push(name)
    }
  }
  return removed
}

/** Open an address in the system's browser. */
export function openExternal(url: string): Promise<void> {
  removeOpenerSelectors()
  return shell.openExternal(url)
}

/** Hand a file to the app the system assigns its type. Resolves to an empty string when it worked. */
export function openPath(path: string): Promise<string> {
  removeOpenerSelectors()
  return shell.openPath(path)
}

/** Reveal a file in the system's file manager. */
export function showItemInFolder(path: string): void {
  removeOpenerSelectors()
  shell.showItemInFolder(path)
}
