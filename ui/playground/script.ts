import type { AskAnswer, EventMap, EventName } from '../src/shared/protocol'

/** The kinds of question the person answers with a yes or a no. Mirrors `REPLY` in `transcript.ts`. */
export type Kind =
  | 'confirm' | 'run' | 'output' | 'vouch' | 'vet' | 'fetch' | 'server' | 'manifest' | 'exposure'
  | 'mcp-server' | 'mcp-tools' | 'mcp-call' | 'mcp-move' | 'host' | 'ask'

/** What the person said, or what the auto-answerer said for them. */
export interface Answered {
  decision: 'approve' | 'reject'
  remember: boolean
  answers?: AskAnswer[]
}

/** What the auto-answerer is asked to press when it is on. */
export type Want = 'approve' | 'reject' | 'always'

/**
 * What a scenario can do. It is the agent's side of a conversation: it announces things and waits
 * for the person, and it never touches the transcript directly.
 */
export interface Script {
  /** The handle of the session the turn runs in. */
  readonly session: string
  /** A fresh request number, so the same scenario can run twice in one conversation. */
  id(): number
  emit<N extends EventName>(event: N, data: EventMap[N]): void
  /** Pause, scaled by the speed the panel is set to. Rejects when the turn is stopped. */
  wait(ms: number): Promise<void>
  /** Park until the turn is stopped. */
  hold(): Promise<never>
  /** Draw a question and wait for its answer. `want` is what auto-answer presses. */
  ask(kind: Kind, request: number, want?: Want): Promise<Answered>
  /** `turn.started`, with the next turn number. */
  start(): void
  /** `turn.done` with a reply. */
  done(reply: string, extra?: { notices?: string[]; steps?: number; rewind?: boolean; consolidating?: boolean }): void
  /** `turn.error`, as the agent reports a failure. */
  fail(kind: 'cancelled' | 'precommit' | 'workspace' | 'chat', message: string, extra?: Record<string, unknown>): void
}

export type Scenario = (script: Script) => Promise<void>

export interface Described {
  id: string
  group: string
  title: string
  detail: string
  run: Scenario
  /** Needs the conversation to be a manifest run rather than a turn. */
  plan?: boolean
}
