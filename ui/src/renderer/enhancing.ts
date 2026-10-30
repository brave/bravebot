/**
 * Making the message in the box more specific before it is sent (INPUT-50).
 *
 * The terminal does this with Ctrl-P. The rewrite itself is the bridge's, from the one Rust
 * implementation the terminal shares (SHARE-001): it goes to the session's model once, over the
 * conversation as it stands, and comes back as an `enhance.done` event. What is here is what the
 * window does with the answer, as plain functions so each step can be tested without a window:
 * when the box may be rewritten, what the box holds afterwards, and what the person is told where
 * it holds nothing new. Nothing here reads a word of the rewrite.
 */

import type { EventMap } from '../shared/protocol'

/** What the bridge said a request came back with. */
export type Enhanced = EventMap['enhance.done']

/** A request that has gone out for `asked`, the line the box held when it did. */
export interface Asking {
  asked: string
}

/** The line the person wrote, kept while a rewrite stands in its place. */
export interface Rewritten {
  original: string
}

/** What a session holds about the rewrite of its box. */
export interface Enhancement {
  asking?: Asking | null
  rewritten?: Rewritten | null
  note?: string | null
}

/**
 * Why the box cannot be rewritten now, or `null` where it can.
 *
 * A picture, a dropped file or a summoned bot is named in the line by a marker, which a rewrite of
 * words would lose. The bridge cannot see what is staged, so this is the one refusal the window
 * makes; an empty or overlong line and a running turn are the bridge's to refuse, and its sentence
 * is the note.
 */
export function refusal(staged: boolean): string | null {
  return staged ? 'A message with attached files, pictures or a bot picked for it is left as written.' : null
}

/** The request goes out for `draft`. */
export function begun(held: Enhancement, draft: string): Enhancement {
  return { ...held, asking: { asked: draft }, note: null }
}

/**
 * What the box and the session hold once the answer is in.
 *
 * `draft` is what the box holds now. A rewrite is only put where the line it was asked for still
 * is: one that has been typed over since is not written over, because what is in the box is what
 * the person is looking at. `write` is the text to put there, or `null` to leave it alone. Where
 * the box was already a rewrite, the line the person wrote stays the one Escape puts back.
 */
export function settled(held: Enhancement, data: Enhanced, draft: string): { held: Enhancement; write: string | null } {
  const asked = held.asking?.asked ?? null
  const ended = { ...held, asking: null }
  if (data.status === 'ready' && typeof data.text === 'string') {
    if (asked === null || draft !== asked) {
      return { held: { ...ended, note: 'You changed the message while it was being made more specific, so the rewrite was not used.' }, write: null }
    }
    const removed = data.removed ? ` ${data.removed} character${data.removed === 1 ? '' : 's'} that draw as nothing ${data.removed === 1 ? 'was' : 'were'} taken out.` : ''
    return {
      held: {
        asking: null,
        rewritten: { original: held.rewritten?.original ?? asked },
        note: `Made more specific. Edit it, then send, or press Escape to put back what you wrote.${removed}`,
      },
      write: data.text,
    }
  }
  return { held: { ...ended, note: unmade(data) }, write: null }
}

/** What the person is told where no rewrite came back. */
function unmade(data: Enhanced): string {
  switch (data.status) {
    case 'untrusted':
      return 'This conversation has read something untrusted, so a rewrite is not offered. Your message is as you wrote it.'
    case 'unusable':
      return `The rewrite was empty or over ${data.limit} characters, so it was not used. Your message is as you wrote it.`
    case 'stopped':
      return 'Stopped. Your message is as you wrote it.'
    case 'failed':
      return `The rewrite could not be made: ${data.message ?? 'the request failed'}. Your message is as you wrote it.`
    default:
      return 'The rewrite could not be used. Your message is as you wrote it.'
  }
}

/**
 * Put back what the person wrote, over a rewrite still in the box.
 *
 * `null` where the box is not holding one, so Escape does what it did before. A rewrite that has
 * been edited is put back as well: the line the person wrote is the one thing they cannot get
 * again, and Escape is how they ask for it.
 */
export function restored(held: Enhancement): { held: Enhancement; write: string } | null {
  if (!held.rewritten) return null
  return { held: { asking: held.asking ?? null, rewritten: null, note: null }, write: held.rewritten.original }
}

/** The box was emptied or the message sent, so what the person wrote is not waiting to come back. */
export function forgotten(held: Enhancement, draft: string): Enhancement {
  return draft.trim() ? held : { ...held, rewritten: null }
}
