import { useEffect, useState } from 'react'
import type { TitleState } from '../shared/window-title'
import type { Live } from './live'
import { call } from './rpc'
import { titleState } from './session-list'

/** Asks for the title's text for a session named `name` in `state`. */
export type AskTitle = (name: string, state: TitleState) => Promise<string>

/**
 * A writer of `page`'s title, which asks `ask` for the text of each name and state it is given
 * and writes the answer.
 *
 * A name and state the same as the last one asks nothing, an answer that is the text last written
 * writes nothing, and an answer that arrives after a newer request was made is dropped, so a slow
 * answer cannot put back the title of a session no longer on screen. A failed ask leaves the title
 * as it is.
 *
 * Electron copies the page title onto the main window (see `createWindow()` in
 * `src/main/index.ts`), so this is what names the window.
 */
export function titleWriter(page: { title: string }, ask: AskTitle): (name: string, state: TitleState) => Promise<void> {
  let asked: string | null = null
  let shown: string | null = null
  return async (name, state) => {
    const key = JSON.stringify([name, state])
    if (key === asked) return
    asked = key
    let text: string
    try {
      text = await ask(name, state)
    } catch {
      return
    }
    if (asked !== key || typeof text !== 'string' || text === shown) return
    shown = text
    page.title = text
  }
}

/** The bridge's `window.title`, which builds the text as the terminal's title is built. */
const askBridge: AskTitle = async (name, state) =>
  (await call<{ title: string }>('window.title', { name, state })).title

/**
 * Title the window from the session on screen: its name, with `✦` while a turn runs and `[!]`
 * while an approval or question waits, or the bare `Bravebot` with no session open.
 *
 * The name and state are derived on every render and sent only when they change, so a frame of a
 * streaming turn asks nothing.
 */
export function useWindowTitle(live: Live | null): void {
  const [write] = useState(() => titleWriter(document, askBridge))
  const name = live?.summary.title ?? ''
  const state = titleState(live)
  useEffect(() => {
    void write(name, state)
  }, [write, name, state])
}
