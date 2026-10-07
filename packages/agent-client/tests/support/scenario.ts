import assert from 'node:assert/strict'
import { readdirSync, readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { rawRequest } from '../../src/common/client.js'
import { RpcAgentClient, type AgentSession } from '../../src/common/index.js'
import { subsetMismatch, within } from './wait.js'

/** One step of a language-independent scenario. See test-fixtures/README.md. */
export type Step = Record<string, unknown>

export interface Scenario {
  name: string
  description: string
  steps: Step[]
}

export type Chunking = 'batch' | 'line' | 'character' | 'odd'
export const CHUNKINGS: Chunking[] = ['batch', 'line', 'character', 'odd']

const here = dirname(fileURLToPath(import.meta.url))
export const FIXTURES = join(here, '../../../test-fixtures')

export function loadScenarios(): Scenario[] {
  const dir = join(FIXTURES, 'scenarios')
  return readdirSync(dir)
    .filter((name) => name.endsWith('.json'))
    .sort()
    .map((name) => JSON.parse(readFileSync(join(dir, name), 'utf8')) as Scenario)
}

function split(text: string, chunking: Chunking): string[] {
  switch (chunking) {
    case 'batch':
      return [text]
    case 'line':
      return text.split(/(?<=\n)/)
    case 'character':
      return [...text]
    case 'odd': {
      const chunks: string[] = []
      for (let at = 0; at < text.length; at += 7) chunks.push(text.slice(at, at + 7))
      return chunks
    }
  }
}

/** Replace `"$name"` strings with the id of the request bound to that name. */
function resolve(value: unknown, ids: Map<string, number>): unknown {
  if (typeof value === 'string' && value.startsWith('$')) {
    const id = ids.get(value.slice(1))
    assert.ok(id !== undefined, `scenario refers to unbound request ${value}`)
    return id
  }
  if (Array.isArray(value)) return value.map((item) => resolve(item, ids))
  if (typeof value === 'object' && value !== null) {
    return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, resolve(item, ids)]))
  }
  return value
}

/**
 * Play a scenario against the client with a scripted server. The client runs as it would over a
 * stream: server output reaches it in chunks the scenario does not control, so every scenario is
 * run under each chunking and must give the same result.
 */
export async function runScenario(scenario: Scenario, chunking: Chunking): Promise<void> {
  const received: { id: number; method: string; params: unknown }[] = []
  let wake: (() => void) | null = null
  const client = new RpcAgentClient(
    {
      write(line) {
        received.push(JSON.parse(line) as { id: number; method: string; params: unknown })
        wake?.()
      },
    },
    { workspaces: ['work', 'one', 'two'].map((id) => ({ id, name: id[0]!.toUpperCase() + id.slice(1), directory: `/${id}` })) },
  )
  const ids = new Map<string, number>()
  const running = new Map<string, Promise<unknown>>()
  const sessions = new Map<string, AgentSession>()

  const nextRequest = async (): Promise<{ id: number; method: string; params: unknown }> => {
    while (received.length === 0) {
      await within(new Promise<void>((resolveWake) => (wake = resolveWake)), 'the client to send a request', 5000)
    }
    return received.shift()!
  }
  const session = (name: unknown): AgentSession => {
    const found = sessions.get(String(name))
    assert.ok(found, `scenario refers to unbound session ${String(name)}`)
    return found
  }

  type Args = Record<string, any>
  const calls: Record<string, (args: Args, target: () => AgentSession) => Promise<unknown>> = {
    describe: () => client.describe(),
    createSession: (args) => client.createSession({ workspace: args.workspace }),
    unsupported: (args) => client.unsupported(args.operation),
    raw: (args) => rawRequest(client, args.method, args.params),
    answerTrust: (args, target) => target().answerTrust(args.trusted),
    send: (args, target) => target().send(args.text),
    cancel: (_args, target) => target().cancel(),
    close: (_args, target) => target().close(),
  }

  for (const [at, step] of scenario.steps.entries()) {
    const where = `${scenario.name} [${chunking}] step ${at}`
    try {
      if ('start' in step) {
        const args = (step.args ?? {}) as Args
        const call = calls[String(step.call)]
        assert.ok(call, `unknown call ${String(step.call)}`)
        const operation = call(args, () => session(step.session))
        // Observed by the await step; kept from reporting as unhandled until then.
        operation.catch(() => undefined)
        running.set(String(step.start), operation)
      } else if ('expectRequest' in step) {
        const request = await nextRequest()
        assert.equal(request.method, step.method, `${where}: the next request`)
        assert.deepEqual(request.params, step.params ?? {}, `${where}: params of ${request.method}`)
        ids.set(String(step.expectRequest), request.id)
      } else if ('expectWorkspaces' in step) {
        assert.deepEqual(client.workspaces(), step.expectWorkspaces, `${where}: workspaces`)
      } else if ('expectNoRequest' in step) {
        await new Promise((done) => setTimeout(done, 20))
        assert.deepEqual(received, [], `${where}: the client sent a request it should have refused`)
      } else if ('send' in step) {
        const lines = (step.send as unknown[]).map((line) =>
          typeof line === 'string' ? line : JSON.stringify(resolve(line, ids)),
        )
        for (const chunk of split(lines.join('\n') + '\n', chunking)) client.receive(chunk)
      } else if ('await' in step) {
        const operation = running.get(String(step.await))
        assert.ok(operation, `${where}: nothing started as ${String(step.await)}`)
        let outcome: { value: unknown } | { error: Error }
        try {
          outcome = { value: await within(operation, `${String(step.await)} to settle`, 5000) }
        } catch (error) {
          outcome = { error: error as Error }
        }
        if ('error' in step) {
          if (!('error' in outcome)) assert.fail(`${where}: expected ${JSON.stringify(step.error)}, the call succeeded`)
          const expected = step.error as { name: string; code?: string }
          assert.equal(outcome.error.name, expected.name, `${where}: ${outcome.error.message}`)
          if (expected.code) assert.equal((outcome.error as { code?: string }).code, expected.code)
        } else {
          if ('error' in outcome) assert.fail(`${where}: the call failed: ${outcome.error.message}`)
          if (step.bindSession) sessions.set(String(step.bindSession), outcome.value as AgentSession)
          if (step.result !== undefined) {
            assert.equal(subsetMismatch(outcome.value, step.result), null, `${where}: result`)
          }
        }
      } else if ('expectView' in step) {
        const { expectView, ...expected } = step
        const mismatch = subsetMismatch(session(expectView).view, expected)
        assert.equal(mismatch, null, `${where}: view of ${String(expectView)}`)
      } else if ('expectStartupTrust' in step) {
        assert.deepEqual(session(step.expectStartupTrust).startupTrust, step.value, `${where}: startup trust`)
      } else if ('closeTransport' in step) {
        client.transportClosed(String(step.closeTransport))
      } else {
        assert.fail(`${where}: unknown step ${JSON.stringify(Object.keys(step))}`)
      }
    } catch (error) {
      if (error instanceof Error && !error.message.includes(scenario.name)) {
        error.message = `${where}: ${error.message}`
      }
      throw error
    }
  }
}
