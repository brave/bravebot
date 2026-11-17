import { useEffect, useState } from 'react'
import { Modal } from './Modal'
import type { FilesystemKey, ForgottenMcp, KeptTrust, RememberedMcp, SettingsRules } from '../../shared/protocol'
import { Alert, Button, Icon } from '../nala'
import { t } from '../i18n'
import { IconButton } from './IconButton'
import { ago } from './Sessions'
import { middleTruncate } from '../truncate'

/** How many characters of a path fit a grant row before the middle gives way. */
export const GRANT_PATH_WIDTH = 40

/** A path in a grant row: shortened in the middle when it would wrap. The full spelling is the accessible name and the tooltip, on a control the keyboard can reach. */
function GrantName({ name }: { name: string }): React.JSX.Element {
  const cut = middleTruncate(name, GRANT_PATH_WIDTH)
  const full = cut === name ? undefined : name
  return <code tabIndex={full ? 0 : undefined} aria-label={full} data-tooltip={full}>{cut}</code>
}

interface Grants {
  /** `shownPath` is `path` spelled so a blank or whitespace-ended name is visible. */
  paths: { path: string; shownPath: string; integrity: string }[]
  commands: { program: string; startedAs: string; args: string[]; display: string }[]
  /** The yes kept about this directory for later sessions (TRUST-23), read when this was asked. */
  remembered?: KeptTrust | null
  /** The rules this conversation opened under. Absent from an older bridge. */
  settingsRules?: SettingsRules | null
  /** The project's standing MCP answers, read when this was asked; null with no state directory, absent from an older bridge. */
  mcpRemembered?: RememberedMcp | null
  /** What a revoke of kind `mcp` dropped, on that answer only. */
  forgotten?: ForgottenMcp
}
/**
 * The Forget refusal still shown once `params` is asked for: a list or another revoke keeps it, so
 * a refresh does not wipe what the person was told, and only the next Forget clears it. Closing the
 * dialog drops it with the rest of the dialog's state.
 */
export const mcpProblemAsked = (shown: string, params: Record<string, unknown>): string => params.kind === 'mcp' ? '' : shown

export function Permissions({ session, onClose, onRemembered }: { session: string; onClose: () => void; onRemembered?: (kept: KeptTrust | null) => void }): React.JSX.Element {
  const [grants, setGrants] = useState<Grants | null>(null)
  const [problem, setProblem] = useState('')
  // A refusal of Forget for the MCP answers is said in their section, beside the button pressed.
  const [mcpProblem, setMcpProblem] = useState('')
  const [busy, setBusy] = useState(false)
  const request = async (method: string, params: Record<string, unknown> = {}) => {
    const mcp = params.kind === 'mcp'
    const refused = mcp ? setMcpProblem : setProblem
    setBusy(true); setProblem(''); setMcpProblem((shown) => mcpProblemAsked(shown, params))
    try {
      const answer = await window.bravebot.request<Grants>(method, { session, ...params })
      if (answer.error) refused(answer.error.message)
      else if (answer.ok) {
        setGrants(answer.ok)
        if ('remembered' in answer.ok) onRemembered?.(answer.ok.remembered ?? null)
      }
    } catch { refused(mcp ? t.uiPermissionsMcpFailed() : 'Permissions could not be loaded. Try again.') }
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
        <Icon name="window-console" /><GrantName name={command.display} />
        <Button size="small" kind="plain-faint" isDisabled={busy} onClick={() => void request('permissions.revoke', { kind: 'command', command: { program: command.program, startedAs: command.startedAs, args: command.args } })}>Revoke</Button>
      </li>)}</ul>}
      {grants?.commands.length === 0 && <p className="grant-empty"><Icon name="window-console" />No remembered command grants.</p>}
    </section>
    <section className="grant-section">
      <h3>Trusted for this directory</h3>
      <p className="grant-note">Kept outside this conversation: sessions started in this directory, or below it where it is the root of a git repository, are trusted without asking. Forgetting it makes the next one ask; this conversation keeps its own grants.</p>
      {grants?.remembered && <RememberedHere kept={grants.remembered} busy={busy} onForget={() => void request('permissions.revoke', { kind: 'remembered' })} />}
      {grants && !grants.remembered && <p className="grant-empty"><Icon name="pin" />Nothing is kept for this directory.</p>}
    </section>
    {grants && grants.mcpRemembered !== undefined && <RememberedMcpAnswers remembered={grants.mcpRemembered} forgotten={grants.forgotten ?? null}
      problem={mcpProblem} busy={busy} onForget={() => void request('permissions.revoke', { kind: 'mcp' })} />}
    <RulesInForce rules={grants?.settingsRules ?? null} />
  </Modal>
}

/**
 * The yes kept about this directory: when the person trusted it, the file it is kept in, and the
 * button that takes it back (TRUST-24).
 */
