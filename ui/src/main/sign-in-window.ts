/**
 * The small window that shows what a sign-in needs a person to act on (CHATGPT-16, XAI-21,
 * BACKEND-70).
 *
 * A fixed local page with a preload of its own that reaches five channels: read what to show,
 * copy one of its fields, open the browser at the recorded address, cancel, and say how tall it is
 * drawn, and is told when what to show has grown. It loads no session and cannot reach the
 * bridge. Closing it cancels the sign-in.
 */

import { BrowserWindow, clipboard, ipcMain } from 'electron'
import { smallWindow } from './small-window'
import type { SignIn } from './sign-in'
import type { SignInWindowState } from '../shared/accounts'

const signInWindow = smallWindow('sign-in')

const SIGN_IN_WINDOW_WIDTH = 460
const SIGN_IN_WINDOW_HEIGHTS = { least: 220, most: 720 }

/** Each way's first guess at its height; the page says its height once drawn, and again when it grows. */
const HEIGHTS = { chatgpt: 380, xai: 400, bedrock: 400 } as const

export function showSignInWindow(state: SignInWindowState, parent: BrowserWindow | null, cancel: () => void): void {
  const title = `Sign in to ${state.name}`
  signInWindow.show({
    title,
    width: SIGN_IN_WINDOW_WIDTH,
    height: HEIGHTS[state.way],
    parent,
    cancel,
    reuse: (window) => {
      window.setTitle(title)
      window.webContents.send('bravebot:sign-in-window:changed', state)
    },
  })
}

/** Tell the window's page what to show now. Only what the main process recorded is sent. */
export function updateSignInWindow(state: SignInWindowState): void {
  signInWindow.current()?.webContents.send('bravebot:sign-in-window:changed', state)
}

export const closeSignInWindow = signInWindow.close

/** The sign-in window's own channels. Each answers only that window's page. */
export function handleSignInWindow(signIn: SignIn<BrowserWindow>): void {
  const { ours } = signInWindow
  ipcMain.handle('bravebot:sign-in-window:read', (event) => (ours(event) ? signIn.windowState() : null))
  ipcMain.handle('bravebot:sign-in-window:copy', (event, what: unknown) => {
    const text = ours(event) ? signIn.copyable(what) : null
    if (!text) return false
    clipboard.writeText(text)
    return true
  })
  ipcMain.handle('bravebot:sign-in-window:open', (event) => (ours(event) ? signIn.open().catch(() => false) : false))
  ipcMain.handle('bravebot:sign-in-window:cancel', (event) => (ours(event) ? signIn.cancelShown() : undefined))
  ipcMain.handle('bravebot:sign-in-window:fit', (event, height: unknown) => {
    if (!ours(event) || typeof height !== 'number' || !Number.isFinite(height)) return
    signInWindow.current()?.setContentSize(SIGN_IN_WINDOW_WIDTH, Math.min(SIGN_IN_WINDOW_HEIGHTS.most, Math.max(SIGN_IN_WINDOW_HEIGHTS.least, Math.ceil(height))))
  })
}
