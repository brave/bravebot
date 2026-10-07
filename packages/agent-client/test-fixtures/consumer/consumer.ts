// What a separate strict TypeScript project sees through the package's exports.
import { CapabilityError, type AgentSession, type ViewState } from '@brave/agent-client'
import { connectStdio } from '@brave/agent-client/node'

const connection = connectStdio({ command: 'bravebot-rpc', env: {}, workspaces: [{ id: 'w', name: 'W', directory: '/w' }] })
async function use(): Promise<ViewState> {
  const session: AgentSession = await connection.client.createSession({ workspace: 'w' })
  await session.cancel()
  return session.view
}
void use().catch((error: unknown) => error instanceof CapabilityError)

// Raw dispatch is on the connection for diagnostics, not on the client a caller is given.
// @ts-expect-error `client` is typed as `AgentClient`, which has no `raw`.
void connection.client.raw
void connection.raw
