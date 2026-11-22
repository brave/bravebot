import { useEffect, useRef } from 'react'
import { windowTitle } from '../shared/window-title'
import type { Live } from './live'
import { titleState } from './session-list'

/**
 * Write `text` as the page title unless it is the text last written through `last`.
 *
 * Electron copies the page title onto the main window (see `createWindow()` in
 * `src/main/index.ts`), so this is what names the window.
 */
export function writeTitle(page: { title: string }, last: { current: string | null }, text: string): void {
  if (text === last.current) return
  last.current = text
  page.title = text
}

/**
 * Title the window from the session on screen: its name, with `✦` while a turn runs and `[!]`
 * while an approval or question waits, or the bare `Bravebot` with no session open.
 *
 * The text is derived on every render and written only when it changes, as `usePublishedState`
 * does, so a frame of a streaming turn writes nothing.
 */
export function useWindowTitle(live: Live | null): void {
  const written = useRef<string | null>(null)
  const text = windowTitle(live?.summary.title ?? '', titleState(live))
  useEffect(() => {
    writeTitle(document, written, text)
  }, [text])
}
