import { describeFailure, describeThrown, isolate } from './isolate.js'
import { ConnectionLostError, ProtocolError, RpcError } from './errors.js'
import { LineFramer } from './framing.js'
import { decodeIncoming, type BridgeEvent } from './wire.js'

/** Where a connection writes. A transport adapter supplies it; throwing means the write failed. */
export interface LineSink {
  write(line: string): void
}

export interface ConnectionHandlers {
  /** Called for every event, in arrival order, before any later line is read. */
  onEvent(event: BridgeEvent): void
  /** Called once when the connection ends, for whatever reason. */
  onClosed(detail: string): void
  /** Lines that could not be read. Never parsed for meaning. */
  onDiagnostic?(message: string): void
}

/** Deadlines for requests. The common code has no timers; the adapter supplies them. */
export interface Deadlines {
  ms: number
  /** Run `fire` after `ms`; the returned function cancels it. */
  schedule(ms: number, fire: () => void): () => void
}

export type Outcome = { ok: unknown } | { error: RpcError }

interface Waiting {
  resolve: (value: unknown) => void
  reject: (error: Error) => void
  sync: ((outcome: Outcome) => void) | undefined
  cancel: (() => void) | undefined
}

/**
 * Framing and request correlation over a text stream.
 *
 * Responses are matched to requests by id and may arrive in any order. Events are handed on as
 * they are read, so an event written before the response to the request that caused it is seen
 * first. A request's `sync` callback runs inside the read of its own response, before the next
 * line, which is how a caller registers state that following events need.
 */
export class RpcConnection {
  private readonly framer = new LineFramer()
  private readonly waiting = new Map<number, Waiting>()
  private nextId = 0
  private ended: string | null = null

  constructor(
    private readonly sink: LineSink,
    private readonly handlers: ConnectionHandlers,
    private readonly deadlines?: Deadlines,
  ) {}

  get closed(): boolean {
    return this.ended !== null
  }

  /**
   * Send a request. An `untimed` request has no deadline, for best-effort work whose silence must
   * not end the connection.
   */
  request(
    method: string,
    params: Record<string, unknown> = {},
    sync?: (outcome: Outcome) => void,
    options: { untimed?: boolean } = {},
  ): Promise<unknown> {
    if (this.ended !== null) return Promise.reject(new ConnectionLostError(this.ended))
    const id = ++this.nextId
    return new Promise((resolve, reject) => {
      const cancel = options.untimed
        ? undefined
        : this.deadlines?.schedule(this.deadlines.ms, () =>
            this.close(`${method} had no answer within ${this.deadlines?.ms}ms; its outcome is unknown`),
          )
      this.waiting.set(id, { resolve, reject, sync, cancel })
      try {
        this.sink.write(JSON.stringify({ id, method, params }) + '\n')
      } catch (error) {
        this.waiting.delete(id)
        cancel?.()
        reject(new RpcError('write_failed', describeFailure(error) ?? 'unreadable failure'))
      }
    })
  }

  /** Feed text read from the transport. */
  receive(chunk: string): void {
    if (this.ended !== null) return
    let lines: string[]
    try {
      lines = this.framer.push(chunk)
    } catch (error) {
      this.close(describeFailure(error) ?? 'unreadable failure')
      return
    }
    for (const line of lines) {
      if (this.ended !== null) return
      this.deliver(line)
    }
  }

  /** The transport ended (EOF, exit or error). Requests in flight fail; nothing is retried. */
  transportClosed(detail: string): void {
    this.close(detail)
  }

  /** Diagnostics are advisory: a hook that throws must not discard the message being read. */
  private diagnose(message: string): void {
    isolate(() => this.handlers.onDiagnostic?.(message), () => undefined)
  }

  private deliver(line: string): void {
    let parsed: unknown
    try {
      parsed = JSON.parse(line)
    } catch {
      this.diagnose(`unparseable line: ${line.slice(0, 200)}`)
      return
    }
    let incoming
    try {
      incoming = decodeIncoming(parsed)
    } catch (error) {
      this.diagnose(error instanceof ProtocolError ? error.message : (describeFailure(error) ?? 'unreadable failure'))
      return
    }
    if (incoming.type === 'event') {
      // A handler's failure must not discard the lines still to be read from this chunk.
      isolate(() => this.handlers.onEvent(incoming.event), (error) => {
        this.diagnose(describeThrown('an event handler failed', error))
      })
      return
    }
    const waiting = this.waiting.get(incoming.id)
    if (!waiting) {
      this.diagnose(`response ${incoming.id} matches no request`)
      return
    }
    this.waiting.delete(incoming.id)
    waiting.cancel?.()
    const result = incoming.result
    if ('error' in result) {
      const error = new RpcError(result.error.code, result.error.message)
      this.sync(waiting, { error })
      waiting.reject(error)
    } else {
      this.sync(waiting, { ok: result.ok })
      waiting.resolve(result.ok)
    }
  }

  /** A `sync` callback's failure must not leave its request unsettled or discard the rest of the chunk. */
  private sync(waiting: Waiting, outcome: Outcome): void {
    isolate(() => waiting.sync?.(outcome), (error) => {
      this.diagnose(describeThrown('a response handler failed', error))
    })
  }

  private close(detail: string): void {
    if (this.ended !== null) return
    this.ended = detail
    this.framer.reset()
    const failed = [...this.waiting.values()]
    this.waiting.clear()
    for (const waiting of failed) waiting.cancel?.()
    isolate(() => this.handlers.onClosed(detail), (error) => {
      this.diagnose(describeThrown('the close callback failed', error))
    })
    for (const waiting of failed) waiting.reject(new ConnectionLostError(detail))
  }
}
