// What a separate strict TypeScript project sees through the package's exports.
import { CapabilityError, RpcAgentClient, type AgentSession, type ViewState } from '@brave/agent-client'
// @ts-expect-error `RpcConnection` is not exported.
import type { RpcConnection } from '@brave/agent-client'
void (null as RpcConnection | null)
import { connectStdio } from '@brave/agent-client/node'

const connection = connectStdio({ command: 'bravebot-rpc', env: {}, workspaces: [{ id: 'w', name: 'W', directory: '/w' }] })
async function use(): Promise<ViewState> {
  const session: AgentSession = await connection.client.createSession({ workspace: 'w' })
  await session.cancel()
  return session.view
}
void use().catch((error: unknown) => error instanceof CapabilityError)

// Raw dispatch and the connection class are not part of what a caller is given.
// @ts-expect-error `client` is typed as `AgentClient`, which has no `raw`.
void connection.client.raw
// @ts-expect-error the stdio connection has no `raw`.
void connection.raw
const concrete = new RpcAgentClient({ write: () => undefined })
// @ts-expect-error `RpcAgentClient` has no `raw`.
void concrete.raw
// @ts-expect-error `connection` is private to the client.
void concrete.connection
