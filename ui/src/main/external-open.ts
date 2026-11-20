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
 * `KDESYCOCA` or, when that is not set, the `XDG_CACHE_HOME` directory. A program the person
 * approved with `run` may have written the file any of these name, and the opener then runs it
 * outside the sandbox (SANDBOX-30). The same names are checked for the terminal client's opener in
 * `crates/tui/src/opener.rs`, which also judges `HOME`, `XDG_RUNTIME_DIR` and
 * `DBUS_SESSION_BUS_ADDRESS` and removes them from the opener's own start.
 * `HOME` is not deleted here: this process and the agent it starts read it, and without `HOME`,
 * `gio` reads the account's passwd home, which is the same directory whenever the real home is a
 * place the sandbox let a program write (SANDBOX-30 gives the reasons and the cost;
 * brave/bravebot#3904 is the issue for `HOME`).
 *
 * `XDG_RUNTIME_DIR` and `DBUS_SESSION_BUS_ADDRESS` are where `gio open` finds the session bus, and
 * a service a stage runs on a socket it can create answers as the bus. Electron also finds the
 * session bus through them, and the agent this process starts puts its proxy socket under
 * `XDG_RUNTIME_DIR`, so they are not deleted whatever they hold. The directories a session may
 * write are recorded in the agent's process and not in this one, so before each call this asks the
 * agent (`opener.judge`) which of the two it holds lead to a socket a stage may write, and deletes
 * those. The agent applies the check of the terminal client's opener to them. When the agent cannot
 * be asked, or does not answer within `JUDGE_TIMEOUT_MS`, both are deleted, since nothing says they
 * are safe.
 * The eleven Windows names of the `rundll32 url.dll,FileProtocolHandler`
 * start in `opener.rs` (`PATHEXT`, `COMSPEC`, `SystemRoot`, `windir`, `ProgramFiles`,
 * `ProgramW6432`, `ProgramFiles(x86)`, `LOCALAPPDATA`, `APPDATA`, `USERPROFILE`, `PSModulePath`) are not deleted here. The opener
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

/**
 * The two variables `gio open` finds the session bus from. They are judged by the agent and
 * deleted only when they lead to a socket a stage may write. Keep in step with `NAMES` in
 * `crates/ui-bridge/src/opener_names.rs`.
 */
export const SESSION_BUS_NAMES = ['XDG_RUNTIME_DIR', 'DBUS_SESSION_BUS_ADDRESS'] as const

/** Ask the agent a question by bridge method, as `Ask` in `bug-report.ts` does. */
export type Judge = (method: string, params: Record<string, unknown>) => Promise<unknown>

/**
 * The longest a call waits for the agent's answer, in milliseconds. A request to the agent has no
 * timeout of its own and each call here waits for the one before it, so a request that never
 * answers would otherwise stop every link and every show-in-folder.
 */
export const JUDGE_TIMEOUT_MS = 5000

let judge: Judge | null = null
let judgeTimeoutMs = JUDGE_TIMEOUT_MS

/**
 * Give this module the way to reach the agent, and how long to wait for it. Until it is given one,
 * the two names are deleted.
 */
export function useOpenerJudge(next: Judge | null, timeoutMs: number = JUDGE_TIMEOUT_MS): void {
  judge = next
  judgeTimeoutMs = timeoutMs
}

/** Reject when `ms` pass, so a wait on the agent is bounded. */
function timeout(ms: number): { expired: Promise<never>; cancel: () => void } {
  let timer: ReturnType<typeof setTimeout> | undefined
  const expired = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error('the agent did not answer in time')), ms)
  })
  return { expired, cancel: () => clearTimeout(timer) }
}

/**
 * Which of the session bus names `env` holds are to be deleted, from the agent's answer.
 *
 * A name the agent does not list is kept. Where the agent cannot be asked, does not answer within
 * `JUDGE_TIMEOUT_MS`, or its answer is not a list of the two names, every name held is deleted,
 * since nothing then says it is safe.
 */