export function RememberedHere({ kept, busy, onForget }: { kept: KeptTrust; busy: boolean; onForget: () => void }): React.JSX.Element {
  const root = kept.shownRoot ? middleTruncate(kept.shownRoot, GRANT_PATH_WIDTH) : null
  const rootFull = root && kept.shownRoot && root !== kept.shownRoot ? kept.shownRoot : undefined
  return <ul className="grant-list"><li className="grant-row">
    <Icon name="pin" />
    <div className="grant-text">
      <GrantName name={kept.shownPath} />
      <span className="grant-meta">
        <span data-test="remembered-when">{`Trusted ${ago(kept.at)}`}</span>
        {root && <span data-test="remembered-root" tabIndex={rootFull ? 0 : undefined} aria-label={rootFull && `for ${rootFull}`} data-tooltip={rootFull}>{` · for ${root}`}</span>}
      </span>
    </div>
    <Button size="small" kind="plain-faint" isDisabled={busy} onClick={onForget}>Forget</Button>
  </li></ul>
}

/**
 * The project's standing MCP answers: answer 2 at a server's question, which starts every server
 * the project requests, and at a tool call's, which calls that tool without asking. They belong to
 * every conversation started in the project, and Forget drops them all, as `bravebot mcp forget`
 * does in a terminal. Whether that can be done is the bridge's `forgettable`; a refusal is said
 * as the bridge worded it.
 */
export function RememberedMcpAnswers({ remembered, forgotten, problem, busy, onForget }: {
  remembered: RememberedMcp | null
  forgotten: ForgottenMcp | null
  problem: string
  busy: boolean
  onForget: () => void
}): React.JSX.Element {
  const tools = remembered?.tools ?? []
  const any = Boolean(remembered?.everyServer) || tools.length > 0
  // Not forgettable with a record that can be read is an incognito session, which writes nothing.
  const incognito = remembered !== null && !remembered.forgettable && !remembered.unreadable
  const open = Boolean(remembered?.forgettable) && any && !busy
  return <section className="grant-section" data-test="mcp-remembered">
    <h3>{t.uiPermissionsMcpHeading()}</h3>
    {remembered && <p className="grant-note">{t.uiPermissionsMcpNote({ project: remembered.shownProject })}</p>}
    {problem && <Alert type="error" data-test="mcp-forget-error">{problem}</Alert>}
    {forgotten && <Alert type="success" data-test="mcp-forgotten">
      {forgotten.everyServer || forgotten.tools.length > 0
        ? <>{t.uiPermissionsMcpForgotten()}<ul className="grant-forgotten">
          {forgotten.everyServer && <li>{t.uiPermissionsMcpEveryServer()}</li>}
          {forgotten.tools.map((tool) => <li key={tool.tool}><code>{tool.shownTool}</code></li>)}
        </ul>{t.uiPermissionsMcpAfter()}</>
        : t.uiPermissionsMcpForgotNothing()}
    </Alert>}
    {remembered?.unreadable && <Alert type="warning" data-test="mcp-unreadable">
      {t.uiPermissionsMcpUnreadable({ path: remembered.unreadable.shownPath, reason: remembered.unreadable.reason })}
    </Alert>}
    {any && <ul className="grant-list">
      {remembered?.everyServer && <li className="grant-row" data-test="mcp-every-server">
        <Icon name="plug" />
        <div className="grant-text">
          <span className="grant-title">{t.uiPermissionsMcpEveryServer()}</span>
          <span className="grant-meta">{t.uiPermissionsMcpEveryServerMeta()}</span>
        </div>
      </li>}
      {tools.map((tool) => <li className="grant-row" key={tool.tool} data-test="mcp-tool">
        <Icon name="plug" />
        <div className="grant-text">
          <GrantName name={tool.shownTool} />
          <span className="grant-meta">{t.uiPermissionsMcpToolMeta()}</span>
        </div>
      </li>)}
    </ul>}
    {remembered === null && <p className="grant-empty"><Icon name="plug" />{t.uiPermissionsMcpNoState()}</p>}
    {remembered && !remembered.unreadable && !any && <p className="grant-empty" data-test="mcp-empty"><Icon name="plug" />{t.uiPermissionsMcpEmpty()}</p>}
    {remembered && !remembered.unreadable && <div className="grant-actions">
      <Button size="small" kind="outline" isDisabled={!open} aria-disabled={!open} onClick={onForget} data-test="mcp-forget"
        data-tooltip={incognito ? t.uiPermissionsMcpIncognito() : undefined}>{t.uiPermissionsMcpForget()}</Button>
    </div>}
  </section>
}

/** The root is the empty name, which a person is shown in words; any other rule is drawn as the agent spelled it. */
const named = (grant: Grants['paths'][number]): string => grant.path === '' ? 'Project root' : grant.shownPath

