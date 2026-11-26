import { useCallback, useEffect, type MutableRefObject, type SetStateAction } from 'react'
import type { GoalAnswer, GoalContinued, GoalJudged, GoalSnapshot } from '../shared/goal'
import { askingFirst } from './can-send'
import { markersOf } from './interjecting'
import type { Live } from './live'
import { call } from './rpc'
import { RequestFailed } from './rewind'
import type { Staged } from './staging'
import * as t from './transcript'

/**
 * `/goal` in the window (GOAL-1): the typed command, the goal the bridge sends back, and the step
 * the window asks for after each turn.
 *
 * The bridge holds the goal and decides what each check means (crates/ui-bridge/src/goal.rs). It
 * starts nothing unless asked, because the window owns the prompt queue. So once a turn under the
 * goal has ended and the queue is empty, or a check has sent the work back, the window sends
 * `session.goal.continue`, and the bridge starts the check or the turn that carries the work on.
 */

/** What `useGoal` takes from the component that owns the sessions. */
export interface GoalInputs {
  openedLives: MutableRefObject<Map<string, Live>>
  updateSession: (handle: string, action: SetStateAction<Live | null>) => void
  /** Chats whose queued prompt is between leaving the queue and its turn starting. */
  dequeuing: MutableRefObject<Set<string>>
  live: Live | null
  livesRevision: number
}

/** The chat with the goal the bridge sent in `goal.changed`. A check stopped by clearing the goal ends with it. */
export function goalChanged(live: Live, goal: GoalSnapshot | null): Live {
  const settled = { ...live, goal, goalNext: goal ? live.goalNext : false }
  return goal === null && live.goalChecking ? checkEnded(settled) : settled
}

/** The chat while the goal is out to the judge: drawn as a turn, so Stop and Escape send `turn.cancel` (GOAL-10). */
export function checkBegun(live: Live): Live {
  return {
    ...live,
    running: true,
    goalChecking: true,
    goalNext: false,
    phase: 'thinking',
    entries: [...live.entries, t.narrated('Checking whether the goal is met…', '/goal')],
  }
}

/** The chat once the check is over, however it ended. */
export function checkEnded(live: Live): Live {
  return { ...live, running: false, goalChecking: false, phase: null }
}

/** The chat with what a check came to: its sentence as a row, and the next step due when it sent the work back (GOAL-6, GOAL-13). */
export function judged(live: Live, data: GoalJudged): Live {
  const ended = live.goalChecking ? checkEnded(live) : live
  return {
    ...ended,
    goal: data.goal,
    goalNext: data.next === 'carry-on',
    entries: [...ended.entries, { ...t.narrated(data.line, '/goal'), ...(data.outcome === 'failed' ? { refused: true as const } : {}) }],
  }
}

/** The chat once a turn has ended: a goal that is set and not paused has a step to ask for. */
export function turnEnded(live: Live): Live {
  return live.goal && !live.goal.paused ? { ...live, goalNext: true } : live
}

/**
 * Whether the window asks for the goal's next step now: one is due, nothing runs, no question
 * waits, and no prompt the person queued is waiting or on its way to the bridge.
 */
export function mayContinue(live: Live, dequeuing: boolean): boolean {
  return !!live.goalNext && !live.running && !live.compacting && !askingFirst(live)
    && (live.queued?.length ?? 0) === 0 && !dequeuing
}

export function useGoal({ openedLives, updateSession, dequeuing, live, livesRevision }: GoalInputs) {
  /**
   * Send a typed `/goal` line to the bridge and draw what it answers as a row (CMD-1). It goes out
   * at once, while a turn runs too: setting a goal sends nothing (GOAL-2). Only the names of text
   * files dropped on the line go with it (DROP-4).
   */
  const command = useCallback(async (handle: string, argument: string, staged: Staged[]) => {
    try {
      const answer = await call<GoalAnswer>('session.goal', { session: handle, argument, ...markersOf(staged) })
      updateSession(handle, (old) => old ? { ...old, goal: answer.goal, entries: [...old.entries, t.narrated(answer.lines.join('\n'), '/goal')] } : old)
    } catch (error) {
      updateSession(handle, (old) => old ? { ...old, entries: [...old.entries, t.errored(error instanceof RequestFailed ? error.reason : String(error))] } : old)
    }
  }, [updateSession])

  /** Take a `goal.*` event, or a `turn.done`, for the chat `handle`. */
  const receive = useCallback((handle: string, message:
    | { event: 'goal.changed'; data: { goal: GoalSnapshot | null } }
    | { event: 'goal.checking' }
    | { event: 'goal.judged'; data: GoalJudged }
    | { event: 'goal.carried'; data: { prompt: string } }
    | { event: 'turn.done' }) => {
    updateSession(handle, (old) => {
      if (!old) return old
      switch (message.event) {
        case 'goal.changed': return goalChanged(old, message.data.goal)
        case 'goal.checking': return checkBegun(old)
        case 'goal.judged': return judged(old, message.data)
        // The turn this starts has the driver's sentence as its prompt, drawn as the driver's row.
        case 'goal.carried': return { ...old, entries: [...old.entries, t.goalCarried(message.data.prompt)] }
        case 'turn.done': return turnEnded(old)
      }
    })
  }, [updateSession])

  // Ask for the next step of each chat that has one and may take it. Cleared before the request,
  // so it is asked once. What the bridge starts says itself: `goal.checking`, or `turn.started`.
  useEffect(() => {
    for (const item of openedLives.current.values()) {
      if (!mayContinue(item, dequeuing.current.has(item.handle))) continue
      const handle = item.handle
      updateSession(handle, (old) => old ? { ...old, goalNext: false } : old)
      void call<GoalContinued>('session.goal.continue', { session: handle })
        .catch((error) => {
          updateSession(handle, (old) => old ? { ...old, entries: [...old.entries, t.errored(error instanceof RequestFailed ? error.reason : String(error))] } : old)
        })
    }
  }, [live, livesRevision, updateSession])

  return { command, receive }
}
