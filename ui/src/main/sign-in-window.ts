/**
 * The small window that shows what a sign-in needs a person to act on (CHATGPT-16, XAI-21,
 * BACKEND-70).
 *
 * A fixed local page with a preload of its own that reaches four channels: read what to show,
 * copy one of its fields, open the browser at the recorded address, and cancel, and is told when
 * what to show has grown. It loads no session and cannot reach the bridge. Closing it cancels the
 * sign-in.
 */

import { BrowserWindow, clipboard, ipcMain } from 'electron'
import { join } from 'node:path'
import type { SignIn } from './sign-in'
import type { SignInWindowState } from '../shared/accounts'

let shown: BrowserWindow | null = null
/** Closed by this process because the sign-in ended, rather than by a person. */
let closing = false

const TITLES = { chatgpt: 'Sign in to ChatGPT', xai: 'Sign in to xAI', bedrock: 'Sign in to AWS' } as const

/** Each way's content height, measured from its page, so the window opens without a scroll bar. */
const HEIGHTS = { chatgpt: 346, xai: 332, bedrock: 358 } as const

export function showSignInWindow(state: SignInWindowState, parent: BrowserWindow | null, cancel: () => void): void {
  if (shown && !shown.isDestroyed()) {
    shown.webContents.send('bravebot:sign-in-window:changed', state)
    shown.show()
    shown.focus()
    return
  }
  const window = new BrowserWindow({
    width: 460,
    height: HEIGHTS[state.way],
    useContentSize: true,
    resizable: false,
    minimizable: false,
    maximizable: false,
    fullscreenable: false,
    title: TITLES[state.way],
    show: false,
    ...(parent && !parent.isDestroyed() ? { parent } : {}),
    webPreferences: {
      preload: join(__dirname, '../preload/sign-in.js'),
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
  if (process.env.ELECTRON_RENDERER_URL) void window.loadURL(`${process.env.ELECTRON_RENDERER_URL}/sign-in.html`)
  else void window.loadFile(join(__dirname, '../renderer/sign-in.html'))
}

/** Tell the window's page what to show now. Only what the main process recorded is sent. */
export function updateSignInWindow(state: SignInWindowState): void {
  if (!shown || shown.isDestroyed()) return
  shown.webContents.send('bravebot:sign-in-window:changed', state)
}

export function closeSignInWindow(): void {
  if (!shown || shown.isDestroyed()) return
  closing = true
  shown.close()
}

/** The sign-in window's own channels. Each answers only that window's page. */
export function handleSignInWindow(signIn: SignIn<BrowserWindow>): void {
  const ours = (event: Electron.IpcMainInvokeEvent) => Boolean(shown && !shown.isDestroyed() && event.sender === shown.webContents)
  ipcMain.handle('bravebot:sign-in-window:read', (event) => (ours(event) ? signIn.windowState() : null))
  ipcMain.handle('bravebot:sign-in-window:copy', (event, what: unknown) => {
    const text = ours(event) ? signIn.copyable(what) : null
    if (!text) return false
    clipboard.writeText(text)
    return true
  })
  ipcMain.handle('bravebot:sign-in-window:open', (event) => (ours(event) ? signIn.open().catch(() => false) : false))
  ipcMain.handle('bravebot:sign-in-window:cancel', (event) => (ours(event) ? signIn.cancelShown() : undefined))
}
