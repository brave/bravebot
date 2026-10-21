import type { AgentClient } from '@brave/agent-client'

/** Where a session runs. The app depends on the client interface and never on which target it has. */
export interface Target {
  id: string
  label: string
  description: string
  suggestions: readonly string[]
  create: () => AgentClient
}
