/**
 * The desktop's end of the host's channel: the shared client over it, and the state the prototype
 * window draws.
 *
 * Everything about a session comes from the Rust view the shared client holds (RPCVIEW-1). This
 * file chooses which session is on screen and turns a refusal into the host's code and words. It
 * decides nothing from what a row or a question says.
 */

import { basename } from 'node:path'
import { RpcAgentClient, RpcError, StaleActionError, type AgentSession, type HeldSession } from '../../../../packages/agent-client/src/common/index'
import type { PrototypeAction, PrototypeOutcome, PrototypeState, PrototypeView } from '../../shared/prototype'
import type { HostClientOptions } from './options'

/** The host's channel, as the main process holds it. */
export interface Channel {
  /** Called with each chunk the host wrote. */
  onData(listener: (chunk: string) => void): void
  /** Called when the host's end has gone, with why where the channel said. */
  onEnd(listener: (reason?: string) => void): void
  write(line: string): void
}

/** How long a request may go unanswered before the connection is given up on. */
const REQUEST_MS = 30_000

function viewOf(session: AgentSession): PrototypeView {
  const view = session.view
  return {
    sequence: view.sequence,
    turn: view.turn,
    target: view.target,
    status: view.status,
    actor: view.actor,
    pending: view.pending,
    rows: view.rows,
    omitted: view.omitted,
    ended: view.ended,
  }
}

const refused = (error: unknown): PrototypeOutcome => {
  if (error instanceof RpcError) return { ok: false, code: error.code, message: error.message }
  if (error instanceof StaleActionError) return { ok: false, code: 'stale', message: error.message }
  return { ok: false, code: 'failed', message: error instanceof Error ? error.message : String(error) }
}

export class HostClient {
  private readonly client: RpcAgentClient
  /** The sessions this connection is attached to. A host session stays attached until the channel ends. */
  private readonly known = new Map<string, AgentSession>()
  private selected: AgentSession | null = null
  /** Which choice of session is the latest, so a slow one that finishes late does not take the window back. */
  private choice = 0
  private unsubscribe: (() => void) | null = null
  private answers: PrototypeState['answers'] = []
  private held: readonly HeldSession[] = []
  private ended: string | null = null

  constructor(
    private readonly options: HostClientOptions,
    channel: Channel,
    private readonly changed: () => void,
  ) {
    this.client = new RpcAgentClient(
      { write: (line) => channel.write(line) },
      {
        workspaces: [{ id: options.workspace, name: basename(options.workspace) || options.workspace, directory: options.workspace }],
        onClosed: (detail) => this.close(detail),
        deadlines: {
          ms: REQUEST_MS,
          schedule(ms, fire) {
            const timer = setTimeout(fire, ms)
            return () => clearTimeout(timer)
          },
        },
      },
    )
    channel.onData((chunk) => this.client.receive(chunk))
    channel.onEnd((reason) => this.client.transportClosed(reason ?? 'the host closed the channel'))
  }

  private close(detail: string): void {
    if (this.ended !== null) return
    this.ended = detail
    this.changed()
  }

  state(): PrototypeState {
    return {
      channel: this.ended === null ? 'open' : 'ended',
      endedReason: this.ended,
      name: this.options.name,
      workspace: basename(this.options.workspace) || this.options.workspace,
      answers: this.answers,
      held: this.held,
      session: this.selected
        ? { id: this.selected.id, view: viewOf(this.selected), startupTrust: this.selected.startupTrust }
        : null,
    }
  }

  /** The question to the host that is out now, so answers cannot arrive out of the order they were asked. */
  private refreshing: Promise<void> | null = null

  /**
   * Ask the host what it holds and what this client may answer. A question already out is shared,
   * unless `fresh` says what the host holds has just changed: the answer to the one out was made
   * before it did, so a new one follows it.
   */
  refresh(fresh = false): Promise<void> {
    if (this.refreshing !== null && !fresh) return this.refreshing
    const after = this.refreshing ?? Promise.resolve()
    const mine: Promise<void> = after
      .catch(() => undefined)
      .then(() => this.ask())
      .finally(() => {
        if (this.refreshing === mine) this.refreshing = null
      })
    this.refreshing = mine
    return mine
  }

  private async ask(): Promise<void> {
    const info = await this.client.instance()
    // The window is told only of a change, so a quiet host costs it nothing.
    if (JSON.stringify([this.answers, this.held]) === JSON.stringify([info.answers, info.sessions])) return
    this.answers = info.answers
    this.held = info.sessions
    this.changed()
  }

  /** Put a session on screen, and tell the window now, not when the host next answers. */
  private show(session: AgentSession | null): void {
    this.select(session)
    this.changed()
  }

  private select(session: AgentSession | null): void {
    this.unsubscribe?.()
    this.selected = session
    this.unsubscribe = session ? session.subscribe(() => this.changed()) : null
  }

  async act(action: PrototypeAction): Promise<PrototypeOutcome> {
    try {
      await this.run(action)
      // What the action changed here, such as a trust question answered, is known only now.
      this.changed()
      return { ok: true }
    } catch (error) {
      return refused(error)
    }
  }

  /** The session on screen, if it is the one an action is for. */
  private session(named: string): AgentSession {
    if (this.selected === null) throw new RpcError('no_session', 'no session is open', 'rejected')
    if (this.selected.id !== named) throw new RpcError('stale_session', 'that session is no longer the one on screen', 'rejected')
    return this.selected
  }

  private async run(action: PrototypeAction): Promise<void> {
    switch (action.kind) {
      case 'refresh':
        return this.refresh()
      case 'new': {
        const mine = ++this.choice
        await this.client.describe()
        const session = await this.client.createSession({ workspace: this.options.workspace })
        this.known.set(session.id, session)
        if (mine === this.choice) this.show(session)
        return this.refresh(true)
      }
      case 'attach': {
        const choice = ++this.choice
        const held = this.known.get(action.session)
        if (held && held.view.ended === null) return this.show(held)
        this.known.delete(action.session)
        const session = await this.client.attach(action.session)
        this.known.set(session.id, session)
        if (choice === this.choice) this.show(session)
        return this.refresh(true)
      }
      case 'leave':
        // Only the window's view of it. The host keeps the session and its work, and this
        // connection stays attached until the channel ends.
        ++this.choice
        this.show(null)
        return
      case 'send':
        await this.session(action.session).send(action.text)
        return
      case 'cancel':
        await this.session(action.session).cancel()
        return
      case 'decide':
        return this.session(action.session).decide(action.request, action.decision)
      case 'answer':
        return this.session(action.session).answer(action.request, action.answers)
      case 'trust':
        return this.session(action.session).answerTrust(action.trusted)
    }
  }
}
