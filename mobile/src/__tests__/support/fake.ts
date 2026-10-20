import type {
  AgentClient,
  AgentSession,
  AskAnswer,
  CancelResult,
  CloseOutcome,
  JsonValue,
  Pending,
  Row,
  SendResult,
  SessionSummary,
  Status,
  TargetInfo,
  ViewEndReason,
  ViewListener,
  ViewState,
} from '@brave/agent-client'

/** What one change to a fake session's view sets. Rows replace the row with the same id or are appended. */
export interface ViewPatch {
  turn?: number
  target?: number
  status?: Status
  pending?: Pending | null
  rows?: readonly Row[]
}

export interface Call {
  method: 'send' | 'decide' | 'answer' | 'answerTrust' | 'cancel' | 'close'
  args: unknown[]
}

/** What a fake session does when called. Each may change the view with `emit`, wait, or throw. The default accepts and changes nothing. */
export interface Handlers {
  send?: (text: string) => Promise<SendResult | void> | SendResult | void
  decide?: (request: number, decision: 'approve' | 'reject') => Promise<void> | void
  answer?: (request: number, answers: AskAnswer[]) => Promise<void> | void
  answerTrust?: (trusted: boolean) => Promise<void> | void
  cancel?: (target?: number) => Promise<CancelResult | void> | CancelResult | void
  close?: () => Promise<void> | void
}

/**
 * A session whose view a test sets directly. It keeps the rules the app relies on: a view is replaced
 * rather than changed, each change takes the next sequence number, and every call is recorded.
 */
export class FakeSession implements AgentSession {
  readonly calls: Call[] = []
  readonly on: Handlers = {}
  readonly workspace: string
  firstPrompt: string | null
  startupTrust: JsonValue | null
  private current: ViewState
  private readonly listeners = new Set<ViewListener>()

  constructor(options: { id: string; workspace?: string; firstPrompt?: string | null; initial?: ViewPatch; trust?: JsonValue | null }) {
    this.id = options.id
    this.workspace = options.workspace ?? 'work'
    this.firstPrompt = options.firstPrompt ?? null
    this.startupTrust = options.trust ?? null
    const initial = options.initial ?? {}
    this.current = {
      sequence: 0,
      turn: initial.turn ?? 0,
      target: initial.target ?? 0,
      status: initial.status ?? (options.trust ? 'awaiting_trust' : 'idle'),
      pending: initial.pending ?? null,
      rows: [...(initial.rows ?? [])],
      ended: null,
    }
  }

  readonly id: string

  get view(): ViewState {
    return this.current
  }

  /** The methods called, in order. */
  get methods(): string[] {
    return this.calls.map((call) => call.method)
  }

  subscribe(listener: ViewListener): () => void {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  /** Change the view as the runtime would, and tell every listener. */
  emit(patch: ViewPatch): ViewState {
    const rows = [...this.current.rows]
    for (const row of patch.rows ?? []) {
      const at = rows.findIndex((candidate) => candidate.id === row.id)
      if (at === -1) rows.push(row)
      else rows[at] = row
    }
    this.current = {
      sequence: this.current.sequence + 1,
      turn: patch.turn ?? this.current.turn,
      target: patch.target ?? this.current.target,
      status: patch.status ?? this.current.status,
      pending: patch.pending === undefined ? this.current.pending : patch.pending,
      rows,
      ended: this.current.ended,
    }
    this.notify()
    return this.current
  }

  /** The view ends, as it does when the connection is lost. */
  end(reason: ViewEndReason = 'connection_lost'): void {
    this.current = { ...this.current, ended: { reason, detail: '' } }
    this.notify()
  }

  private notify(): void {
    for (const listener of [...this.listeners]) listener(this.current)
  }

  async answerTrust(trusted: boolean): Promise<void> {
    this.calls.push({ method: 'answerTrust', args: [trusted] })
    await this.on.answerTrust?.(trusted)
  }

  async send(text: string): Promise<SendResult> {
    this.calls.push({ method: 'send', args: [text] })
    return (await this.on.send?.(text)) ?? { turn: this.current.turn + 1, target: this.current.target + 1 }
  }

  async decide(request: number, decision: 'approve' | 'reject'): Promise<void> {
    this.calls.push({ method: 'decide', args: [request, decision] })
    await this.on.decide?.(request, decision)
  }

  async answer(request: number, answers: AskAnswer[]): Promise<void> {
    this.calls.push({ method: 'answer', args: [request, answers] })
    await this.on.answer?.(request, answers)
  }

  async cancel(target?: number): Promise<CancelResult> {
    this.calls.push({ method: 'cancel', args: [target] })
    return (await this.on.cancel?.(target)) ?? { cancelled: true }
  }

  async close(): Promise<CloseOutcome> {
    this.calls.push({ method: 'close', args: [] })
    await this.on.close?.()
    return { viewDetached: true, workerTerminated: 'unknown', saved: 'unknown' }
  }

  /** What the target lists for this session. */
  summary(): SessionSummary {
    return {
      id: this.id,
      workspace: this.workspace,
      firstPrompt: this.firstPrompt,
      status: this.current.status,
      waiting: this.current.pending !== null,
      ended: this.current.ended !== null,
    }
  }
}

/** A client whose sessions a test makes, lists and opens. */
export class FakeClient implements AgentClient {
  readonly created: FakeSession[] = []
  readonly opened: string[] = []
  /** Sessions the target lists, newest first. */
  sessions: FakeSession[]
  private serial = 0

  constructor(
    options: {
      workspaces?: readonly { id: string; name: string }[]
      sessions?: FakeSession[]
      /** Makes the session `createSession` returns. */
      make?: (workspace: string, id: string) => FakeSession
    } = {},
  ) {
    this.configured = options.workspaces ?? [{ id: 'work', name: 'work' }]
    this.sessions = options.sessions ?? []
    this.make = options.make ?? ((workspace, id) => new FakeSession({ id, workspace }))
  }

  private readonly configured: readonly { id: string; name: string }[]
  private readonly make: (workspace: string, id: string) => FakeSession

  async describe(): Promise<TargetInfo> {
    throw new Error('the app does not call describe')
  }

  workspaces(): readonly { id: string; name: string }[] {
    return this.configured
  }

  async createSession(options: { workspace: string }): Promise<AgentSession> {
    const session = this.make(options.workspace, `new-${++this.serial}`)
    this.created.push(session)
    this.sessions = [session, ...this.sessions]
    return session
  }

  async listSessions(): Promise<readonly SessionSummary[]> {
    return this.sessions.map((session) => session.summary())
  }

  async openSession(id: string): Promise<AgentSession> {
    this.opened.push(id)
    const session = this.sessions.find((candidate) => candidate.id === id)
    if (session === undefined) throw new Error(`no session ${id}`)
    return session
  }

  async unsupported(): Promise<never> {
    throw new Error('not used')
  }
}
