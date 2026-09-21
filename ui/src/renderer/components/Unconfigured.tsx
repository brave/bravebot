import { Modal } from './Modal'
export function Unconfigured({ detail, onClose }: { detail: string; onClose: () => void }): React.JSX.Element {
  return <Modal title="Backend setup" onClose={onClose}>
    <h2>Connect the agent backend</h2>
    <p>This build cannot load its backend credentials. You can continue browsing conversations and writing drafts.</p>
    <p>If you installed Brave Bot, obtain a configured build from its distributor. Credentials are currently provided when the agent is built.</p>
    <details><summary>Development setup</summary><ol>
      <li>Provide the backend credentials through the project’s approved environment configuration.</li>
      <li>Run <code>npm run bridge</code> from the interface checkout.</li>
      <li>Restart Brave Bot, then use <strong>Check again</strong>.</li>
    </ol><p>See <code>docs/setup.md</code> for the configuration layout. Keep credentials outside the repository.</p></details>
    <details><summary>Technical details</summary><pre>{detail}</pre></details>
    <button onClick={onClose}>Continue browsing</button>
  </Modal>
}
