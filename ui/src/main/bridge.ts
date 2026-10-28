/**
 * Talking to `bravebot-rpc`.
 *
 * One child process for the whole app. It is spawned on demand, supervised, and its
 * stdout is framed back into messages; requests are correlated by id and events are
 * handed to a listener.
 *
 * The renderer never sees any of this. It has no `child_process`, no `fs`, and no way to
 * reach the agent except the narrow surface in the preload.
 */

import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process'
import { existsSync } from 'node:fs'
import { app } from 'electron'
import { helperPaths } from './helpers'
import type { BridgeEvent, BridgeFailure } from '../shared/protocol'

/** A request waiting for its answer. */
interface Waiting {
  resolve: (value: unknown) => void
  reject: (error: Error) => void
}

export class BridgeError extends Error {
  constructor(readonly code: string, message: string) {
    super(message)
    this.name = 'BridgeError'
  }
}

export class Bridge {
  private child: ChildProcessWithoutNullStreams | null = null
  private waiting = new Map<number, Waiting>()
  private nextId = 0
  private buffer = ''

  constructor(private readonly onEvent: (event: BridgeEvent) => void, private settingsPath: string | null = null) {}

  /**
   * Where the binary is.
   *
   * Packaged, it ships beside the app as a resource. In development it is whatever
   * `cargo build` last produced. Release builds must be built with the agent's
   * credentials present — see docs/phase-0-rpc-protocol.md §3.2 — or the app runs but
   * cannot reach the backend.
   */
  private binaryPath(): string {
    const { packaged, development } = helperPaths('bravebot-rpc', app.getAppPath())
    if (app.isPackaged && existsSync(packaged)) return packaged
    return development
  }

  private ensure(): ChildProcessWithoutNullStreams {
    if (this.child && !this.child.killed) return this.child

    const path = this.binaryPath()
    if (!existsSync(path)) {
      throw new BridgeError(
        'no_binary',
        `bravebot-rpc is not built. Run \`pnpm run bridge\`. Looked in ${path}`,
      )
    }

    // The packaged app says so, which is what lets the agent ask the release listing at start
    // (UPDATE-12). A checkout does not, so a development run and a driver ask nothing.
    const env = app.isPackaged ? { ...process.env, BRAVEBOT_INSTALLED_VIA: 'desktop' } : process.env
    const child = spawn(path, this.settingsPath ? ['--settings', this.settingsPath] : [], { stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true, env })
    child.stdout.setEncoding('utf8')
    child.stderr.setEncoding('utf8')

    child.stdout.on('data', (chunk: string) => this.receive(chunk))
    // Read and let go. What the agent prints there is unvetted text (a refusal's detail, a panic),
    // so it is neither kept nor put in a bug report; the diagnostic log is what a report names
    // (DIAG-9). It is read so the pipe never fills and stalls the agent.
    child.stderr.on('data', () => {})
    child.on('exit', (code, signal) => this.died(code, signal))

    this.child = child
    return child
  }

  /**
   * Frame stdout back into lines.
   *
   * A chunk is not a message: it can hold several, or half of one.
   */
  private receive(chunk: string): void {
    this.buffer += chunk
    let newline = this.buffer.indexOf('\n')
    while (newline !== -1) {
      const line = this.buffer.slice(0, newline)
      this.buffer = this.buffer.slice(newline + 1)
      if (line.trim()) this.deliver(line)
      newline = this.buffer.indexOf('\n')
    }
  }

  private deliver(line: string): void {
    let message: Record<string, unknown>
    try {
      message = JSON.parse(line) as Record<string, unknown>
    } catch {
      // Not a message, so nothing waits on it.
      return
    }

    if (typeof message.event === 'string') {
      this.onEvent(message as unknown as BridgeEvent)
      return
    }

    const id = message.id
    if (typeof id !== 'number') return
    const waiting = this.waiting.get(id)
    if (!waiting) return
    this.waiting.delete(id)

    if ('error' in message) {
      const failure = message.error as BridgeFailure
      waiting.reject(new BridgeError(failure.code, failure.message))
    } else {
      waiting.resolve(message.ok)
    }
  }

  /**
   * The agent has gone.
   *
   * Everything still waiting is failed rather than left hanging. A request that can never
   * be answered is not pending, it is refused, and a UI that shows a spinner forever is
   * worse than one that says what happened.
   */
  private died(code: number | null, signal: string | null): void {
    const detail = signal ? `killed by ${signal}` : `exited with code ${code}`
    for (const [, waiting] of this.waiting) {
      waiting.reject(new BridgeError('agent_gone', `bravebot-rpc ${detail}`))
    }
    this.waiting.clear()
    this.child = null
    this.buffer = ''
  }

  /** Send a request and wait for its answer. */
  request<T = unknown>(method: string, params: Record<string, unknown> = {}): Promise<T> {
    const child = this.ensure()
    const id = ++this.nextId

    return new Promise<T>((resolve, reject) => {
      this.waiting.set(id, { resolve: resolve as (value: unknown) => void, reject })
      const line = JSON.stringify({ id, method, params }) + '\n'
      child.stdin.write(line, (error) => {
        if (error) {
          this.waiting.delete(id)
          reject(new BridgeError('write_failed', error.message))
        }
      })
    })
  }

  async selectSettings<T>(path: string | null): Promise<T> {
    const report = await this.request<T>('settings.select', { path })
    this.settingsPath = path
    return report
  }

  /**
   * Shut down.
   *
   * Closing stdin is the graceful path and it is also the meaningful one: the agent
   * treats EOF as "the front-end has gone" and refuses anything waiting on an answer,
   * which is exactly right when the window is closing. It also closes each session, which fires its
   * `session-ended` hook, so the agent waits up to ten seconds for that and for a turn still writing
   * its record, and ends by itself when it is done.
   */
  dispose(): void {
    if (!this.child) return
    try {
      this.child.stdin.end()
    } catch {
      // Already gone; the kill below covers it.
    }
    const child = this.child
    // A turn mid-model-call will not notice EOF immediately, and the agent waits up to ten seconds
    // for the work it is closing. Give it that and a moment more, then stop it.
    setTimeout(() => {
      if (!child.killed) child.kill('SIGTERM')
    }, 12000)
    this.child = null
  }
}
