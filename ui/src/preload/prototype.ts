/**
 * The prototype window's only way to the main process: the state it draws, and the actions it may
 * ask for (HOSTCLIENT-1). No other channel, no filesystem, no child process.
 */

import { contextBridge, ipcRenderer, type IpcRendererEvent } from 'electron'
import type { PrototypeAction, PrototypeOutcome, PrototypeState } from '../shared/prototype'

const STATE = 'bravebot:prototype:state'

contextBridge.exposeInMainWorld('prototypeHost', {
  /** The state now. */
  state: (): Promise<PrototypeState | null> => ipcRenderer.invoke(STATE),
  /** Told the state again each time it changes. Returns the way to stop. */
  subscribe: (listener: (state: PrototypeState) => void): (() => void) => {
    const handler = (_event: IpcRendererEvent, state: PrototypeState): void => listener(state)
    ipcRenderer.on(STATE, handler)
    return () => ipcRenderer.removeListener(STATE, handler)
  },
  /** Ask for an action. The main process says no to anything it does not accept. */
  act: (action: PrototypeAction): Promise<PrototypeOutcome> => ipcRenderer.invoke('bravebot:prototype:act', action),
})
