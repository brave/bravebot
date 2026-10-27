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
 * What the bridge needs to put a line into words for the turn (DROP-8).
 *
 * The words themselves are the bridge's, from the one Rust implementation the terminal shares
 * (SHARE-001): a dropped file becomes its name and a pasted picture a sentence saying it cannot be
 * shown. The window sends the line as it was written and says which marker stands for which.
 */
export function markersOf(staged: Staged[]): { files: { marker: string; name: string }[]; pictures: string[] } {
  return {
    files: staged.flatMap((item) => (item.via === 'drop' ? [{ marker: item.marker, name: item.file.name }] : [])),
    pictures: staged.flatMap((item) => (item.via === 'paste' ? [item.marker] : [])),
  }
}

/**
 * Where Enter sends a line while a turn runs (INPUT-46).
 *
 * A file chosen with the paperclip is a grant the running turn cannot be handed, and the bot's
 * memory update reads nothing from the person: both wait for the turn to end. A line that summons
 * a bot starts a turn addressed to that bot, so it waits too (MEMORY-13).
 */
export function route(live: Pick<Live, 'consolidating' | 'attachments'>, staged: Staged[] = []): 'interject' | 'queue' {
  return live.consolidating || live.attachments?.length || staged.some((item) => item.via === 'summon') ? 'queue' : 'interject'
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
