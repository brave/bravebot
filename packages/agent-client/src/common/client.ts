import { CapabilityError, ProtocolError, RpcError, UnsupportedError } from './errors.js'
import { RpcConnection, type Deadlines, type LineSink, type Outcome } from './connection.js'
import type { AgentClient, AgentSession, CloseOutcome, SendResult, TargetInfo, ViewListener } from './interface.js'
import { applyUpdate, endView, startView, type ViewState } from './view.js'
import {
  SESSION_VIEW_START,
  SESSION_VIEW_VERSION,
  decodeUpdate,
  readSessionViewCapability,
  type BridgeEvent,
  type JsonValue,
  type SessionViewCapability,
} from './wire.js'

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

class Session implements AgentSession {
  private current: ViewState | null = null
  private trust: JsonValue | null = null
  private readonly listeners = new Set<ViewListener>()
  /** Why the view could not start, when a malformed event arrived before it existed. */
  refused: string | null = null

  constructor(
    readonly id: string,
    private readonly connection: RpcConnection,
    private readonly forget: (id: string) => void,
    private readonly report: (message: string) => void,
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
      try {
        listener(next)
      } catch (error) {
        let message = 'a view listener threw'
        try {
          message += `: ${error instanceof Error ? error.message : String(error)}`
        } catch {
          // Thrown values need not support conversion to text.
        }
        try {
          this.report(message)
        } catch {
          // Diagnostic callbacks cannot interrupt view delivery or shutdown either.
        }
      }
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
    await this.connection.request('trust.reply', this.params({ trusted }))
  }

  async send(text: string): Promise<SendResult> {
    this.live('send')
    const result = await this.connection.request('turn.send', this.params({ prompt: text }))
    if (!isRecord(result) || typeof result.turn !== 'number') throw new ProtocolError('turn.send did not report a turn')
    return { turn: result.turn }
  }

  async cancel(): Promise<void> {
    await this.connection.request('turn.cancel', this.params())
  }

  async close(): Promise<CloseOutcome> {
    await this.connection.request('session.close', this.params())
    this.forget(this.id)
    return { viewDetached: this.current?.ended?.reason === 'detached', workerTerminated: 'unknown', saved: 'unknown' }
  }
}

/** The typed client over one connection to a `bravebot-rpc` process. */
export class RpcAgentClient implements AgentClient {
  private readonly connection: RpcConnection
  private readonly sessions = new Map<string, Session>()
  private readonly early = new Map<string, BridgeEvent[]>()
  private capability: Promise<TargetInfo> | null = null
  private readonly configured: readonly Workspace[]
  private readonly onDiagnostic: ((message: string) => void) | undefined

  constructor(
    sink: LineSink,
    options: { onDiagnostic?: (message: string) => void; onClosed?: (detail: string) => void; deadlines?: Deadlines; workspaces?: readonly Workspace[] } = {},
  ) {
    this.onDiagnostic = options.onDiagnostic
    this.configured = (options.workspaces ?? []).map((workspace) => ({ ...workspace }))
    this.connection = new RpcConnection(sink, {
      onEvent: (event) => this.route(event),
      onClosed: (detail) => {
        this.lost(detail)
        options.onClosed?.(detail)
      },
      onDiagnostic: options.onDiagnostic,
    }, options.deadlines)
  }

  /** Feed text read from the transport. */
  receive(chunk: string): void {
    this.connection.receive(chunk)
  }

  /** The transport ended. */
  transportClosed(detail: string): void {
    this.connection.transportClosed(detail)
  }

  private route(event: BridgeEvent): void {
    if (event.session === null) return
    const session = this.sessions.get(event.session)
    if (session) {
      session.handle(event)
      return
    }
    // Only startup trust can precede the response that registers a session. Other events
    // may come from workers whose sessions have already closed.
    if (event.event !== 'trust.request') return
    const held = this.early.get(event.session)
    if (held) {
      if (held.length < MAX_EARLY_PER_SESSION) held.push(event)
    } else if (this.early.size < MAX_EARLY_SESSIONS) {
      this.early.set(event.session, [event])
    }
  }

  private report(message: string): void {
    this.onDiagnostic?.(message)
  }

  private lost(detail: string): void {
    for (const session of this.sessions.values()) session.end('connection_lost', detail)
  }

  describe(): Promise<TargetInfo> {
    if (this.capability === null) {
      this.capability = this.connection.request('agent.info').then((info) => this.target(info))
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
    if (!workspace) throw new RpcError('unknown_workspace', 'that workspace is not configured')
    await this.describe()
    let created: Session | null = null
    const opened = (outcome: Outcome): void => {
      if (!('ok' in outcome) || !isRecord(outcome.ok) || typeof outcome.ok.session !== 'string') return
      const session = new Session(outcome.ok.session, this.connection, (id) => this.sessions.delete(id), (message) => this.report(message))
      created = session
      this.sessions.set(session.id, session)
      const held = this.early.get(session.id) ?? []
      this.early.delete(session.id)
      for (const event of held) session.handle(event)
    }
    await this.connection.request('session.new', { directory: workspace.directory }, opened)
    if (created === null) throw new ProtocolError('session.new did not return a session handle')
    const session: Session = created
    try {
      await this.connection.request(SESSION_VIEW_START, { session: session.id, version: SESSION_VIEW_VERSION })
      if (!session.hasView) {
        throw new ProtocolError(session.refused ?? 'the initial view did not arrive before the start response')
      }
    } catch (error) {
      this.sessions.delete(session.id)
      await this.connection.request('session.close', { session: session.id }).catch(() => undefined)
      throw error
    }
    return session
  }

  raw(method: string, params: Record<string, unknown> = {}): Promise<unknown> {
    return this.connection.request(method, params)
  }
}
