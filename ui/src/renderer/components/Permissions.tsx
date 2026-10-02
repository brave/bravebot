import { useEffect, useState } from 'react'
import { Modal } from './Modal'
import type { KeptTrust, SettingsRules } from '../../shared/protocol'
import { Alert, Button, Icon } from '../nala'
import { IconButton } from './IconButton'

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
  return <Modal title="Conversation permissions" size="md" onClose={onClose}
    subtitle="Grants saved with this conversation. Revoking one affects future actions only."
    headerAction={<IconButton icon="refresh" label="Refresh" tooltip={busy ? 'Loading…' : 'Refresh permissions'} disabled={busy}
      onClick={() => void request('permissions.list')} data-test="permissions-refresh" />}
    actions={<Button kind="filled" onClick={onClose} data-test="permissions-done">Done</Button>}>
    <p className="grant-lede">Revocation cannot remove content already read by the model.</p>
    {problem && <Alert type="error" data-test="permissions-error">{problem}</Alert>}
    <PathPermissions paths={grants?.paths ?? []} busy={busy} onRevoke={(path) => void request('permissions.revoke', { kind: 'path', path })} />
    <section className="grant-section">
      <h3>Remembered commands</h3>
      <p className="grant-note">Each grant covers the resolved program and its exact arguments, including trust in its output.</p>
      {grants && grants.commands.length > 0 && <ul className="grant-list">{grants.commands.map((command) => <li className="grant-row" key={JSON.stringify(command)}>
        <Icon name="window-console" /><code>{command.display}</code>
        <Button size="small" kind="plain-faint" isDisabled={busy} onClick={() => void request('permissions.revoke', { kind: 'command', command: { program: command.program, startedAs: command.startedAs, args: command.args } })}>Revoke</Button>
      </li>)}</ul>}
      {grants?.commands.length === 0 && <p className="grant-empty"><Icon name="window-console" />No remembered command grants.</p>}
    </section>
    <section className="grant-section">
      <h3>Remembered for this directory</h3>
      <p className="grant-note">Kept outside this conversation: sessions started in exactly this directory are trusted without asking. Forgetting it makes the next one ask; this conversation keeps its own grants.</p>
      {grants?.remembered && <ul className="grant-list"><li className="grant-row"><Icon name="pin" /><code>{grants.remembered.path}</code>
        <Button size="small" kind="plain-faint" isDisabled={busy} onClick={() => void request('permissions.revoke', { kind: 'remembered' })}>Forget</Button></li></ul>}
      {grants && !grants.remembered && <p className="grant-empty"><Icon name="pin" />No answer is remembered for this directory.</p>}
    </section>
    <RulesInForce rules={grants?.settingsRules ?? null} />
  </Modal>
}

export function PathPermissions({ paths, busy, onRevoke }: { paths: Grants['paths']; busy: boolean; onRevoke: (path: string) => void }): React.JSX.Element {
  const trusted = paths.filter((grant) => grant.integrity === 'trusted')
  const refused = paths.filter((grant) => grant.integrity !== 'trusted' && grant.integrity !== 'undecided')
  const undecided = paths.filter((grant) => grant.integrity === 'undecided')
  return <>
    <section className="grant-section">
      <h3>Trusted paths</h3>
      <p className="grant-note">A parent grant covers its descendants unless a more specific rule overrides it. Revoking a parent keeps any separately listed child grants.</p>
      {trusted.length > 0 && <ul className="grant-list">{trusted.map((grant) => <li className="grant-row" key={grant.path}>
        <Icon name="shield-done" /><code>{grant.path || 'Project root'}</code>
        <Button size="small" kind="plain-faint" isDisabled={busy} onClick={() => onRevoke(grant.path)}>Revoke</Button>
      </li>)}</ul>}
      {trusted.length === 0 && <p className="grant-empty"><Icon name="shield-done" />No trusted path grants.</p>}
    </section>
    {refused.length > 0 && <section className="grant-section"><h3>Untrusted path exceptions</h3><p className="grant-note">These paths remain untrusted even when a parent is trusted.</p>
      <ul className="grant-list">{refused.map((grant) => <li className="grant-row" key={grant.path}><Icon name="warning-triangle-outline" /><code>{grant.path || 'Project root'}</code><span className="grant-state">Untrusted</span></li>)}</ul></section>}
    {undecided.length > 0 && <section className="grant-section"><h3>Paths awaiting a decision</h3><p className="grant-note">These paths require write approval and their contents remain untrusted until you grant trust.</p>
      <ul className="grant-list">{undecided.map((grant) => <li className="grant-row" key={grant.path}><Icon name="radio-unchecked" /><code>{grant.path || 'Project root'}</code><span className="grant-state">Not decided</span></li>)}</ul></section>}
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
  return <section className="grant-section settings-rules">
    <h3>Rules from settings files</h3>
    <p className="grant-note">Read when this conversation opened. Edit the settings file to change them; the change applies to the next conversation. Refused comes first, then always asked, then not asked. These rules are not applied to a plan run.</p>
    {none && <p className="grant-empty"><Icon name="settings" />No permission rules are in force.</p>}
    {lists.filter(([, , held]) => held.length > 0).map(([name, meaning, held]) => <div key={name}>
      <h4>{name}</h4>
      <p className="grant-note">{meaning}</p>
      <ul className="grant-list">{held.map((rule, index) => <li className="grant-row" key={index}><code>{rule}</code></li>)}</ul>
    </div>)}
  </section>
}
