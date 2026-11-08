import type { AgentClient } from '@brave/agent-client'

/** Where a session runs. The app depends on the client interface and never on which target it has. */
export interface Target {
  id: string
  label: string
  description: string
  suggestions: readonly string[]
  create: () => AgentClient
}

/** A target reached at run time, and a promise that resolves with the reason once it is gone. */
export interface Connection {
  target: Target
  ended: Promise<string>
}

/**
 * Reach a target at the port and code a person typed. It rejects with an Error whose message is
 * written for the person.
 */
export type Connect = (port: number, code: string) => Promise<Connection>
