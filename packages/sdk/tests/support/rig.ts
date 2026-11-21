import assert from 'node:assert/strict'
import { existsSync, mkdirSync, mkdtempSync, realpathSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import type { CommonOptions } from '../../src/index.js'
import { ModelStub, type PlannerStep } from './model-stub.js'

const here = dirname(fileURLToPath(import.meta.url))

/** The built `bravebot`: BRAVEBOT_BIN, or the workspace's debug build. It is never built implicitly. */
export function binary(): string {
  const path = process.env.BRAVEBOT_BIN ?? join(here, `../../../../../target/debug/bravebot${process.platform === 'win32' ? '.exe' : ''}`)
  assert.ok(existsSync(path), `bravebot is not built at ${path}; run \`make check-sdk\` or set BRAVEBOT_BIN`)
  return path
}

export interface Rig {
  stub: ModelStub
  /** The directory the process runs in. */
  project: string
  home: string
  /** Options that point the library at the real binary, this project, and the stub model. */
  options: CommonOptions
  stop(): Promise<void>
}

/**
 * The environment a test runs the command in: nothing inherited, a home of its own, and the model
 * service pointed at the test's stub, so no real credential, setting or session is read or written.
 */
function environment(home: string, endpoint: string): NodeJS.ProcessEnv {
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

/** The real binary, an empty project directory with nothing trusted, a home of its own and a stub model with `plans`. */
export async function startRig(plans: Record<string, PlannerStep[]>, stderr: string[] = []): Promise<Rig> {
  const scratch = realpathSync(mkdtempSync(join(tmpdir(), 'sdk-')))
  const project = join(scratch, 'project')
  const home = join(scratch, 'home')
  mkdirSync(project)
  mkdirSync(join(home, '.bravebot'), { recursive: true })
  const stub = await ModelStub.start(plans)
  return {
    stub,
    project,
    home,
    options: { command: binary(), cwd: project, env: environment(home, stub.endpoint), onStderr: (text) => stderr.push(text) },
    async stop() {
      await stub.stop()
      rmSync(scratch, { recursive: true, force: true })
    },
  }
}
