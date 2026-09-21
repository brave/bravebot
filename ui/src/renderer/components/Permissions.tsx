import { useEffect, useState } from 'react'
import { Modal } from './Modal'

interface Grants {
  paths: { path: string; integrity: string }[]
  commands: { program: string; args: string[]; display: string }[]
}
export function Permissions({ session, onClose }: { session: string; onClose: () => void }): React.JSX.Element {
  const [grants, setGrants] = useState<Grants | null>(null)
  const [problem, setProblem] = useState('')
  const [busy, setBusy] = useState(false)
  const request = async (method: string, params: Record<string, unknown> = {}) => {
    setBusy(true); setProblem('')
    try {
      const answer = await window.bravebot.request<Grants>(method, { session, ...params })
      if (answer.error) setProblem(answer.error.message)
      else if (answer.ok) setGrants(answer.ok)
    } catch { setProblem('Permissions could not be loaded. Try again.') }
    finally { setBusy(false) }
  }
  useEffect(() => { void request('permissions.list') }, [session])
  return <Modal title="Conversation permissions" onClose={onClose}>
    <h2>Conversation permissions</h2>
    <p>These grants are saved with this conversation. Revocation affects future actions; it cannot remove content already read by the model.</p>
    {problem && <p role="alert">{problem}</p>}
    <button disabled={busy} onClick={() => void request('permissions.list')}>{busy ? 'Loading…' : 'Refresh'}</button>
    <h3>Trusted paths</h3>
    <p className="bot-note">A parent grant covers its descendants unless a more specific rule overrides it. Revoking a parent keeps any separately listed child grants.</p>
    {grants?.paths.filter((grant) => grant.integrity === 'trusted').map((grant) => <div className="permission-row" key={grant.path}><code>{grant.path || 'Project root'}</code><button disabled={busy} onClick={() => void request('permissions.revoke', { kind: 'path', path: grant.path })}>Revoke</button></div>)}
    {grants && !grants.paths.some((grant) => grant.integrity === 'trusted') && <p>No trusted path grants.</p>}
    {grants?.paths.some((grant) => grant.integrity !== 'trusted') && <><h3>Untrusted path exceptions</h3><p className="bot-note">These paths remain untrusted even when a parent is trusted.</p>{grants.paths.filter((grant) => grant.integrity !== 'trusted').map((grant) => <div className="permission-row" key={grant.path}><code>{grant.path || 'Project root'}</code><span>Untrusted</span></div>)}</>}
    <h3>Remembered commands</h3>
    <p className="bot-note">Each grant covers the resolved program and its exact arguments, including trust in its output.</p>
    {grants?.commands.map((command) => <div className="permission-row" key={JSON.stringify(command)}><code>{command.display}</code><button disabled={busy} onClick={() => void request('permissions.revoke', { kind: 'command', command: { program: command.program, args: command.args } })}>Revoke</button></div>)}
    {grants?.commands.length === 0 && <p>No remembered command grants.</p>}
    <button onClick={onClose}>Done</button>
  </Modal>
}
