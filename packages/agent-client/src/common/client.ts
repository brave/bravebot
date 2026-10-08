import { describeThrown, isolate } from './isolate.js'
import { CapabilityError, ProtocolError, RpcError, UnsupportedError } from './errors.js'
import { RpcConnection, type Deadlines, type LineSink, type Outcome } from './connection.js'
import type { AgentClient, AgentSession, AskAnswer, CloseOutcome, SendResult, TargetInfo, ViewListener } from './interface.js'
import { applyUpdate, endView, startView, type ViewState } from './view.js'
import {
  SESSION_VIEW_START,
  SESSION_VIEW_VERSION,
  SUPPORTED_APPROVALS,
  decodeUpdate,
  readActionTargets,
  readSessionViewCapability,
  type BridgeEvent,
  type JsonValue,
  type SessionViewCapability,
} from './wire.js'

/** A refusal made here, without sending, because the displayed question no longer matches. */
export class StaleActionError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'StaleActionError'
  }
}

/** A workspace a client may open: the id and name are shown, the directory stays inside the client. */
export interface Workspace {
  id: string
  name: string
  directory: string
}

const MAX_EARLY_PER_SESSION = 256
const MAX_EARLY_SESSIONS = 64

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

/** The question kinds answered with a decision: every supported kind except the one that takes answers. */
const DECISION_KINDS: readonly string[] = SUPPORTED_APPROVALS.filter((kind) => kind !== 'ask')

function isAskAnswer(value: unknown): value is AskAnswer {
  if (value === null) return true
  if (!isRecord(value)) return false
  if (typeof value.typed === 'string') return true
  return Array.isArray(value.chosen) && value.chosen.every((index) => Number.isSafeInteger(index) && index >= 0)
}

class Session implements AgentSession {
  private current: ViewState | null = null
  private trust: JsonValue | null = null
  private readonly listeners = new Set<ViewListener>()
  private readonly replying = new Set<number>()
  /** The latest turn this session sent, which the view may not show yet. */
  private sent = 0
  /** Why the view could not start, when a malformed event arrived before it existed. */
  refused: string | null = null

  constructor(
    readonly id: string,
    private readonly connection: RpcConnection,
    private readonly forget: (id: string) => void,
    private readonly report: (message: string) => void,
    /** Whether the runtime names the turn a cancel is for. */
    private readonly namesTurns: boolean,
  ) {}

  get view(): ViewState {
    if (this.current === null) throw new ProtocolError('the view has not started')
    return this.current
  }

  get hasView(): boolean {
    return this.current !== null
  }

  get startupTrust(): JsonValue | null {
    return this.trust
  }

