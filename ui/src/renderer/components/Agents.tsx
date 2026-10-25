import { Modal } from './Modal'
import { Button, Icon } from '../nala'
import type { AgentEntry } from '../../shared/protocol'

const STATE: Record<AgentEntry['state'], string> = {
  working: 'Working',
  finished: 'Finished',
  failed: 'Did not finish',
}

/**
 * The agents this conversation has started, each with its kind, what it was asked and where it
 * stands (VIEW-36). What an agent printed is not here: the window is never sent it.
 */
export function Agents({ agents, onClose }: { agents: readonly AgentEntry[]; onClose: () => void }): React.JSX.Element {
  return <Modal title="Agents" size="md" onClose={onClose} className="agents-view"
    subtitle="Agents this conversation has started. Open a turn in the conversation to read what one did."
    actions={<Button kind="filled" onClick={onClose} data-test="agents-done">Done</Button>}>
    {agents.length === 0
      ? <p className="grant-empty"><Icon name="robot" />No agents have started in this conversation.</p>
      : <ul className="grant-list agent-list">{agents.map((agent, index) => <li key={index} className="grant-row" data-state={agent.state}>
        <Icon name="robot" />
        <div className="grant-text">
          <strong className="grant-path">{agent.kind} <span className="agent-id">{agent.id}</span></strong>
          <span className="grant-meta">{agent.task}</span>
        </div>
        <span className="agent-state" data-state={agent.state}>{STATE[agent.state]}</span>
      </li>)}</ul>}
  </Modal>
}
