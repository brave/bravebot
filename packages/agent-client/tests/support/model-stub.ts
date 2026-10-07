import { createServer, type IncomingMessage, type Server, type ServerResponse } from 'node:http'
import type { AddressInfo } from 'node:net'
import { within } from './wait.js'

/** What the planner does at one step: call a tool or answer, optionally after a test-controlled gate. */
export interface PlannerStep {
  tool?: { name: string; arguments: Record<string, unknown> }
  say?: string
  /** The reply is withheld until `release(hold)` is called. */
  hold?: string
}

/**
 * A model service of the test's own.
 *
 * Which plan a conversation follows is read from the plan key its latest user prompt carries, and
 * its step from how many tool results follow that prompt, so the service keeps no per-session
 * state. Every request body is kept so a test can look for what the planner was sent.
 */
export class ModelStub {
  readonly requests: string[] = []
  /** Requests that carried no plan key, such as a processor's. */
  readonly unmatched: string[] = []
  private readonly gates = new Map<string, { released: Promise<void>; release: () => void; reached: Promise<void>; arrive: () => void }>()
  private constructor(
    private readonly server: Server,
    readonly endpoint: string,
    private readonly plans: Record<string, PlannerStep[]>,
  ) {}

  static async start(plans: Record<string, PlannerStep[]>): Promise<ModelStub> {
    const holder: { stub?: ModelStub } = {}
    const server = createServer((request, response) => void holder.stub!.serve(request, response))
    await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve))
    const endpoint = `http://127.0.0.1:${(server.address() as AddressInfo).port}`
    holder.stub = new ModelStub(server, endpoint, plans)
    return holder.stub
  }

  private gate(name: string) {
    let gate = this.gates.get(name)
    if (!gate) {
      let release!: () => void
      let arrive!: () => void
      const released = new Promise<void>((resolve) => (release = resolve))
      const reached = new Promise<void>((resolve) => (arrive = resolve))
      gate = { released, release, reached, arrive }
      this.gates.set(name, gate)
    }
    return gate
  }

  /** Resolves once a request is waiting at the named gate. */
  reached(name: string): Promise<void> {
    return within(this.gate(name).reached, `the model request at gate ${name}`)
  }

  release(name: string): void {
    this.gate(name).release()
  }

  async stop(): Promise<void> {
    for (const gate of this.gates.values()) gate.release()
    this.server.closeAllConnections()
    await new Promise<void>((resolve) => this.server.close(() => resolve()))
  }

  private async serve(request: IncomingMessage, response: ServerResponse): Promise<void> {
    const chunks: Buffer[] = []
    for await (const chunk of request) chunks.push(chunk as Buffer)
    const body = Buffer.concat(chunks).toString('utf8')
    if (request.method === 'GET') {
      const models = [
        {
          key: 'stub-model',
          display_name: 'Stub',
          capabilities: ['tools'],
          options: { access: 'basic_and_premium', long_conversation_warning_character_limit: 400_000 },
        },
      ]
      response.writeHead(200, { 'Content-Type': 'application/json' }).end(JSON.stringify(models))
      return
    }
    this.requests.push(body)
    const conversation = JSON.parse(body) as { messages: { role: string; content: unknown }[] }
    // The latest prompt picks the plan, and the tool results since it pick the step.
    const lastPrompt = conversation.messages.map((message) => message.role).lastIndexOf('user')
    const prompt = JSON.stringify(conversation.messages[lastPrompt]?.content ?? '')
    const key = Object.keys(this.plans).find((candidate) => prompt.includes(candidate))
    if (key === undefined) this.unmatched.push(body)
    const results = conversation.messages.slice(lastPrompt + 1).filter((message) => message.role === 'tool').length
    const step = (key === undefined ? undefined : this.plans[key]?.[results]) ?? { say: 'done' }
    if (step.hold) {
      const gate = this.gate(step.hold)
      gate.arrive()
      await gate.released
    }
    const delta = step.tool
      ? {
          role: 'assistant',
          tool_calls: [
            {
              index: 0,
              id: `call_${results}`,
              type: 'function',
              function: { name: step.tool.name, arguments: JSON.stringify(step.tool.arguments) },
            },
          ],
        }
      : { role: 'assistant', content: step.say ?? 'done' }
    const chunk = {
      id: 'c1',
      object: 'chat.completion.chunk',
      model: 'stub-model',
      choices: [{ index: 0, delta, finish_reason: step.tool ? 'tool_calls' : 'stop' }],
      usage: { prompt_tokens: 10, completion_tokens: 1 },
    }
    response
      .writeHead(200, { 'Content-Type': 'text/event-stream' })
      .end(`data: ${JSON.stringify(chunk)}\n\ndata: [DONE]\n\n`)
  }
}
/** A website that answers every request with one page and records the request lines it saw. */
export async function startWebsite(page: string): Promise<{ origin: string; requests: string[]; stop(): Promise<void> }> {
  const requests: string[] = []
  const server = createServer((request, response) => {
    requests.push(`${request.method} ${request.url}`)
    response.writeHead(200, { 'Content-Type': 'text/plain' }).end(page)
  })
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve))
  return {
    origin: `http://127.0.0.1:${(server.address() as AddressInfo).port}`,
    requests,
    async stop() {
      server.closeAllConnections()
      await new Promise<void>((resolve) => server.close(() => resolve()))
    },
  }
}
