/**
 * The only thing the key window can reach.
 *
 * A preload of its own, so the window a key is typed in has no way to the agent and the main
 * window's page has no way to the key. Read what to show, save the key typed, cancel, say how tall
 * the page is drawn so the window fits it, and hear when what to show changed.
 */

import { contextBridge, ipcRenderer, type IpcRendererEvent } from 'electron'
import type { KeySaveAnswer, KeyWindowState } from '../shared/accounts'

const api = {
  read(): Promise<KeyWindowState | null> {
    return ipcRenderer.invoke('bravebot:key-window:read') as Promise<KeyWindowState | null>
  },
  save(key: string): Promise<KeySaveAnswer> {
    return ipcRenderer.invoke('bravebot:key-window:save', key) as Promise<KeySaveAnswer>
  },
  cancel(): Promise<void> {
    return ipcRenderer.invoke('bravebot:key-window:cancel') as Promise<void>
  },
  fit(height: number): Promise<void> {
    return ipcRenderer.invoke('bravebot:key-window:fit', height) as Promise<void>
  },
  onChanged(listener: (state: KeyWindowState) => void): () => void {
    const handler = (_event: IpcRendererEvent, state: KeyWindowState) => listener(state)
    ipcRenderer.on('bravebot:key-window:changed', handler)
    return () => ipcRenderer.off('bravebot:key-window:changed', handler)
  },
}

contextBridge.exposeInMainWorld('bravebotKey', api)

export type BravebotKeyApi = typeof api
