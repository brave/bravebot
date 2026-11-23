/**
 * Starting a new chat from a brief the person has read, from the desktop's Chat menu (SESSION-34,
 * CMD-34).
 *
 * The brief is the bridge's (`crates/ui-bridge/src/handoff.rs`), from the request the terminal's
 * `/handoff` makes, and comes back as a `handoff.done` event. What is here is what the window does
 * with the answer, as plain functions so each step can be tested without a window: when the brief
 * goes in the message box, what the box held before it, when the handoff is dropped, and what Send
 * takes from the box. Nothing here reads a word of the brief.
 */

import type { EventMap, SessionSummary } from '../shared/protocol'

/** What the bridge said a request came back with. */
export type HandedOff = EventMap['handoff.done']

/** What a chat holds about a handoff from it. */
export interface Handoff {
  /** A request for a brief toward `goal` is out. */
  asking?: { goal: string } | null
  /** The brief is in the box, and Send starts a new chat from it. `earlier` is what the box held before. */
  armed?: { earlier: string } | null
}

/** What the person is told where no brief was put in the box. */
export type HandoffNotice =
  | { toast: { title: string; body?: string; kind: 'note' } }
  | { problem: string }

/** The request goes out for `goal`. A brief already in the box stays armed until the answer. */
export function asked(held: Handoff, goal: string): Handoff {
  return { ...held, asking: { goal } }
}

/**
 * What the box and the chat hold once the answer is in.
 *
 * `draft` is what the box holds now. A ready brief is put in the box and arms the handoff, and the
 * line that was there is kept to put back. Where the box was already a brief, the line kept is the
 * one from before that brief. Every other ending leaves the box alone (`write` is `null`) and says
 * why. An answer to no request this chat made is ignored.
 */
export function received(held: Handoff, data: HandedOff, draft: string): { held: Handoff; write: string | null; notice: HandoffNotice | null } {
  if (!held.asking) return { held, write: null, notice: null }
  const ended = { ...held, asking: null }
  if (data.status === 'ready' && typeof data.text === 'string') {
    return { held: { asking: null, armed: { earlier: held.armed?.earlier ?? draft } }, write: data.text, notice: null }
  }
  return { held: ended, write: null, notice: unmade(data) }
}

function unmade(data: HandedOff): HandoffNotice {
  switch (data.status) {
    case 'untrusted':
      return {
        toast: {
          title: 'No brief offered',
          body: 'This chat has read something the model is not shown, so a brief written from it is not offered',
          kind: 'note',
        },
      }
    case 'stopped':
      return { toast: { title: 'Handoff stopped', body: 'The message box is as it was', kind: 'note' } }
    default:
      return { problem: `The brief could not be written: ${data.message ?? 'no reason was given'}` }
  }
}

/** Whether Send on a box holding `draft` starts a new chat from it. */
export function armed(held: Handoff | null | undefined, draft: string): boolean {
  return !!held?.armed && draft.trim().length > 0
}

/** The handoff once the box holds `draft`: a box that has been emptied drops it. */
export function wiped(held: Handoff, draft: string): Handoff {
  return held.armed && !draft.trim() ? { ...held, armed: null } : held
}

/**
 * Escape over an armed brief: the handoff is dropped and the line the box held before it is put
 * back. `null` where no brief is armed, so Escape is left to whatever else it does.
 */
export function escaped(held: Handoff): { held: Handoff; write: string } | null {
  return held.armed ? { held: { ...held, armed: null }, write: held.armed.earlier } : null
}

/**
 * The chat a record was handed off from, named by its title where the list has it and by its id
 * where it does not, or `null` for a record that was not handed off.
 */
export function sourceOf(id: string | null | undefined, sessions: SessionSummary[]): { id: string; title: string } | null {
  if (!id) return null
  return { id, title: sessions.find((session) => session.id === id)?.title ?? id }
}

/**
 * Send on an armed brief: the text the new chat's first message is, as the person left it, and the
 * handoff ended. `null` where Send is an ordinary send.
 */
export function taken(held: Handoff | null | undefined, draft: string): { held: Handoff; prompt: string } | null {
  if (!held || !armed(held, draft)) return null
  return { held: { ...held, armed: null }, prompt: draft.trim() }
}