async function judged(env: NodeJS.ProcessEnv): Promise<string[]> {
  const held: Record<string, string> = {}
  for (const name of SESSION_BUS_NAMES) {
    const value = env[name]
    if (typeof value === 'string') held[name] = value
  }
  if (Object.keys(held).length === 0) return []
  const wait = timeout(judgeTimeoutMs)
  try {
    if (!judge) throw new Error('no agent to ask')
    const answer = (await Promise.race([judge('opener.judge', held), wait.expired])) as { remove?: unknown } | null
    const remove = answer?.remove
    if (!Array.isArray(remove) || !remove.every((name) => typeof name === 'string' && (SESSION_BUS_NAMES as readonly string[]).includes(name))) {
      throw new Error('unreadable answer')
    }
    return remove as string[]
  } catch {
    return Object.keys(held)
  } finally {
    wait.cancel()
  }
}

/**
 * Delete each opener selector, and each session bus name the agent says leads to a socket a stage
 * may write, from `env`. The judgment is asked first and the deletion follows it with nothing
 * awaited in between. The caller awaits this function before it calls Electron, so the names are
 * deleted when Electron is called.
 */
async function removeForOpening(env: NodeJS.ProcessEnv = process.env): Promise<void> {
  const unsafe = await judged(env)
  removeOpenerSelectors(env)
  for (const name of unsafe) delete env[name]
}

/** The variables the opener scripts read to choose a program. Keep in step with `CODE_SELECTORS` in `crates/tui/src/opener.rs`. */
export const OPENER_SELECTORS = [
  'BROWSER',
  'GIO_LAUNCH_DESKTOP',
  'XDG_DATA_HOME',
  'XDG_DATA_DIRS',
  'XDG_CONFIG_HOME',
  'XDG_CONFIG_DIRS',
  'XDG_CACHE_HOME',
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

/**
 * Variables `xdg-mime` joins to a file name without checking it (`<desktop>-mimeapps.list`,
 * `<prefix>defaults.list`, `<prefix>mimeinfo.cache`), so a value with a `/` in it reads a file
 * outside the directories above. They are deleted only then, as in `FILE_NAME_PREFIXES` in
 * `crates/tui/src/opener.rs`: `XDG_CURRENT_DESKTOP` also tells `xdg-open` which desktop it is on,
 * and a value without a `/` stays inside the directory it is joined to.
 */
export const FILE_NAME_PREFIXES = ['XDG_CURRENT_DESKTOP', 'XDG_MENU_PREFIX'] as const

/**
 * Delete each opener selector from `env`, and each file-name prefix whose value contains a `/`, and
 * return the names that were set.
 */
export function removeOpenerSelectors(env: NodeJS.ProcessEnv = process.env): string[] {
  const removed: string[] = []
  for (const name of OPENER_SELECTORS) {
    if (Object.prototype.hasOwnProperty.call(env, name)) {
      delete env[name]
      removed.push(name)
    }
  }
  for (const name of FILE_NAME_PREFIXES) {
    if (env[name]?.includes('/')) {
      delete env[name]
      removed.push(name)
    }
  }
  return removed
}

/** Open an address in the system's browser. */
export async function openExternal(url: string): Promise<void> {
  await removeForOpening()
  return shell.openExternal(url)
}

/** Hand a file to the app the system assigns its type. Resolves to an empty string when it worked. */
export async function openPath(path: string): Promise<string> {
  await removeForOpening()
  return shell.openPath(path)
}

/**
 * Reveal a file in the system's file manager. On Linux Electron asks the file manager over D-Bus
 * and, when that fails, starts `xdg-open` on the folder, so the deletion applies here as it does to
 * `openPath`. `scripts/external-open-electron.test.mjs` records that start.
 */
export async function showItemInFolder(path: string): Promise<void> {
  await removeForOpening()
  shell.showItemInFolder(path)
}