export function PathPermissions({ paths, busy, onRevoke }: { paths: Grants['paths']; busy: boolean; onRevoke: (path: string) => void }): React.JSX.Element {
  const trusted = paths.filter((grant) => grant.integrity === 'trusted')
  const refused = paths.filter((grant) => grant.integrity !== 'trusted' && grant.integrity !== 'undecided')
  const undecided = paths.filter((grant) => grant.integrity === 'undecided')
  return <>
    <section className="grant-section">
      <h3>Trusted paths</h3>
      <p className="grant-note">A parent grant covers its descendants unless a more specific rule overrides it. Revoking a parent keeps any separately listed child grants.</p>
      {trusted.length > 0 && <ul className="grant-list">{trusted.map((grant) => <li className="grant-row" key={grant.path}>
        <Icon name="shield-done" /><GrantName name={named(grant)} />
        <Button size="small" kind="plain-faint" isDisabled={busy} onClick={() => onRevoke(grant.path)}>Revoke</Button>
      </li>)}</ul>}
      {trusted.length === 0 && <p className="grant-empty"><Icon name="shield-done" />No trusted path grants.</p>}
    </section>
    {refused.length > 0 && <section className="grant-section"><h3>Untrusted path exceptions</h3><p className="grant-note">These paths remain untrusted even when a parent is trusted.</p>
      <ul className="grant-list">{refused.map((grant) => <li className="grant-row" key={grant.path}><Icon name="warning-triangle-outline" /><GrantName name={named(grant)} /><span className="grant-state">Untrusted</span></li>)}</ul></section>}
    {undecided.length > 0 && <section className="grant-section"><h3>Paths awaiting a decision</h3><p className="grant-note">These paths require write approval and their contents remain untrusted until you grant trust.</p>
      <ul className="grant-list">{undecided.map((grant) => <li className="grant-row" key={grant.path}><Icon name="radio-unchecked" /><GrantName name={named(grant)} /><span className="grant-state">Not decided</span></li>)}</ul></section>}
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
    ['Refused', 'Refused before anything is asked or started.', rules?.shownDeny ?? rules?.deny ?? []],
    ['Always asked', 'You are asked, whatever else would have answered.', rules?.shownAsk ?? rules?.ask ?? []],
    ['Not asked', 'The question is answered for you. What a command prints is not trusted because of it.', rules?.shownAllow ?? rules?.allow ?? []],
  ]
  const none = lists.every(([, , held]) => held.length === 0) && !(rules?.filesystem?.length || rules?.filesystemIgnored?.length)
  return <section className="grant-section settings-rules">
    <h3>Rules from settings files</h3>
    <p className="grant-note">Read when this conversation opened. Edit the settings file to change them; the change applies to the next conversation. Refused comes first, then always asked, then not asked. A plan run applies the rules about a server’s tools and no others.</p>
    {none && <p className="grant-empty"><Icon name="settings" />No permission rules are in force.</p>}
    {lists.filter(([, , held]) => held.length > 0).map(([name, meaning, held]) => <div key={name}>
      <h4>{name}</h4>
      <p className="grant-note">{meaning}</p>
      <ul className="grant-list">{held.map((rule, index) => <li className="grant-row" key={index}><GrantName name={rule} /></li>)}</ul>
    </div>)}
    <FilesystemRules rules={rules} />
  </section>
}

const FILESYSTEM_LISTS: [FilesystemKey, string, string][] = [
  ['denyRead', 'Never read', 'No program the agent runs reads these, whatever else would let it.'],
  ['denyWrite', 'Never written', 'No program the agent runs writes these, a directory the conversation was opened on included.'],
  ['allowRead', 'Also readable', 'Read even where the built-in list of credential locations would refuse. A private key under ~/.ssh is never lifted.'],
  ['allowWrite', 'Also writable', 'Written, and read, by the programs the agent runs.'],
]

/**
 * The four `sandbox.filesystem` lists, read only: which paths a program the agent runs is held
 * back from or given beyond the defaults, the file that wrote each, and why an entry is not in force.
 */
export function FilesystemRules({ rules }: { rules: SettingsRules | null }): React.JSX.Element | null {
  const entries = rules?.filesystem ?? []
  const ignored = rules?.filesystemIgnored ?? []
  if (entries.length === 0 && ignored.length === 0) return null
  return <div className="settings-rules-filesystem" data-test="filesystem-rules">
    <h3>Paths for programs the agent runs</h3>
    <p className="grant-note">Edit the settings file to change them; the change applies to the next conversation. A narrower path wins over a wider one.</p>
    {FILESYSTEM_LISTS.map(([key, name, meaning]) => {
      const held = entries.filter((entry) => entry.key === key)
      if (held.length === 0) return null
      return <div key={key}>
        <h4>{name}</h4>
        <p className="grant-note">{meaning}</p>
        <ul className="grant-list">{held.map((entry, index) => <li className="grant-row" key={index}>
          <GrantName name={entry.shownPath ?? entry.path} />
          <span className="grant-state">{entry.refused ? `Not in force: ${entry.refused}` : entry.pinned ? 'Set by your administrator' : entry.shownFile ?? entry.file ?? 'Command line'}</span>
        </li>)}</ul>
      </div>
    })}
    {ignored.map((entry, index) => <Alert type="warning" key={index} data-test="filesystem-ignored">
      {`${entry.key} in ${entry.shownFile ?? entry.file} is not obeyed: a project file can refuse reach and never add it.`}
    </Alert>)}
  </div>
}
