import { useEffect, useState } from 'react'
import { Modal } from './Modal'
import type { KeptTrust, SettingsRules } from '../../shared/protocol'

interface Grants {
  paths: { path: string; integrity: string }[]
  commands: { program: string; startedAs: string; args: string[]; display: string }[]
  /** The yes kept about this directory for later sessions (TRUST-23), read when this was asked. */
  remembered?: KeptTrust | null
  /** The rules this conversation opened under. Absent from an older bridge. */
  settingsRules?: SettingsRules | null
}
export function Permissions({ session, onClose, onRemembered }: { session: string; onClose: () => void; onRemembered?: (kept: KeptTrust | null) => void }): React.JSX.Element {
  const [grants, setGrants] = useState<Grants | null>(null)
  const [problem, setProblem] = useState('')
  const [busy, setBusy] = useState(false)
  const request = async (method: string, params: Record<string, unknown> = {}) => {
    setBusy(true); setProblem('')
    try {
      const answer = await window.bravebot.request<Grants>(method, { session, ...params })
      if (answer.error) setProblem(answer.error.message)
      else if (answer.ok) {
        setGrants(answer.ok)
        if ('remembered' in answer.ok) onRemembered?.(answer.ok.remembered ?? null)
      }
    } catch { setProblem('Permissions could not be loaded. Try again.') }
    finally { setBusy(false) }
  }
  useEffect(() => { void request('permissions.list') }, [session])
  return <Modal title="Conversation permissions" onClose={onClose}>
    <h2>Conversation permissions</h2>
    <p>These grants are saved with this conversation. Revocation affects future actions; it cannot remove content already read by the model.</p>
    {problem && <p role="alert">{problem}</p>}
    <button disabled={busy} onClick={() => void request('permissions.list')}>{busy ? 'Loading…' : 'Refresh'}</button>
    <PathPermissions paths={grants?.paths ?? []} busy={busy} onRevoke={(path) => void request('permissions.revoke', { kind: 'path', path })} />
    <h3>Remembered commands</h3>
    <p className="bot-note">Each grant covers the resolved program and its exact arguments, including trust in its output.</p>
    {grants?.commands.map((command) => <div className="permission-row" key={JSON.stringify(command)}><code>{command.display}</code><button disabled={busy} onClick={() => void request('permissions.revoke', { kind: 'command', command: { program: command.program, startedAs: command.startedAs, args: command.args } })}>Revoke</button></div>)}
    {grants?.commands.length === 0 && <p>No remembered command grants.</p>}
    <h3>Remembered for this directory</h3>
    <p className="bot-note">Kept outside this conversation: sessions started in exactly this directory are trusted without asking. Forgetting it makes the next one ask; this conversation keeps its own grants.</p>
    {grants?.remembered && <div className="permission-row"><code>{grants.remembered.path}</code><button disabled={busy} onClick={() => void request('permissions.revoke', { kind: 'remembered' })}>Forget</button></div>}
    {grants && !grants.remembered && <p>No answer is remembered for this directory.</p>}
    <RulesInForce rules={grants?.settingsRules ?? null} />
    <button onClick={onClose}>Done</button>
  </Modal>
}

export function PathPermissions({ paths, busy, onRevoke }: { paths: Grants['paths']; busy: boolean; onRevoke: (path: string) => void }): React.JSX.Element {
  return <>
    <h3>Trusted paths</h3>
    <p className="bot-note">A parent grant covers its descendants unless a more specific rule overrides it. Revoking a parent keeps any separately listed child grants.</p>
    {paths.filter((grant) => grant.integrity === 'trusted').map((grant) => <div className="permission-row" key={grant.path}><code>{grant.path || 'Project root'}</code><button disabled={busy} onClick={() => onRevoke(grant.path)}>Revoke</button></div>)}
    {!paths.some((grant) => grant.integrity === 'trusted') && <p>No trusted path grants.</p>}
    {paths.some((grant) => grant.integrity !== 'trusted' && grant.integrity !== 'undecided') && <><h3>Untrusted path exceptions</h3><p className="bot-note">These paths remain untrusted even when a parent is trusted.</p>{paths.filter((grant) => grant.integrity !== 'trusted' && grant.integrity !== 'undecided').map((grant) => <div className="permission-row" key={grant.path}><code>{grant.path || 'Project root'}</code><span>Untrusted</span></div>)}</>}
    {paths.some((grant) => grant.integrity === 'undecided') && <><h3>Paths awaiting a decision</h3><p className="bot-note">These paths require write approval and their contents remain untrusted until you grant trust.</p>{paths.filter((grant) => grant.integrity === 'undecided').map((grant) => <div className="permission-row" key={grant.path}><code>{grant.path || 'Project root'}</code><span>Not decided</span></div>)}</>}
  </>
}

/**
 * The permission rules a conversation opened under, read only.
 *
 * They come from settings files and were read when the conversation opened, so there is nothing
 * here to revoke. Editing the file changes them for the next conversation.
 */
export function RulesInForce({ rules }: { rules: SettingsRules | null }): React.JSX.Element {
  const lists: [string, string, string[]][] = [
    ['Refused', 'Refused before anything is asked or started.', rules?.deny ?? []],
    ['Always asked', 'You are asked, whatever else would have answered.', rules?.ask ?? []],
    ['Not asked', 'The question is answered for you. What a command prints is not trusted because of it.', rules?.allow ?? []],
  ]
  const none = lists.every(([, , held]) => held.length === 0)
  return <>
    <h3>Rules from settings files</h3>
    <p className="bot-note">Read when this conversation opened. Edit the settings file to change them; the change applies to the next conversation. Refused comes first, then always asked, then not asked. These rules are not applied to a plan run.</p>
    {none && <p>No permission rules are in force.</p>}
    {lists.filter(([, , held]) => held.length > 0).map(([name, meaning, held]) => <div className="settings-rules" key={name}>
      <h4>{name}</h4>
      <p className="bot-note">{meaning}</p>
      {held.map((rule, index) => <div className="permission-row" key={index}><code>{rule}</code></div>)}
    </div>)}
  </>
}
