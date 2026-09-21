/**
 * The only thing the print window can reach.
 *
 * A second preload rather than a wider first one. The window that lays out a PDF has no
 * business being able to send a turn, answer a question, move the columns or pop a menu —
 * and the window somebody is typing in has no business being able to announce that a print
 * finished. Two narrow surfaces state that; one shared surface with more on it would only
 * imply it.
 *
 * So this is the whole of it: receive one document, say when it has been drawn. There is no
 * reply channel carrying content back, because nothing in that window produces any — it
 * renders what it was given and the main process photographs the result.
 */

import { contextBridge, ipcRenderer, type IpcRendererEvent } from 'electron'
import type { ExportDocument } from '../shared/export'

const api = {
  /**
   * The document to draw, once.
   *
   * `once` rather than `on`: this window is built for a single print and destroyed after
   * it, so a second document would mean something had gone wrong upstream rather than that
   * the page should redraw.
   */
  onDocument(listener: (document: ExportDocument, at: number) => void): void {
    ipcRenderer.once(
      'bravebot:export:document',
      (_event: IpcRendererEvent, document: ExportDocument, at: number) => listener(document, at),
    )
  },

  /**
   * Say the page has finished drawing.
   *
   * The main process waits for this rather than for a timeout. Only the page knows when its
   * Markdown has committed and its fonts have loaded, and a printed file that caught the
   * document half-laid-out is a corrupt artefact somebody keeps.
   */
  ready(): void {
    ipcRenderer.send('bravebot:export:ready')
  },
}

contextBridge.exposeInMainWorld('bravebotExport', api)

export type BravebotExportApi = typeof api
