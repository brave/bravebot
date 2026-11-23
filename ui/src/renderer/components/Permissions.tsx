import { useEffect, useRef, useState } from 'react'
import { Modal } from './Modal'
import type { FilesystemKey, KeptTrust, RuleCheck as RuleChecked, RuleCheckFamily, SettingsRules } from '../../shared/protocol'
import { Alert, Button, Dropdown, Icon, Input } from '../nala'
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
  /** `directory` is the tree the command was vouched in, as held; null where it has no text spelling, which no revoke matches. `shownDirectory` is the same tree spelled for the screen (`.` for the workspace itself) and is only drawn. */
  commands: { program: string; startedAs: string; args: string[]; directory: string | null; shownDirectory: string; display: string }[]
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
      {grants && grants.commands.length > 0 && <CommandPermissions commands={grants.commands} busy={busy} onRevoke={(command) => void request('permissions.revoke', { kind: 'command', command })} />}
      {grants?.commands.length === 0 && <p className="grant-empty"><Icon name="window-console" />No remembered command grants.</p>}
    </section>
    <section className="grant-section">
      <h3>Trusted for this directory</h3>
      <p className="grant-note">Kept outside this conversation: sessions started in this directory, or below it where it is the root of a git repository, are trusted without asking. Forgetting it makes the next one ask; this conversation keeps its own grants.</p>
      {grants?.remembered && <RememberedHere kept={grants.remembered} busy={busy} onForget={() => void request('permissions.revoke', { kind: 'remembered' })} />}
      {grants && !grants.remembered && <p className="grant-empty"><Icon name="pin" />Nothing is kept for this directory.</p>}
    </section>
    <RulesInForce rules={grants?.settingsRules ?? null} />
    <RuleCheck session={session} onProblem={setProblem} />
  </Modal>
}

/** The families `permissions.check` reads, each with what its field asks for. WebSearch takes no words. */
export const RULE_CHECK_FAMILIES: [RuleCheckFamily, string][] = [
  ['Read', 'path'],
  ['Edit', 'path'],
  ['Bash', 'program args'],
  ['WebFetch', 'host or URL'],
  ['WebSearch', 'No words for a web search'],
  ['Mcp', 'server:tool'],
]

/** The words sent for what was typed: a command split on runs of whitespace, a web search none, and anything else the field as one word. */
export function ruleCheckWords(family: RuleCheckFamily, text: string): string[] {
  if (family === 'WebSearch') return []
  if (family === 'Bash') return text.split(/\s+/).filter((word) => word !== '')
  return [text]
}

/** Asks the bridge which of the session's rules decides the call. A refusal comes back as the bridge's own words. */
export async function checkRule(session: string, family: RuleCheckFamily, text: string): Promise<{ answer: RuleChecked } | { problem: string }> {
  try {
    const answer = await window.bravebot.request<RuleChecked>('permissions.check', { session, family, words: ruleCheckWords(family, text) })
    if (answer.error) return { problem: answer.error.message }
    if (answer.ok) return { answer: answer.ok }
    return { problem: 'The rule check got no answer. Try again.' }
  } catch { return { problem: 'The rule could not be checked. Try again.' } }
}

/**
 * Which rule from the conversation's settings decides a call, asked of the rules the
 * conversation holds (CLI-30, PERM-12). It checks and changes nothing.
 */
export function RuleCheck({ session, onProblem }: { session: string; onProblem: (problem: string) => void }): React.JSX.Element {
  const [family, setFamily] = useState<RuleCheckFamily>('Bash')
  const [text, setText] = useState('')
  const [answer, setAnswer] = useState<RuleChecked | null>(null)
  const [busy, setBusy] = useState(false)
  // A click on a Leo button reaches both its inner control and the host, so the second is ignored.
  const checking = useRef(false)
  const check = async () => {
    if (checking.current) return
    checking.current = true
    setBusy(true); onProblem('')
    const result = await checkRule(session, family, text)
    if ('answer' in result) setAnswer(result.answer)
    else { setAnswer(null); onProblem(result.problem) }
    setBusy(false)
    checking.current = false
  }
  return <RuleCheckForm family={family} text={text} busy={busy} answer={answer}
    onFamily={(next) => { setFamily(next); setAnswer(null) }} onText={setText} onCheck={() => void check()} />
}

