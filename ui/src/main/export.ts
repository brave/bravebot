/**
 * Turning a conversation into a PDF.
 *
 * ## Why there is a whole second window in here
 *
 * The obvious implementation is to build an HTML string and print it. It is rejected, and
 * the reason is the same one `Markdown.tsx` gives for having no `rehype-raw`: a string of
 * markup composed in one process and injected into a document in another is exactly the
 * shape this app refuses everywhere else. Getting such a string onto a page needs
 * `dangerouslySetInnerHTML`, `document.write`, or a `data:` URL — and that function appears
 * nowhere in this codebase and must not start here, least of all on a page laying out text
 * an agent read off somebody's disk.
 *
 * So the PDF is drawn by a real renderer entry point (`src/renderer/export.html`) using the
 * same React components the window uses, `Markdown` included. The content path to the file
 * is the content path to the screen. Nothing composes markup; the parsed document crosses as
 * data and React makes elements out of it, which is the one arrangement where a link in a
 * reply cannot become anything but a link.
 *
 * ## Why this window gets the main window's refusals
 *
 * It is laying out model-authored text containing model-authored links. `setWindowOpenHandler`
 * denies outright here rather than handing the URL to `shell.openExternal` the way the main
 * window does: nothing about rendering a page for printing should be able to open somebody's
 * browser, and there is no user gesture behind anything that happens in here.
 */

import { BrowserWindow, ipcMain } from 'electron'
import { join } from 'node:path'
import type { ExportDocument } from '../shared/export'

/** How long a document gets to draw itself before the export gives up. */
const PATIENCE = 30_000

/**
 * One print at a time.
 *
 * The save sheet is modal to the window, so two exports are close to unreachable from the
 * interface — but the menu bar can be driven while a sheet is up on some macOS versions, and
 * two offscreen windows racing would each be waiting on `bravebot:export:ready` with no way
 * to tell whose page had finished. Serialising is cheaper than making that correct.
 */
let queue: Promise<unknown> = Promise.resolve()

export function printToPdf(document: ExportDocument, at: number): Promise<Buffer> {
  const mine = queue.then(
    () => render(document, at),
    () => render(document, at),
  )
  // The chain must not be broken by a failed print, or one bad export would deadlock every
  // one after it. The queue tracks completion; the caller gets the rejection.
  queue = mine.catch(() => undefined)
  return mine
}

async function render(document: ExportDocument, at: number): Promise<Buffer> {
  const window = new BrowserWindow({
    show: false,
    // Roughly a page's proportions, so what the page measures while laying out is close to
    // what it will be printed at. The paper itself is decided by `@page` in `export.css`.
    width: 850,
    height: 1100,
    webPreferences: {
      preload: join(__dirname, '../preload/export.js'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      webviewTag: false,
    },
  })

  const contents = window.webContents
  contents.setWindowOpenHandler(() => ({ action: 'deny' }))
  contents.on('will-navigate', (event) => event.preventDefault())

  // Scoped to this window's contents. A stray `ready` from anywhere else must not be able to
  // photograph a page that has not finished drawing.
  const drawn = new Promise<void>((resolve, reject) => {
    const timer = setTimeout(() => {
      ipcMain.removeListener('bravebot:export:ready', listener)
      reject(new Error('the export view did not finish drawing in time'))
    }, PATIENCE)
    const listener = (event: Electron.IpcMainEvent): void => {
      if (event.sender !== contents) return
      clearTimeout(timer)
      ipcMain.removeListener('bravebot:export:ready', listener)
      resolve()
    }
    ipcMain.on('bravebot:export:ready', listener)
  })

  try {
    contents.once('did-finish-load', () => {
      contents.send('bravebot:export:document', document, at)
    })

    if (process.env.ELECTRON_RENDERER_URL) {
      await window.loadURL(`${process.env.ELECTRON_RENDERER_URL}/export.html`)
    } else {
      await window.loadFile(join(__dirname, '../renderer/export.html'))
    }

    await drawn

    return await contents.printToPDF({
      // Without this the bubbles print colourless, which is most of what "looks like the
      // app" means.
      printBackground: true,
      // The paper and the styles that fit it live together in `export.css`, rather than the
      // size being decided here and the margins there.
      preferCSSPageSize: true,
      // Reading order and structure in the file, so the result is a document with selectable
      // text rather than a picture of one.
      generateTaggedPDF: true,
    })
  } finally {
    // `destroy` rather than `close`: there is no unload worth being polite about, and a
    // window left behind here is invisible and would never be noticed.
    if (!window.isDestroyed()) window.destroy()
  }
}
