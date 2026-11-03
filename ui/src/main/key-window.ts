/**
 * The small window a Brave Premium or gateway key is typed in (PREM-16, CRED-24).
 *
 * A fixed local page with a preload of its own that reaches four channels: read what to show,
 * save the key typed, cancel, and say how tall it is drawn, and is told when what to show changed.
 * It loads no session and cannot reach the bridge; the key it saves goes to this process, which
 * sends it on. Closing it cancels.
 */

import { BrowserWindow, ipcMain } from 'electron'
import { smallWindow } from './small-window'
import type { KeyEntry } from './key-entry'
import type { KeyWindowState } from '../shared/accounts'

const KEY_WINDOW_WIDTH = 460
const KEY_WINDOW_HEIGHTS = { least: 220, most: 720 }

const keyWindow = smallWindow('key-window', { spellcheck: false })

export function showKeyWindow(state: KeyWindowState, parent: BrowserWindow | null, cancel: () => void): void {
  const title = `Enter the key for ${state.name}`
  const notices = Number(state.replace) + Number(state.variable !== null)
  keyWindow.show({
    title,
    width: KEY_WINDOW_WIDTH,
    // A first guess; the page says its height once drawn, and again when a refusal is shown.
    height: 320 + notices * 80,
    parent,
    cancel,
    reuse: (window) => {
      window.setTitle(title)
      window.webContents.send('bravebot:key-window:changed', state)
    },
  })
}

export const closeKeyWindow = keyWindow.close

/** The key window's own channels. Each answers only that window's page. */
export function handleKeyWindow(entry: KeyEntry<BrowserWindow>): void {
  const { ours } = keyWindow
  ipcMain.handle('bravebot:key-window:read', (event) => (ours(event) ? entry.windowState() : null))
  ipcMain.handle('bravebot:key-window:save', (event, key: unknown) => (ours(event) ? entry.save(key) : { refused: 'The key window is not waiting for a key.' }))
  ipcMain.handle('bravebot:key-window:cancel', (event) => { if (ours(event)) entry.cancel() })
  ipcMain.handle('bravebot:key-window:fit', (event, height: unknown) => {
    if (!ours(event) || typeof height !== 'number' || !Number.isFinite(height)) return
    keyWindow.current()?.setContentSize(KEY_WINDOW_WIDTH, Math.min(KEY_WINDOW_HEIGHTS.most, Math.max(KEY_WINDOW_HEIGHTS.least, Math.ceil(height))))
  })
}
