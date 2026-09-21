/**
 * Where a chosen menu item becomes something the window does.
 *
 * The main process knows which item was picked and nothing else — every piece of state a
 * command touches lives here — so this is the whole of the translation, and it is one
 * `switch` rather than a table of closures registered from six places.
 *
 * ## What this module cannot reach, on purpose
 *
 * [`CommandActions`] has no `answer`, no `answerQuestions` and no `answerTrust`. Those three
 * callbacks exist in `App.tsx` and are handed only to the transcript and the trust prompt.
 * They are not omitted here as an oversight to be tidied up later: they are the four
 * approvals and the trust question, and the argument for keeping them out of a menu is that
 * an approval is a claim that somebody looked at the evidence. A menu item is available
 * while the diff is scrolled off screen, and an accelerator can be typed from muscle memory
 * into a window whose contents changed a frame ago.
 *
 * So the enforcement is the scope, not a comment. Anyone who wants an "Approve write" menu
 * item has to widen this interface and thread a prop through `App`, which is a change a
 * reviewer sees rather than one that happens by accident.
 */

import { useEffect, useRef } from 'react'
import type { Side } from './columns'
import type { WindowState } from '../shared/commands'
import type { ExportFormat } from '../shared/export'

export interface CommandActions {
  /** Open the folder picker, or go straight to a directory the main process named. */
  create: (directory?: string) => void
  closeSession: () => void
  send: () => void
  cancel: () => void
  toggle: (side: Side) => void
  resetColumns: () => void
  about: () => void
  doctor: () => void
  /** Open the session a right-click named. */
  openSession: (id: string) => void
  /** Close the session a right-click named, which may not be the open one. */
  closeNamed: (id: string) => void
  /** Put a named session's project directory on the clipboard. */
  copyProjectPath: (id: string) => void
  /** Put a named transcript entry's text on the clipboard. */
  copyEntry: (id: string) => void
  /**
   * Begin a session holding everything said before a named prompt, with that prompt to edit.
   *
   * Named the same way `copyEntry` is — the id of a row, resolved against the transcript here
   * rather than sent anywhere. What crosses to the agent is where that row falls among the
   * prompts, and what it said.
   */
  forkEntry: (id: string) => void
  /** Write the open session's conversation to a file, in one of three formats. */
  exportSession: (format: ExportFormat) => void
  /** Flip whether an export carries the tool calls as well as the conversation. */
  toggleExportTools: () => void
  /**
   * Open the theme picker.
   *
   * Here rather than among the things this module may not reach, because a theme is not an
   * answer: the names are read off disk and drawn for a person, nothing about them is labelled,
   * and choosing one decides nothing the agent asked. The paragraphs above are about approvals,
   * and this is not one.
   */
  theme: () => void
}

/**
 * Listen for menu items, for as long as the app is up.
 *
 * The actions are read through a ref rather than named as dependencies. They are rebuilt on
 * renders that change the session, and re-subscribing on each of those would drop an event
 * arriving in the gap — the same reason `App` keeps the live session handle in a ref for its
 * agent-event listener.
 */
export function useCommandRouter(actions: CommandActions): void {
  const current = useRef(actions)
  current.current = actions

  useEffect(() => {
    return window.bravebot.onCommand((id, context) => {
      const act = current.current
      switch (id) {
        // Context items carry the thing they were opened on. The reference is an identifier
        // and a kind — the renderer looks the thing up in the state it already has, which is
        // why nothing renderable ever had to cross to the main process and back.
        case 'session.new-here':
          return context && act.create(context.id)
        case 'context.session.open':
          return context && act.openSession(context.id)
        case 'context.session.close':
          return context && act.closeNamed(context.id)
        case 'context.session.copy-path':
          return context && act.copyProjectPath(context.id)
        case 'context.entry.copy':
          return context && act.copyEntry(context.id)
        case 'context.entry.fork':
          return context && act.forkEntry(context.id)
        case 'session.new':
          return act.create()
        case 'session.close':
          return act.closeSession()
        case 'session.export-tools':
          return act.toggleExportTools()
        case 'session.export-text':
          return act.exportSession('txt')
        case 'session.export-markdown':
          return act.exportSession('md')
        case 'session.export-pdf':
          return act.exportSession('pdf')
        case 'turn.send':
          return act.send()
        case 'turn.cancel':
          return act.cancel()
        case 'view.fold-left':
          return act.toggle('left')
        case 'view.fold-right':
          return act.toggle('right')
        case 'view.reset-columns':
          return act.resetColumns()
        case 'view.theme':
          return act.theme()
        case 'app.about':
          return act.about()
        case 'help.doctor':
          return act.doctor()
      }
      // No default. `CommandId` is a closed union, so a new command that nobody handled is
      // a type error here rather than a menu item that silently does nothing.
      const unhandled: never = id
      return unhandled
    })
  }, [])
}

/**
 * Tell the main process what the menu should be offering, when that changes.
 *
 * Derived on every render — it is made of render state — but only sent when it differs from
 * what was last sent. Without that, typing in the composer would put one message per
 * keystroke on a channel whose only reader repaints a menu bar. The comparison is a
 * `JSON.stringify` of six fields, which is cheaper than the message it avoids.
 */
export function usePublishedState(state: WindowState): void {
  const sent = useRef<string | null>(null)
  useEffect(() => {
    const encoded = JSON.stringify(state)
    if (encoded === sent.current) return
    sent.current = encoded
    window.bravebot.publishState(state)
  }, [state])
}
