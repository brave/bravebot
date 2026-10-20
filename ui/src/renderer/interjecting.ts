/**
 * Words typed into a turn that is running, and what becomes of each (INPUT-10).
 *
 * Enter while a turn runs sends the line into that turn, and the planner reads it at the turn's
 * next round boundary. The line is drawn as sent at once, moves into the transcript when the agent
 * says the planner has it, and becomes a turn of its own if the turn ends without reading it. The
 * steps are plain functions of the live session so each can be tested without a window.
 */

import * as t from './transcript'
import type { Interjection, Live, Queued } from './live'
import { sent, type Staged } from './staging'

/**
 * What goes to the turn in place of what the person sees (DROP-8).
 *
 * A turn already running fixed the shape of its context before it read anything, so it cannot be
 * handed a file or a picture. A dropped file becomes its name, which the planner can go and read
 * through the gate any file is read through. A picture has no such recourse and becomes a sentence
 * saying so. The box and the transcript keep the marker, because that is what was on the screen.
 *
 * Not from the message catalog: the planner reads this, so it is part of what the model is given
 * and not something a person is being told.
 */
export function inWords(draft: string, staged: Staged[]): string {
  let words = draft
  for (const item of staged) {
    const stands = item.via === 'drop'
      ? item.file.name
      : `(${item.marker} was pasted here, but it cannot be shown to you: a picture cannot join a turn that is already running. Ask for it again if you need to see it.)`
    words = words.split(item.marker).join(stands)
  }
  return words
}

/** Draw `line` as sent into the turn, and take off the composer what it named. */
export function sentInto(live: Live, line: Interjection): Live {
  return {
    ...live,
    interjected: [...(live.interjected ?? []), line],
    attachments: [],
    staging: sent(live.staging),
  }
}

/**
 * The turn would not take `id`, so it is a turn of its own.
 *
 * It goes first and whether or not the queue is paused: stopping pauses what a person chose to
 * hold back, and this was not held back.
 */
export function declined(live: Live, id: string): Live {
  const line = live.interjected?.find((item) => item.id === id)
  if (!line) return live
  return {
    ...live,
    interjected: live.interjected?.filter((item) => item.id !== id),
    queued: [asQueued(line, true), ...(live.queued ?? [])],
  }
}

/**
 * The planner has read the oldest line sent into the turn, so it is in the conversation now.
 *
 * Lines are read in the order they were sent, so the event names none: the oldest is the one. A
 * line is put in the transcript only here, at the moment the planner is given it.
 */
export function taken(live: Live): Live {
  const [line, ...rest] = live.interjected ?? []
  if (!line) return live
  return { ...live, interjected: rest, entries: [...live.entries, t.userSaid(line.prompt)] }
}

/**
 * The turn is over. Whatever it did not read goes on as turns of their own, ahead of what was
 * queued, and is released from a paused queue when `release` says the person has not been told the
 * turn failed: a turn that was stopped or finished took nothing more, and a turn that failed leaves
 * the line waiting with the queue it paused.
 */
export function leftOver<L extends Live>(live: L, release: boolean): L {
  const unread = live.interjected ?? []
  if (unread.length === 0) return live
  return {
    ...live,
    interjected: [],
    queued: [...unread.map((line) => asQueued(line, release)), ...(live.queued ?? [])],
  }
}

function asQueued(line: Interjection, release: boolean): Queued {
  return { prompt: line.prompt, attachments: line.attachments, staged: line.staged, fromInterjection: release }
}

/** Whether the next queued message may go, given a queue that may be paused. */
export function mayGo(live: Pick<Live, 'queued' | 'queuePaused'>): boolean {
  const next = live.queued?.[0]
  if (!next) return false
  return !live.queuePaused || next.fromInterjection === true
}
