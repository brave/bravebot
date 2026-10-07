import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process'
import { RpcAgentClient, type Workspace } from '../common/client.js'
import type { AgentClient } from '../common/index.js'

export interface StdioOptions {
  /** Absolute path of the `bravebot-rpc` binary. */
  command: string
  args?: string[]
  /** The whole environment of the child. It is not merged with this process's own. */
  env: NodeJS.ProcessEnv
  cwd?: string
  onDiagnostic?: (message: string) => void
  /** How long a request may go unanswered before the connection ends and the child is stopped. Default 30000. */
  requestTimeoutMs?: number
  /** The workspaces `createSession` may open. */
  workspaces?: readonly Workspace[]
}

export interface Exit {
  code: number | null
  signal: NodeJS.Signals | null
}

/** A client bound to a child process speaking the bridge's newline-delimited JSON on stdio. */
export interface StdioConnection {
  /** The operations a caller may use. */
  readonly client: AgentClient
  readonly child: ChildProcessWithoutNullStreams
  /** What the child wrote to stderr: human-readable, never parsed. */
  stderr(): string
  /** Resolves when the child has exited and its output has been read. */
  readonly exited: Promise<Exit>
  /** End the child's input. The bridge treats EOF as the front end having gone. */
  endInput(): void
  /** End input, wait up to `graceMs` for exit, then terminate. */
  dispose(graceMs?: number): Promise<Exit>
}

export function connectStdio(options: StdioOptions): StdioConnection {
  const child = spawn(options.command, options.args ?? [], {
    cwd: options.cwd,
    env: options.env,
    stdio: ['pipe', 'pipe', 'pipe'],
    windowsHide: true,
  })
  const client = new RpcAgentClient(
    {
      write(line) {
        if (!child.stdin.writable) {
          // Nothing more can reach the bridge, so the connection ends as it does for a failed flush.
          client.transportClosed('bravebot-rpc is not accepting input')
          throw new Error('the bridge is not accepting input')
        }
        child.stdin.write(line, (error) => {
          // A failed flush means the request never reached the bridge or its outcome is unknown.
          if (error) client.transportClosed(`writing to bravebot-rpc failed: ${error.message}`)
        })
      },
    },
    {
      onDiagnostic: options.onDiagnostic,
      workspaces: options.workspaces,
      // An unanswered request cannot be told from one that took effect, so the connection ends.
      onClosed: () => void stop(),
      deadlines: {
        ms: options.requestTimeoutMs ?? 30_000,
        schedule(ms, fire) {
          const timer = setTimeout(fire, ms)
          return () => clearTimeout(timer)
        },
      },
    },
  )
  let errors = ''
  child.stdout.setEncoding('utf8')
  child.stderr.setEncoding('utf8')
  child.stdout.on('data', (chunk: string) => client.receive(chunk))
  child.stdout.on('end', () => client.transportClosed('bravebot-rpc output ended'))
  child.stdout.on('error', (error) => client.transportClosed(`bravebot-rpc output failed: ${error.message}`))
  child.stderr.on('data', (chunk: string) => {
    errors = (errors + chunk).slice(-64 * 1024)
  })
  // Failed writes are reported through their own callback; this keeps the error event from throwing.
  child.stdin.on('error', () => undefined)

  const exited = new Promise<Exit>((resolve) => {
    let settled = false
    const finish = (exit: Exit, detail: string): void => {
      if (settled) return
      settled = true
      client.transportClosed(detail)
      resolve(exit)
    }
    child.on('error', (error) => finish({ code: null, signal: null }, `could not run bravebot-rpc: ${error.message}`))
    child.on('close', (code, signal) =>
      finish({ code, signal }, signal ? `bravebot-rpc was killed by ${signal}` : `bravebot-rpc exited with code ${code}`),
    )
  })

  const endInput = (): void => {
    if (!child.stdin.destroyed) child.stdin.end()
  }
  const stop = async (graceMs = 2000): Promise<Exit> => {
    endInput()
    const timer = setTimeout(() => child.kill('SIGKILL'), graceMs)
    try {
      return await exited
    } finally {
      clearTimeout(timer)
    }
  }
  return {
    client,
    child,
    stderr: () => errors,
    exited,
    endInput,
    dispose: stop,
  }
}
