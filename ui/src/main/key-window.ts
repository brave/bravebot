/**
 * The small window a Brave Premium or gateway key is typed in (PREM-16, CRED-24).
 *
 * A fixed local page with a preload of its own that reaches four channels: read what to show,
 * save the key typed, cancel, and say how tall it is drawn, and is told when what to show changed.
 * It loads no session and cannot reach the bridge; the key it saves goes to this process, which
 * sends it on. Closing it cancels.
 */

import { BrowserWindow, ipcMain } from 'electron'
import { join } from 'node:path'
import type { KeyEntry } from './key-entry'
import type { KeyWindowState } from '../shared/accounts'

const KEY_WINDOW_WIDTH = 460
const KEY_WINDOW_HEIGHTS = { least: 220, most: 720 }

let shown: BrowserWindow | null = null
/** Closed by this process because the entry ended, rather than by a person. */
let closing = false

export function showKeyWindow(state: KeyWindowState, parent: BrowserWindow | null, cancel: () => void): void {
  if (shown && !shown.isDestroyed()) {
    shown.setTitle(`Enter the key for ${state.name}`)
    shown.webContents.send('bravebot:key-window:changed', state)
    shown.show()
    shown.focus()
    return
  }
  const notices = Number(state.replace) + Number(state.variable !== null)
  const window = new BrowserWindow({
    width: KEY_WINDOW_WIDTH,
    // A first guess; the page says its height once drawn, and again when a refusal is shown.
    height: 320 + notices * 80,
    useContentSize: true,
    resizable: false,
    minimizable: false,
    maximizable: false,
    fullscreenable: false,
    title: `Enter the key for ${state.name}`,
    show: false,
    ...(parent && !parent.isDestroyed() ? { parent } : {}),
    webPreferences: {
      preload: join(__dirname, '../preload/key-window.js'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      webviewTag: false,
      spellcheck: false,
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
  if (process.env.ELECTRON_RENDERER_URL) void window.loadURL(`${process.env.ELECTRON_RENDERER_URL}/key-window.html`)
  else void window.loadFile(join(__dirname, '../renderer/key-window.html'))
}

export function closeKeyWindow(): void {
  if (!shown || shown.isDestroyed()) return
  closing = true
  shown.close()
}

/** The key window's own channels. Each answers only that window's page. */
export function handleKeyWindow(entry: KeyEntry<BrowserWindow>): void {
  const ours = (event: Electron.IpcMainInvokeEvent) => Boolean(shown && !shown.isDestroyed() && event.sender === shown.webContents)
  ipcMain.handle('bravebot:key-window:read', (event) => (ours(event) ? entry.windowState() : null))
  ipcMain.handle('bravebot:key-window:save', (event, key: unknown) => (ours(event) ? entry.save(key) : { refused: 'The key window is not waiting for a key.' }))
  ipcMain.handle('bravebot:key-window:cancel', (event) => { if (ours(event)) entry.cancel() })
  ipcMain.handle('bravebot:key-window:fit', (event, height: unknown) => {
    if (!ours(event) || typeof height !== 'number' || !Number.isFinite(height)) return
    shown?.setContentSize(KEY_WINDOW_WIDTH, Math.min(KEY_WINDOW_HEIGHTS.most, Math.max(KEY_WINDOW_HEIGHTS.least, Math.ceil(height))))
  })
}