  subscribe(listener: ViewListener): () => void {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  /** Apply one event addressed to this session. Anything that is not view state is ignored. */
  handle(event: BridgeEvent): void {
    // A startup that broke the protocol stays broken; a later valid-looking event cannot repair it.
    if (this.refused !== null) return
    try {
      if (event.event === 'trust.request') {
        this.trust = event.data as JsonValue
      } else if (event.event === 'session.view.initial') {
        if (this.current !== null) throw new ProtocolError('a second initial view arrived')
        this.set(startView(decodeUpdate(event.data)))
      } else if (event.event === 'session.view.update') {
        if (this.current === null) throw new ProtocolError('a view update arrived before the initial view')
        this.set(applyUpdate(this.current, decodeUpdate(event.data)))
      }
    } catch (error) {
      if (!(error instanceof ProtocolError)) throw error
      if (this.current === null) this.refused = error.message
      else this.end('protocol_error', error.message)
    }
  }

  end(reason: 'protocol_error' | 'connection_lost', detail: string): void {
    if (this.current !== null) this.set(endView(this.current, reason, detail))
  }

  private set(next: ViewState): void {
    if (next === this.current) return
    this.current = next
    for (const listener of [...this.listeners]) {
      // A listener's failure is the listener's own; it must not stop other listeners or shutdown.
      isolate(() => listener(next), (error) => this.report(describeThrown('a view listener threw', error)))
    }
  }

  private params(extra: Record<string, unknown> = {}): Record<string, unknown> {
    return { session: this.id, ...extra }
  }

  private live(what: string): ViewState {
    const view = this.view
    if (view.ended !== null) throw new UnsupportedError(`cannot ${what}: the view ended (${view.ended.reason})`)
    return view
  }

  async answerTrust(trusted: boolean): Promise<void> {
    this.live('answer trust')
    const asked = this.trust
    await this.connection.request('trust.reply', this.params({ trusted }))
    // Answered questions are not offered again; a newer question that arrived meanwhile stays.
    if (this.trust === asked) this.trust = null
  }

  async send(text: string): Promise<SendResult> {
    this.live('send')
    const result = await this.connection.request('turn.send', this.params({ prompt: text }))
    if (!isRecord(result) || typeof result.turn !== 'number') throw new ProtocolError('turn.send did not report a turn')
    this.sent = Math.max(this.sent, result.turn)
    return { turn: result.turn }
  }

  /** The question on screen, if it is the one being answered and this client can answer it. */
  private target(request: number): NonNullable<ViewState['pending']> {
    const pending = this.live('reply').pending
    if (pending === null || pending.request !== request) {
      throw new StaleActionError(`request ${request} is not the question on screen`)
    }
    if (!pending.supported) throw new UnsupportedError(`a ${pending.kind} question cannot be answered here`)
    return pending
  }

  async decide(request: number, decision: 'approve' | 'reject'): Promise<void> {
    const pending = this.target(request)
    if (!DECISION_KINDS.includes(pending.kind)) {
      throw new UnsupportedError(`a ${pending.kind} question is not an approval to approve or reject`)
    }
    if (decision !== 'approve' && decision !== 'reject') {
      throw new UnsupportedError("a decision is 'approve' or 'reject'")
    }
    await this.reply(request, `${pending.kind}.reply`, { decision })
  }

  async answer(request: number, answers: AskAnswer[]): Promise<void> {
    const pending = this.target(request)
    if (pending.kind !== 'ask') throw new UnsupportedError(`a ${pending.kind} question takes a decision, not answers`)
    if (!Array.isArray(answers) || !answers.every(isAskAnswer)) {
      throw new UnsupportedError('answers are a list of { typed }, { chosen } or null')
    }
    await this.reply(request, 'ask.reply', { answers })
  }

  /** One reply per request at a time; the bridge would refuse the second, but the caller learns it here. */
  private async reply(request: number, method: string, body: Record<string, unknown>): Promise<void> {
    if (this.replying.has(request)) throw new StaleActionError(`a reply to request ${request} is already being sent`)
    this.replying.add(request)
    try {
      await this.connection.request(method, this.params({ request, ...body }))
    } finally {
      this.replying.delete(request)
    }
  }

  async cancel(): Promise<void> {
    // Name the turn to stop when the runtime can use it, so a cancel that arrives late cannot reach a
    // turn that began after the one meant. The turn just sent may not be in the view yet.
    const turn = Math.max(this.sent, this.current?.turn ?? 0)
    await this.connection.request('turn.cancel', this.params(this.namesTurns ? { turn } : {}), undefined, { control: true })
  }

  async close(): Promise<CloseOutcome> {
    try {
      await this.connection.request('session.close', this.params(), undefined, { control: true })
    } catch (error) {
      // The bridge says the session is already gone, so there is nothing left to keep registered.
      if (error instanceof RpcError && error.code === 'no_such_session') this.forget(this.id)
      throw error
    }
    this.forget(this.id)
    // The bridge detaches the view when it closes the session. Its update normally arrives first;
    // if it arrives after this response it is no longer routed here, so the view ends now.
    if (this.current !== null && this.current.ended === null) this.set(endView(this.current, 'detached', 'the bridge closed the session'))
    return { viewDetached: this.current?.ended?.reason === 'detached', workerTerminated: 'unknown', saved: 'unknown' }
  }
}

type RawAccess = (client: RpcAgentClient, method: string, params: Record<string, unknown>) => Promise<unknown>
let rawAccess: RawAccess | undefined

/**
 * Send any bridge method. This is for tests and diagnostics: it is not confined to configured
 * workspaces and replies to nothing on a caller's behalf. The package entry points do not export
 * it, and the client keeps its connection in a private field, so only code inside the package
 * that imports this module can reach it.
 */
export function rawRequest(client: RpcAgentClient, method: string, params: Record<string, unknown> = {}): Promise<unknown> {
  return rawAccess!(client, method, params)
}

/** The typed client over one connection to a `bravebot-rpc` process. */
export class RpcAgentClient implements AgentClient {
  static {
    rawAccess = (client, method, params) => client.#connection.request(method, params)
  }

  readonly #connection: RpcConnection
  private readonly sessions = new Map<string, Session>()
  private readonly early = new Map<string, BridgeEvent[]>()
  private capability: Promise<TargetInfo> | null = null
  /** How many `session.new` requests are in flight. Startup trust is held only while one is. */
  private creating = 0
  private readonly configured: readonly Workspace[]
  private readonly onDiagnostic: ((message: string) => void) | undefined

