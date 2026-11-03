/**
 * A small fixed window of the main process's own: the sign-in window and the key window.
 *
 * It loads one local page with a preload of its own, has no menu, keeps its title, opens nothing
 * and navigates nowhere. Closing it by hand calls `cancel`; closing it from here does not.
 */

import { BrowserWindow } from 'electron'
import { join } from 'node:path'

export interface SmallWindowOpening {
  title: string
  width: number
  height: number
  parent: BrowserWindow | null
  /** A person closed the window. */
  cancel: () => void
  /** The window is already open: tell it what to show before it is brought forward. */
  reuse: (window: BrowserWindow) => void
}

/** The window loading `out/renderer/<page>.html` with `out/preload/<page>.js`. */
export function smallWindow(page: string, preferences: Electron.WebPreferences = {}) {
  let shown: BrowserWindow | null = null
  /** Closed by this process, rather than by a person. */
  let closing = false
  const current = (): BrowserWindow | null => (shown && !shown.isDestroyed() ? shown : null)

  function show({ title, width, height, parent, cancel, reuse }: SmallWindowOpening): void {
    const open = current()
    if (open) {
      reuse(open)
      open.show()
      open.focus()
      return
    }
    const window = new BrowserWindow({
      width,
      height,
      useContentSize: true,
      resizable: false,
      minimizable: false,
      maximizable: false,
      fullscreenable: false,
      title,
      show: false,
      ...(parent && !parent.isDestroyed() ? { parent } : {}),
      webPreferences: {
        ...preferences,
        preload: join(__dirname, `../preload/${page}.js`),
        contextIsolation: true,
        nodeIntegration: false,
        sandbox: true,
        webviewTag: false,
      },
    })
    shown = window
    closing = false
    window.removeMenu()
    window.once('ready-to-show', () => window.show())
    window.on('page-title-updated', (event) => event.preventDefault())
    window.webContents.setWindowOpenHandler(() => ({ action: 'deny' }))
    window.webContents.on('will-navigate', (event) => event.preventDefault())
    window.on('closed', () => {
      if (shown === window) shown = null
      if (!closing) cancel()
      closing = false
    })
    if (process.env.ELECTRON_RENDERER_URL) void window.loadURL(`${process.env.ELECTRON_RENDERER_URL}/${page}.html`)
    else void window.loadFile(join(__dirname, `../renderer/${page}.html`))
  }

  function close(): void {
    const open = current()
    if (!open) return
    closing = true
    open.close()
  }

  /** The event came from this window's page. */
  const ours = (event: Electron.IpcMainInvokeEvent): boolean => {
    const open = current()
    return open !== null && event.sender === open.webContents
  }

  return { show, close, ours, current }
}
