/**
 * The desktop as a client of `bravebot-host`: one window, drawn from the Rust view.
 *
 * The host started this process with its channel as standard input and output, so nothing here may
 * write to standard output but the channel. The window has a preload of its own and reaches the
 * channel only through the two calls below. It is told the state, and it may ask for the actions
 * `parseAction` accepts, and a call from any page but its own is refused.
 */

import { app, BrowserWindow, ipcMain } from 'electron'
import { join } from 'node:path'
import { parseAction, type PrototypeOutcome } from '../../shared/prototype'
import { rendererUrl } from '../renderer-url'
import { HostClient, type Channel } from './client'
import type { HostClientOptions } from './options'

/** How often the host is asked what it holds, so a session another client opens is listed. */
const REFRESH_MS = 2_000

const channel: Channel = {
  onData(listener) {
    process.stdin.setEncoding('utf8')
    process.stdin.on('data', listener)
  },
  // The host's end going away shows as the input ending or closing, or as a write that fails, and
  // all of them end this client.
  onEnd(listener) {
    process.stdin.on('end', () => listener())
    process.stdin.on('close', () => listener())
    process.stdin.on('error', (error) => listener(`reading from the host failed: ${error.message}`))
    process.stdout.on('error', (error) => listener(`writing to the host failed: ${error.message}`))
  },
  write(line) {
    process.stdout.write(line)
  },
}

export function startHostClient(options: HostClientOptions): void {
  let window: BrowserWindow | null = null
  const client = new HostClient(options, channel, () => {
    if (window && !window.isDestroyed()) window.webContents.send('bravebot:prototype:state', client.state())
  })

  const ours = (event: Electron.IpcMainInvokeEvent): boolean =>
    window !== null && !window.isDestroyed() && event.sender === window.webContents && event.senderFrame === window.webContents.mainFrame

  void app.whenReady().then(async () => {
    ipcMain.handle('bravebot:prototype:state', (event) => (ours(event) ? client.state() : null))
    ipcMain.handle('bravebot:prototype:act', async (event, value: unknown): Promise<PrototypeOutcome> => {
      if (!ours(event)) return { ok: false, code: 'not_allowed', message: 'not this window' }
      const action = parseAction(value)
      if (action === null) return { ok: false, code: 'bad_request', message: 'not an action this window may ask for' }
      return client.act(action)
    })

    window = new BrowserWindow({
      width: 1080,
      height: 760,
      minWidth: 640,
      minHeight: 480,
      show: false,
      title: `Bravebot · ${options.name}`,
      webPreferences: {
        preload: join(__dirname, '../preload/prototype.js'),
        contextIsolation: true,
        nodeIntegration: false,
        sandbox: true,
        webviewTag: false,
      },
    })
    window.once('ready-to-show', () => window?.show())
    window.on('page-title-updated', (event) => event.preventDefault())
    window.webContents.setWindowOpenHandler(() => ({ action: 'deny' }))
    window.webContents.on('will-navigate', (event) => event.preventDefault())
    // Closing the window ends this client and nothing else. The host keeps each session and what it
    // is doing, and the person's other clients go on.
    window.on('closed', () => {
      window = null
      app.quit()
    })
    const devServer = rendererUrl(app)
    if (devServer) void window.loadURL(`${devServer}/prototype.html`)
    else void window.loadFile(join(__dirname, '../renderer/prototype.html'))

    try {
      await client.refresh()
    } catch (error) {
      process.stderr.write(`bravebot: the host did not answer: ${error instanceof Error ? error.message : String(error)}\n`)
    }
    const timer = setInterval(() => {
      if (client.state().channel === 'ended') return clearInterval(timer)
      void client.refresh().catch(() => {})
    }, REFRESH_MS)
    timer.unref()
  })
}
