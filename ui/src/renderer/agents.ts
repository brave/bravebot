/**
 * The agents a conversation has started, as the window keeps them (VIEW-36).
 *
 * Each is its number, its kind, what it was asked and where it stands. Nothing a delegate printed
 * is kept: the bridge sends none of it (VIEW-35), and this holds only what it is sent.
 */

export type AgentState = 'working' | 'finished' | 'failed'

export interface Agent {
  /** The driver's number for it, such as `d1` or `d2.1`. */
  id: string
  kind: string
  task: string
  state: AgentState
}

/** A delegate beginning. Added whether or not the number was seen before: see `finish`. */
export function started(agents: Agent[], delegate: string, kind: string, task: string): Agent[] {
  return [...agents, { id: delegate, kind, task, state: 'working' }]
}

/**
 * A delegate ending, matched by its number. Numbers count from one in every turn, so the same
 * number can name an earlier agent that has ended: the one still working is the one that ended, and
 * the latest of those where more than one is.
 */
export function finish(agents: Agent[], delegate: string, failed: boolean): Agent[] {
  let at = -1
  agents.forEach((agent, index) => { if (agent.id === delegate && agent.state === 'working') at = index })
  if (at < 0) return agents
  return agents.map((agent, index) => (index === at ? { ...agent, state: failed ? 'failed' : 'finished' } : agent))
}

/** The turn is over, and a delegate does not outlive it (DELEGATE-17): one still working did not finish. */
export function turnEnded(agents: Agent[]): Agent[] {
  if (!agents.some((agent) => agent.state === 'working')) return agents
  return agents.map((agent) => (agent.state === 'working' ? { ...agent, state: 'failed' } : agent))
}

/** How many have been started, and how many of those are still working. */
export function tally(agents: readonly Agent[]): { started: number; working: number } {
  return { started: agents.length, working: agents.filter((agent) => agent.state === 'working').length }
}
