/**
 * The only thing the sign-in window can reach.
 *
 * A third preload, so the window that shows a sign-in's URL or code has no way to the agent, and
 * the main window's page has no way to the URL or code. Read what to show, copy one field of it,
 * open the browser at the address this process recorded, cancel, and say how tall the page is
 * drawn so the window fits it: nothing here takes text or an address from the page. What to show can grow while the window is up, as an AWS sign-in's code
 * arrives after its URL, and the page is told.
 */

import { contextBridge, ipcRenderer } from 'electron'
import type { SignInWindowState } from '../shared/accounts'

const api = {
  read(): Promise<SignInWindowState | null> {
    return ipcRenderer.invoke('bravebot:sign-in-window:read') as Promise<SignInWindowState | null>
  },
  copy(what: 'url' | 'code' | 'address'): Promise<boolean> {
    return ipcRenderer.invoke('bravebot:sign-in-window:copy', what) as Promise<boolean>
  },
  open(): Promise<boolean> {
    return ipcRenderer.invoke('bravebot:sign-in-window:open') as Promise<boolean>
  },
  cancel(): Promise<void> {
    return ipcRenderer.invoke('bravebot:sign-in-window:cancel') as Promise<void>
  },
  fit(height: number): Promise<void> {
    return ipcRenderer.invoke('bravebot:sign-in-window:fit', height) as Promise<void>
  },
  onChanged(listener: (state: SignInWindowState) => void): () => void {
    const handler = (_event: Electron.IpcRendererEvent, state: SignInWindowState) => listener(state)
    ipcRenderer.on('bravebot:sign-in-window:changed', handler)
    return () => ipcRenderer.off('bravebot:sign-in-window:changed', handler)
  },
}

contextBridge.exposeInMainWorld('bravebotSignIn', api)

export type BravebotSignInApi = typeof api