export function RuleCheckForm({ family, text, busy, answer, onFamily, onText, onCheck }: {
  family: RuleCheckFamily; text: string; busy: boolean; answer: RuleChecked | null
  onFamily: (family: RuleCheckFamily) => void; onText: (text: string) => void; onCheck: () => void
}): React.JSX.Element {
  const noWords = family === 'WebSearch'
  const placeholder = RULE_CHECK_FAMILIES.find(([name]) => name === family)?.[1] ?? ''
  return <section className="grant-section rule-check" data-test="rule-check">
    <h3>Check a call</h3>
    <p className="grant-note">Shows which rule from the settings files decides a call, using the rules this conversation opened under. Nothing is run or granted.</p>
    <div className="rule-check-form">
      <Dropdown value={family} size="small" disabled={busy} data-test="rule-check-family"
        onChange={(detail) => { const next = RULE_CHECK_FAMILIES.find(([name]) => name === String(detail.value)); if (next) onFamily(next[0]) }}>
        <span slot="label">Tool</span>
        {RULE_CHECK_FAMILIES.map(([name]) => <leo-option key={name} value={name}>{name}</leo-option>)}
      </Dropdown>
      <Input value={noWords ? '' : text} size="small" placeholder={placeholder} disabled={busy || noWords} data-test="rule-check-words"
        onInput={(event) => {
          const detail = event as { value?: unknown; target?: EventTarget | null }
          const value = typeof detail.value === 'string' ? detail.value
            : detail.target && typeof detail.target === 'object' && 'value' in detail.target && typeof detail.target.value === 'string' ? detail.target.value
              : null
          if (value !== null) onText(value)
        }}>Call</Input>
      <Button size="small" kind="outline" onClick={onCheck} isDisabled={busy || (!noWords && !text.trim())} data-test="rule-check-submit">Check</Button>
    </div>
    {answer && <RuleCheckAnswer answer={answer} />}
  </section>
}

const DECISIONS: Record<RuleChecked['decision'], string> = {
  deny: 'Refused',
  ask: 'Always asked',
  allow: 'Not asked',
  none: 'No rule matches, so the usual questions decide',
  shortName: 'Refused: a command rule is in force, and an 8.3 short name cannot be matched against it',
}

/** The answer as rows: the decision, the rule as its file spelled it, the file or grant it came from, the person's reason, and a checkout rule that is not in force. */
export function RuleCheckAnswer({ answer }: { answer: RuleChecked }): React.JSX.Element {
  const rule = answer.shownRule ?? answer.rule
  const file = answer.shownFile ?? answer.file
  const away = answer.notInForce
  return <div className="rule-check-answer" data-test="rule-check-answer" aria-live="polite">
    <ul className="grant-list">
      <li className="grant-row"><span className="rule-check-label">Decision</span><span className="rule-check-value" data-test="rule-check-decision" data-decision={answer.decision}>{DECISIONS[answer.decision] ?? answer.decision}</span></li>
      {rule !== null && <li className="grant-row"><span className="rule-check-label">Rule</span><GrantName name={rule} /></li>}
      {file !== null && <li className="grant-row" data-test="rule-check-file"><span className="rule-check-label">File</span>
        {answer.granted ? <span className="rule-check-value">Granted for this workspace, written in <GrantName name={file} /></span> : <GrantName name={file} />}</li>}
      {answer.reason !== null && <li className="grant-row"><span className="rule-check-label">Reason</span><span className="rule-check-value" data-test="rule-check-reason">{answer.reason}</span></li>}
    </ul>
    {away && <Alert type="warning" data-test="rule-check-not-in-force">
      Not in force: <code>{away.shownRule ?? away.rule}</code> in <code>{away.shownFile ?? away.file}</code> would allow this once it is granted at the question.
    </Alert>}
  </div>
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
 * The remembered commands, one row for each entry. An entry is keyed on the tree it was vouched in as well as the
 * command, so two rows can read alike and differ in the tree drawn under them. Revoke names the tree, and the
 * bridge withdraws only the entry held in it.
 */
export function CommandPermissions({ commands, busy, onRevoke }: { commands: Grants['commands']; busy: boolean; onRevoke: (command: { program: string; startedAs: string; args: string[]; directory: string | null }) => void }): React.JSX.Element {
  return <ul className="grant-list">{commands.map((command) => <li className="grant-row" key={commandKey(command)}>
    <Icon name="window-console" />
    <div className="grant-text">
      <GrantName name={command.display} />
      <span className="grant-meta" data-test="command-tree">{`in ${command.shownDirectory}`}</span>
      {command.directory === null && <span className="grant-meta" data-test="command-not-revocable">This tree is not text, so no revoke can name it.</span>}
    </div>
    <Button size="small" kind="plain-faint" isDisabled={busy || command.directory === null} onClick={() => onRevoke({ program: command.program, startedAs: command.startedAs, args: command.args, directory: command.directory })}>Revoke</Button>
  </li>)}</ul>
}

/** All four fields of an entry's key, so two entries for one command in two trees are two rows. */
export const commandKey = (command: Grants['commands'][number]): string => JSON.stringify([command.program, command.startedAs, command.args, command.directory])

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
