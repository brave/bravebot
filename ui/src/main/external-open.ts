/**
 * Every address, file and folder the main process hands to the system goes through here.
 *
 * On Linux Electron starts `xdg-open` (or `xdg-mime`) with this process's own environment. Those
 * scripts read the handler that opens an address or a file from the `XDG_` directories, and in
 * their generic path (no desktop environment detected, or the desktop tool missing) run the
 * commands in `BROWSER`. Under a GNOME session with `gio` installed they run `gio open`, which
 * does not read `BROWSER` and executes the wrapper `GIO_LAUNCH_DESKTOP` names with the handler's
 * command line. Under KDE they run `kde-open`. GLib loads modules from `GIO_EXTRA_MODULES` and
 * `GIO_MODULE_DIR`, and Qt loads plug-ins from the `QT_` names and the service cache from
 * `KDESYCOCA`. A program the person approved with `run` may have written the file any of these
 * name, and the opener then runs it outside the sandbox (SANDBOX-30). The same names are checked for the terminal client's opener in
 * `crates/tui/src/opener.rs`, which also judges `HOME`, `XDG_RUNTIME_DIR` and
 * `DBUS_SESSION_BUS_ADDRESS` and removes them from the opener's own start.
 * Those three are not deleted here: this process and the agent it starts read them, and Electron
 * finds the session bus through the last two. Without `HOME`, `gio` reads the account's passwd
 * home, which is the same directory whenever the real home is a place the sandbox let a program
 * write. So under a GNOME session on Linux the opener still reads a `HOME`, a runtime directory
 * or a bus address the sandbox let a program write (SANDBOX-30 gives the reasons and the cost;
 * brave/bravebot#3904 is the issue for `HOME` and brave/bravebot#3940 tracks the other two, which
 * the bullet "The starts that keep XDG_RUNTIME_DIR and DBUS_SESSION_BUS_ADDRESS" under "What a
 * checked start does not cover" in SANDBOX-30 records). `external-open.test.mjs` asserts that all
 * three are left set.
 * The ten Windows names of the `rundll32 url.dll,FileProtocolHandler`
 * start in `opener.rs` (`PATHEXT`, `COMSPEC`, `SystemRoot`, `windir`, `ProgramFiles`,
 * `ProgramW6432`, `ProgramFiles(x86)`, `LOCALAPPDATA`, `APPDATA`, `USERPROFILE`) are not deleted here. The opener
 * removes them from its own start (brave/bravebot#3860). Electron's own Windows start is not
 * covered: it is given those names as this process holds them.
 * `contrib/check-code-selectors.py` holds these exceptions.
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
  'GIO_LAUNCH_DESKTOP',
  'XDG_DATA_HOME',
  'XDG_DATA_DIRS',
  'XDG_CONFIG_HOME',
  'XDG_CONFIG_DIRS',
  'GIO_EXTRA_MODULES',
  'GIO_MODULE_DIR',
  'QT_PLUGIN_PATH',
  'QT_QPA_PLATFORM_PLUGIN_PATH',
  'QT_QPA_PLATFORMTHEME',
  'QT_QPA_GENERIC_PLUGINS',
  'QT_QPA_PLATFORM',
  'QT_STYLE_OVERRIDE',
  'KDESYCOCA',
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

/**
 * Reveal a file in the system's file manager. On Linux Electron asks the file manager over D-Bus
 * and, when that fails, starts `xdg-open` on the folder, so the deletion applies here as it does to
 * `openPath`. `scripts/external-open-electron.test.mjs` records that start.
 */
export function showItemInFolder(path: string): void {
  removeOpenerSelectors()
  shell.showItemInFolder(path)
}
