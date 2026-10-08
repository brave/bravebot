import type { ViewState } from './view.js'
import type { JsonValue, SessionViewCapability } from './wire.js'

/** What the runtime says about itself. The state-directory path `agent.info` carries is dropped. */
export interface TargetInfo {
  build: string | null
  version: string | null
  configured: boolean
  defaultModel: string | null
  sessionView: SessionViewCapability
  /** Whether the runtime names the turn a cancel is for, so a late cancel cannot stop a later turn. */
  actionTargets: boolean
}

export interface SendResult {
  /** The turn number the bridge accepted the prompt as. Completion is reported by the view. */
  turn: number
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
  /** Ask the bridge to cancel the running turn, which also refuses a pending question. The view reports the outcome. */
  cancel(): Promise<void>
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
