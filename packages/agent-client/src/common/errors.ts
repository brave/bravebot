/** The failure the bridge itself reported for one request: a closed set of codes plus a message. */
export class RpcError extends Error {
  constructor(
    readonly code: string,
    message: string,
  ) {
    super(message)
    this.name = 'RpcError'
  }
}

/** The runtime lacks the session view this client requires, or advertises a version it does not know. */
export class CapabilityError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'CapabilityError'
  }
}

/** The peer sent something the wire contract does not allow. */
export class ProtocolError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'ProtocolError'
  }
}

/** The client refused an operation locally, without sending it. */
export class UnsupportedError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'UnsupportedError'
  }
}

/** The connection ended. Requests in flight fail with this; nothing is retried. */
export class ConnectionLostError extends RpcError {
  constructor(detail: string) {
    super('agent_gone', detail)
    this.name = 'ConnectionLostError'
  }
}
