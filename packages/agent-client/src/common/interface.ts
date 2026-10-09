import type { ViewState } from './view.js'
import type { JsonValue, SessionViewCapability } from './wire.js'

/** What the runtime says about itself. The state-directory path `agent.info` carries is dropped. */
export interface TargetInfo {
  build: string | null
  version: string | null
  configured: boolean
  defaultModel: string | null
  sessionView: SessionViewCapability
  /** Whether the runtime names what a cancel is for, so a late cancel cannot stop a later turn. */
  actionTargets: boolean
}

export interface SendResult {
  /** The turn number the bridge accepted the prompt as. Completion is reported by the view. */
  turn: number
  /** What a cancel names to stop this turn. Unlike `turn`, it is never reused, so it stays exact after a rewind. */
  target: number
}

export interface CancelResult {
  /**
   * Whether the bridge stopped the named turn. `false` means the turn had already ended or another
   * had begun, and nothing was stopped. `null` means the cancel named nothing and stopped whatever
   * was running, because the runtime cannot name a target, the view had ended, or a send was unanswered.
   */
  cancelled: boolean | null
}

/**
 * What a close request established. The view is detached, which says nothing about the worker
 * having stopped or the session record having been saved; both stay unknown.
 */
export interface CloseOutcome {
  viewDetached: boolean
  workerTerminated: 'unknown'
  saved: 'unknown'
}

/** One answer to an `ask` question, in the bridge's existing shape. `null` declines. */
export type AskAnswer = { typed: string } | { chosen: number[] } | null

/** A listener may be async; a thrown error or rejection is reported and goes no further. */
export type ViewListener = (view: ViewState) => void

/** One fresh session with a view. Every method addresses exactly this session. */
export interface AgentSession {
  readonly id: string
  /** The latest view. Replaced, never mutated, so a held value stays consistent. */
  readonly view: ViewState
  /** The startup trust question as the bridge sent it, or null if none was asked or it has been answered. Opaque. */
  readonly startupTrust: JsonValue | null
  /** Called on every change, including when the view ends. Returns an unsubscribe function. */
  subscribe(listener: ViewListener): () => void
  /** Answer the startup trust question. Never sent unless a caller asks; never remembered. */
  answerTrust(trusted: boolean): Promise<void>
  send(text: string): Promise<SendResult>
  /**
   * Approve or reject the displayed `confirm`, `run` (once, never remembered) or `fetch` question.
   * A second reply to a request while the first is being sent is refused locally.
   */
  decide(request: number, decision: 'approve' | 'reject'): Promise<void>
  /** Answer the displayed `ask` question. */
  answer(request: number, answers: AskAnswer[]): Promise<void>
  /**
   * Ask the bridge to cancel a turn, which also refuses a pending question. `target` is the one a
   * `SendResult` or the view reported; without it the turn on screen is named. The view reports the outcome.
   */
  cancel(target?: number): Promise<CancelResult>
  close(): Promise<CloseOutcome>
}

export interface AgentClient {
  /** Describe the runtime. Rejects with `CapabilityError` if it lacks session view version 1. */
  describe(): Promise<TargetInfo>
  /** The workspaces this client may open, by id. Paths are never exposed. */
  workspaces(): readonly { id: string; name: string }[]
  /** Open a fresh session in a configured workspace. An unknown id is refused without sending. */
  createSession(options: { workspace: string }): Promise<AgentSession>
  /** Operations a later stage adds. They fail locally and send nothing. */
  unsupported(operation: 'attach' | 'takeControl' | 'messageStatus'): Promise<never>
}
