/**
 * One instance of the app per user-data profile.
 *
 * Electron keeps the lock per `userData` directory, so a second launch on the same profile is the
 * same app opened again, and a launch with its own `--user-data-dir` (every driver and test under
 * `scripts/`) is a different app that runs beside it.
 *
 * The second launch's command line, working directory and additional data are not read here or
 * anywhere else. They come from another process, and the only thing the first instance does with
 * the launch is raise its own window, so no listener takes them as parameters.
 */

/** What of Electron's `app` the lock needs. */
export interface SingleInstanceApp {
  requestSingleInstanceLock(): boolean
  quit(): void
  isReady(): boolean
  on(event: 'second-instance', listener: () => void): unknown
}

/** What of Electron's `app` raising a window needs. `show` is macOS only. */
export interface RaisingApp {
  focus(options?: { steal: boolean }): void
  show?(): void
}

/** Only macOS: elsewhere `app.focus` picks the first visible window, which may be a small one. */
const ACTIVATES_THE_APP = 'darwin'

/** What of a `BrowserWindow` raising it needs. */
export interface RaisableWindow {
  isDestroyed(): boolean
  isMinimized(): boolean
  restore(): void
  show(): void
  focus(): void
}

/**
 * Take the lock, or quit. Called before anything that makes a window, reads state or starts the
 * agent, and the caller starts none of them when this answers false.
 *
 * A launch that finds the lock held has already sent itself to the first instance, which calls
 * `activate` when it hears, once the app is ready. Electron documents `second-instance` as emitted
 * after `ready`, and a launch heard earlier is dropped here: the window, the IPC handlers and the
 * window opener do not exist yet, and the first instance builds its own window when it is ready.
 */
export function takeSingleInstance(app: SingleInstanceApp, activate: () => void): boolean {
  if (!app.requestSingleInstanceLock()) {
    app.quit()
    return false
  }
  app.on('second-instance', () => {
    if (app.isReady()) activate()
  })
  return true
}

/**
 * Bring the main window forward for a second launch: restored when minimised, shown when hidden
 * and focused, with the app made active so a window behind another app comes to the front. With no
 * main window, which is the macOS state after the last window closed, `create` builds one the way
 * the dock icon does. Only the main window is raised, never a sign-in, key or print window.
 */
export function raiseMainWindow(
  app: RaisingApp,
  window: RaisableWindow | null,
  create: () => void,
  platform: NodeJS.Platform = process.platform,
): void {
  if (!window || window.isDestroyed()) {
    create()
    return
  }
  if (window.isMinimized()) window.restore()
  window.show()
  if (platform === ACTIVATES_THE_APP) {
    app.show?.()
    app.focus({ steal: true })
  }
  window.focus()
}
