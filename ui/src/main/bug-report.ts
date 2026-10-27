/**
 * The bug report, the two pages and the log, done in the main process (DIAG-9, UPDATE-12).
 *
 * The window shows the report and may ask for four things: the text again, a file of it, a page
 * opened, and the log shown in the file manager. None of them takes anything the window wrote.
 * A save asks the bridge for the text again and writes that, so a renderer that was talked into
 * composing a file of its own cannot have it written. A page is a name this module turns into a
 * URL it holds. The log is the path the bridge names, checked here to be a log in a `logs`
 * directory, and shown, never opened: opening it would hand a file to whatever the system runs
 * `.log` files with.
 */

import { app, dialog, shell, type BrowserWindow } from 'electron'
import { chmodSync, lstatSync, writeFileSync } from 'node:fs'
import { basename, dirname, isAbsolute, join } from 'node:path'
import type { Composed, SaveOutcome } from '../shared/bug-report'
import { pageUrl } from '../shared/update'

/** Asks the bridge one method. Handed in, so this module holds no bridge of its own. */
export type Ask = (method: string, params: Record<string, unknown>) => Promise<unknown>

/** The name the save sheet suggests, in Documents. */
export const SUGGESTED = 'bravebot-bug-report.txt'

const UNAVAILABLE: Composed = { available: false, text: '', log: null }

/** An answer of the shape `bug-report.compose` gives, or the unavailable one. */
export function parseComposed(value: unknown): Composed {
  if (typeof value !== 'object' || value === null) return UNAVAILABLE
  const { available, text, log } = value as Record<string, unknown>
  if (available !== true || typeof text !== 'string') return UNAVAILABLE
  return { available: true, text, log: typeof log === 'string' && isLogPath(log) ? log : null }
}

/** The report, with this app's own version stated as the interface's. */
export async function compose(ask: Ask): Promise<Composed> {
  try {
    return parseComposed(await ask('bug-report.compose', { interface: app.getVersion() }))
  } catch {
    return UNAVAILABLE
  }
}

/**
 * Write the report where the person chooses, readable by them alone.
 *
 * Takes no text: the report is asked for again here, after the sheet closes, so what is written is
 * what the bridge composed and the newest log it names is the newest at the moment of saving.
 */
export async function save(window: BrowserWindow | null, ask: Ask): Promise<SaveOutcome> {
  if (!window) return { status: 'failed', message: 'There is no window to ask in.' }
  const result = await dialog.showSaveDialog(window, {
    title: 'Save bug report',
    defaultPath: join(app.getPath('documents'), SUGGESTED),
    filters: [{ name: 'Plain Text', extensions: ['txt'] }],
    properties: ['createDirectory'],
  })
  if (result.canceled || !result.filePath) return { status: 'cancelled' }
  const report = await compose(ask)
  if (!report.available) {
    return { status: 'failed', message: 'A bug report is not available in a session that keeps nothing.' }
  }
  try {
    writeFileSync(result.filePath, report.text, { encoding: 'utf8', mode: 0o600 })
    // The mode above applies only to a file this call made. One that was there keeps its own,
    // so it is set again.
    if (process.platform !== 'win32') chmodSync(result.filePath, 0o600)
    return { status: 'saved', where: result.filePath }
  } catch (error) {
    return { status: 'failed', message: `The report could not be saved: ${String(error)}` }
  }
}

/** Open one of the two pages by name. A name that is not one opens nothing. */
export function openPage(name: unknown): boolean {
  const url = pageUrl(name)
  if (!url) return false
  void shell.openExternal(url)
  return true
}

/**
 * Whether `path` is a diagnostic log's: an absolute path to a file named as the log writer names
 * them, directly in a directory called `logs`.
 */
export function isLogPath(path: string): boolean {
  return isAbsolute(path)
    && basename(dirname(path)) === 'logs'
    && /^\d{8}T\d{6}Z-\d+\.log$/.test(basename(path))
}

/** Show the newest log in the file manager, where the bridge names one and it is a plain file. */
export async function showLog(ask: Ask): Promise<boolean> {
  const { log } = await compose(ask)
  if (!log) return false
  try {
    if (!lstatSync(log).isFile()) return false
  } catch {
    return false
  }
  shell.showItemInFolder(log)
  return true
}