  constructor(
    sink: LineSink,
    options: { onDiagnostic?: (message: string) => void; onClosed?: (detail: string) => void; deadlines?: Deadlines; workspaces?: readonly Workspace[] } = {},
  ) {
    this.onDiagnostic = options.onDiagnostic
    this.configured = (options.workspaces ?? []).map((workspace) => ({ ...workspace }))
    this.#connection = new RpcConnection(sink, {
      onEvent: (event) => this.route(event),
      onClosed: (detail) => {
        this.lost(detail)
        isolate(() => options.onClosed?.(detail), (error) => this.report(describeThrown('the close callback threw', error)))
      },
      onDiagnostic: options.onDiagnostic,
    }, options.deadlines)
  }

  /** Feed text read from the transport. */
  receive(chunk: string): void {
    this.#connection.receive(chunk)
  }

  /** The transport ended. */
  transportClosed(detail: string): void {
    this.#connection.transportClosed(detail)
  }

  private route(event: BridgeEvent): void {
    if (event.session === null) return
    const session = this.sessions.get(event.session)
    if (session) {
      session.handle(event)
      return
    }
    // Only startup trust can precede the response that registers a session, so nothing is held
    // unless a creation is waiting for one. Other events may come from workers whose sessions
    // have already closed.
    if (event.event !== 'trust.request' || this.creating === 0) return
    const held = this.early.get(event.session)
    if (held) {
      if (held.length < MAX_EARLY_PER_SESSION) held.push(event)
    } else if (this.early.size < MAX_EARLY_SESSIONS) {
      // Entries nobody claims (raw sessions, closed sessions) live only until the last creation
      // in flight finishes. When a flood fills the map, the earliest entries are kept: they
      // belong to the creation most likely still waiting.
      this.early.set(event.session, [event])
    }
  }

  private report(message: string): void {
    isolate(() => this.onDiagnostic?.(message), () => undefined)
  }

  private lost(detail: string): void {
    this.early.clear()
    for (const session of this.sessions.values()) session.end('connection_lost', detail)
    // No event can arrive on a closed connection, so nothing is left to route to.
    this.sessions.clear()
  }

  describe(): Promise<TargetInfo> {
    if (this.capability === null) {
      const pending = this.#connection.request('agent.info').then((info) => this.target(info))
      this.capability = pending
      // A refusal of the capability or a lost connection is final; any other failure may be retried.
      pending.catch((error: unknown) => {
        if (this.capability === pending && !(error instanceof CapabilityError) && !this.#connection.closed) {
          this.capability = null
        }
      })
    }
    return this.capability
  }

  private target(info: unknown): TargetInfo {
    const view: SessionViewCapability | null = readSessionViewCapability(info)
    if (view === null) {
      throw new CapabilityError(
        `the runtime does not advertise session view version ${SESSION_VIEW_VERSION}; update bravebot-rpc`,
      )
    }
    const record = isRecord(info) ? info : {}
    return {
      build: typeof record.build === 'string' ? record.build : null,
      version: typeof record.version === 'string' ? record.version : null,
      configured: record.configured === true,
      defaultModel: typeof record.defaultModel === 'string' ? record.defaultModel : null,
      sessionView: view,
      actionTargets: readActionTargets(info),
    }
  }

  workspaces(): readonly { id: string; name: string }[] {
    return this.configured.map(({ id, name }) => ({ id, name }))
  }

  unsupported(operation: 'attach' | 'takeControl' | 'messageStatus'): Promise<never> {
    return Promise.reject(new UnsupportedError(`${operation} is unavailable on the local stdio client`))
  }

  async createSession(options: { workspace: string }): Promise<AgentSession> {
    const workspace = this.configured.find((candidate) => candidate.id === options.workspace)
    if (!workspace) throw new RpcError('unknown_workspace', 'that workspace is not configured', 'rejected')
    const described = await this.describe()
    let created: Session | null = null
    const opened = (outcome: Outcome): void => {
      if (!('ok' in outcome) || !isRecord(outcome.ok) || typeof outcome.ok.session !== 'string') return
      const session = new Session(outcome.ok.session, this.#connection, (id) => this.sessions.delete(id), (message) => this.report(message), described.actionTargets)
      created = session
      this.sessions.set(session.id, session)
      const held = this.early.get(session.id) ?? []
      this.early.delete(session.id)
      for (const event of held) session.handle(event)
    }
    this.creating++
    try {
      await this.#connection.request('session.new', { directory: workspace.directory }, opened)
    } finally {
      if (--this.creating === 0) this.early.clear()
    }
    if (created === null) throw new ProtocolError('session.new did not return a session handle')
    const session: Session = created
    try {
      await this.#connection.request(SESSION_VIEW_START, { session: session.id, version: SESSION_VIEW_VERSION })
      if (session.refused !== null || !session.hasView) {
        throw new ProtocolError(session.refused ?? 'the initial view did not arrive before the start response')
      }
    } catch (error) {
      this.sessions.delete(session.id)
      // Best effort and unawaited: the caller gets the startup failure now. The request has no deadline,
      // so a silent bridge leaves it pending until the connection ends rather than ending the connection.
      this.#connection.request('session.close', { session: session.id }, undefined, { untimed: true, control: true }).catch(() => undefined)
      throw error
    }
    return session
  }
}
