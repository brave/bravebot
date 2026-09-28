import { Modal } from './Modal'
import { Button, Collapse } from '../nala'

export function Unconfigured({ detail, onClose }: { detail: string; onClose: () => void }): React.JSX.Element {
  return <Modal title="Backend setup" onClose={onClose}>
    <h2>Connect the agent backend</h2>
    <p>This build cannot load its backend credentials. You can continue browsing conversations and writing drafts.</p>
    <p>If you installed Brave Bot, obtain a configured build from its distributor. Credentials are currently provided when the agent is built.</p>
    <Collapse title="Development setup" isOpen={undefined} data-test="unconfigured-setup">
      <ol>
        <li>Provide the backend credentials through the project’s approved environment configuration.</li>
        <li>Run <code>npm run bridge</code> from the interface checkout.</li>
        <li>Restart Brave Bot, then use <strong>Check again</strong>.</li>
      </ol>
      <p>See <code>docs/setup.md</code> for the configuration layout. Keep credentials outside the repository.</p>
    </Collapse>
    <Collapse title="Technical details" isOpen={undefined} data-test="unconfigured-details">
      <pre>{detail}</pre>
    </Collapse>
    <Button size="small" kind="filled" onClick={onClose} data-test="unconfigured-continue">Continue browsing</Button>
  </Modal>
}
