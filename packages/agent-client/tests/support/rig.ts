import assert from 'node:assert/strict'
import { existsSync, mkdirSync, mkdtempSync, realpathSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { connectStdio, type StdioConnection } from '../../src/node/index.js'
import type { AgentSession, ViewState } from '../../src/common/index.js'
import { ModelStub, type PlannerStep } from './model-stub.js'
import { within } from './wait.js'

const here = dirname(fileURLToPath(import.meta.url))

/** The built `bravebot-rpc`: BRAVEBOT_RPC, or the workspace's debug build. It is never built implicitly. */
export function rpcBinary(): string {
  const path = process.env.BRAVEBOT_RPC ?? join(here, `../../../../../target/debug/bravebot-rpc${process.platform === 'win32' ? '.exe' : ''}`)
  assert.ok(existsSync(path), `bravebot-rpc is not built at ${path}; run \`make check-agent-client\` or set BRAVEBOT_RPC`)
  return path
}

/**
 * The environment a test runs the bridge in: nothing inherited, a home of its own, and the model
 * service pointed at the test's stub, so no real credential, setting or session is read or written.
 */
export function isolatedEnvironment(home: string, endpoint: string): NodeJS.ProcessEnv {
  return {
    HOME: home,
    BRAVEBOT_LOCALE: 'en-US',
    SERVICES_KEY_AICHAT: 'a-services-key',
    BRAVE_SERVICES_KEY_ID: 'a-key-id',
    BRAVE_AI_CHAT_ENDPOINT: endpoint,
    BRAVE_AI_CHAT_PREMIUM_ENDPOINT: endpoint,
    BRAVEBOT_DEFAULT_MODEL: 'stub-model',
  }
}

export interface Rig {
  stub: ModelStub
  rpc: StdioConnection
  project: string
  home: string
  diagnostics: string[]
  stop(): Promise<void>
}

/** A real bridge process, a project directory and a home of its own, and a stub model with `plans`. */
export async function startRig(plans: Record<string, PlannerStep[]>): Promise<Rig> {
  const scratch = realpathSync(mkdtempSync(join(tmpdir(), 'agent-client-')))
  const project = join(scratch, 'project')
  const home = join(scratch, 'home')
  mkdirSync(project)
  mkdirSync(join(home, '.bravebot'), { recursive: true })
  const stub = await ModelStub.start(plans)
  const diagnostics: string[] = []
  const rpc = connectStdio({
    command: rpcBinary(),
    env: isolatedEnvironment(home, stub.endpoint),
    cwd: scratch,
    workspaces: [{ id: 'project', name: 'Project', directory: project }],
    onDiagnostic: (message) => diagnostics.push(message),
  })
  return {
    stub,
    rpc,
    project,
    home,
    diagnostics,
    async stop() {
      await rpc.dispose(1000)
      await stub.stop()
      rmSync(scratch, { recursive: true, force: true })
    },
  }
}

/** Resolve when the session's view satisfies `wanted`, as observed through its updates. */
export function until(session: AgentSession, what: string, wanted: (view: ViewState) => boolean, ms = 30_000): Promise<ViewState> {
  const now = session.view
  if (wanted(now)) return Promise.resolve(now)
  return within(
    new Promise<ViewState>((resolve) => {
      const stop = session.subscribe((view) => {
        if (wanted(view)) {
          stop()
          resolve(view)
        }
      })
    }),
    what,
    ms,
  )
}

/** Record every view the session passes through, so a test can check the sequence it saw. */
export function recording(session: AgentSession): ViewState[] {
  const seen = [session.view]
  session.subscribe((view) => seen.push(view))
  return seen
}
