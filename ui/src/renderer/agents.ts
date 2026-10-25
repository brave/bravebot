/**
 * What the window draws of the agents a conversation has started (VIEW-36).
 *
 * The list is the bridge's `agents` event, stored as it was sent. Nothing here decides when an
 * agent starts or ends, and nothing a delegate printed is kept: the bridge sends none of it (VIEW-35).
 */

import type { AgentEntry } from '../shared/protocol'

/** How many are in the list, and how many of those are working, as the sidebar shows them. */
export function tally(agents: readonly AgentEntry[]): { started: number; working: number } {
  return { started: agents.length, working: agents.filter((agent) => agent.state === 'working').length }
}
