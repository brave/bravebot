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
import { smallWindow } from './small-window'
import type { SignIn } from './sign-in'
import type { SignInWindowState } from '../shared/accounts'

const signInWindow = smallWindow('sign-in')

const TITLES = { chatgpt: 'Sign in to ChatGPT', xai: 'Sign in to xAI', bedrock: 'Sign in to AWS' } as const

/** Each way's content height, measured from its page, so the window opens without a scroll bar. */
const HEIGHTS = { chatgpt: 346, xai: 332, bedrock: 358 } as const

export function showSignInWindow(state: SignInWindowState, parent: BrowserWindow | null, cancel: () => void): void {
  signInWindow.show({
    title: TITLES[state.way],
    width: 460,
    height: HEIGHTS[state.way],
    parent,
    cancel,
    reuse: (window) => window.webContents.send('bravebot:sign-in-window:changed', state),
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
}
