import { rawRequest, type RpcAgentClient } from '../../src/common/client.js'
import type { StdioConnection } from '../../src/node/index.js'

/** Send any bridge method through a stdio connection. Tests only: the package does not export this. */
export function rawOn(connection: StdioConnection, method: string, params: Record<string, unknown> = {}): Promise<unknown> {
  return rawRequest(connection.client as RpcAgentClient, method, params)
}
