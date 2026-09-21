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
import { join } from 'node:path'
import { app } from 'electron'
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
  /** Diagnostics from the agent, kept for a bug report rather than parsed. */
  private diagnostics: string[] = []

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
    const packaged = join(process.resourcesPath ?? '', 'bravebot-rpc')
    if (app.isPackaged && existsSync(packaged)) return packaged
    return join(app.getAppPath(), 'target', 'debug', 'bravebot-rpc')
  }

  private ensure(): ChildProcessWithoutNullStreams {
    if (this.child && !this.child.killed) return this.child

    const path = this.binaryPath()
    if (!existsSync(path)) {
      throw new BridgeError(
        'no_binary',
        `bravebot-rpc is not built. Run \`npm run bridge\`. Looked in ${path}`,
      )
    }

    const child = spawn(path, this.settingsPath ? ['--settings', this.settingsPath] : [], { stdio: ['pipe', 'pipe', 'pipe'] })
    child.stdout.setEncoding('utf8')
    child.stderr.setEncoding('utf8')

    child.stdout.on('data', (chunk: string) => this.receive(chunk))
    child.stderr.on('data', (chunk: string) => {
      // Human-readable only, never parsed. Kept so a bug report can include it.
      this.diagnostics.push(chunk)
      if (this.diagnostics.length > 200) this.diagnostics.shift()
    })
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
      this.diagnostics.push(`unparseable line from bravebot-rpc: ${line.slice(0, 200)}`)
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

  /** The agent's own diagnostics, for a bug report. */
  stderr(): string {
    return this.diagnostics.join('')
  }

  /**
   * Shut down.
   *
   * Closing stdin is the graceful path and it is also the meaningful one: the agent
   * treats EOF as "the front-end has gone" and refuses anything waiting on an answer,
   * which is exactly right when the window is closing.
   */
  dispose(): void {
    if (!this.child) return
    try {
      this.child.stdin.end()
    } catch {
      // Already gone; the kill below covers it.
    }
    const child = this.child
    // A turn mid-model-call will not notice EOF immediately. Give it a moment, then stop.
    setTimeout(() => {
      if (!child.killed) child.kill('SIGTERM')
    }, 2000)
    this.child = null
  }
}
