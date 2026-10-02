import { useEffect, useState } from 'react'
import { SettingsGroup } from './SettingsGroup'
import type { AgentSettings as Report, Hook, HooksDocument, Limits, Refusal } from '../../shared/agent-settings'
import { composeHooks } from '../../shared/agent-settings'
import { Alert, Button, Collapse, Dropdown, Icon, Input, ProgressRing } from '../nala'
const fieldText = (event: { value?: unknown; target?: EventTarget | null }): string | null => {
  if (typeof event.value === 'string') return event.value
  const target = event.target
  if (target && typeof target === 'object' && 'value' in target && typeof target.value === 'string') return target.value
  return null
}

const refusalText = (refusal: Refusal, on: string, off: string): string => {
  if (refusal.value !== true) return off
  return refusal.managed ? `${on} (set by your administrator)` : refusal.path ? `${on} (asked for by ${refusal.path})` : on
}
const grouped = new Intl.NumberFormat('en-US')
const runLimit = (value: number | null, unit: string): string => value === null ? 'Built-in' : `${grouped.format(value)} ${unit}`

function LimitsInForce({ limits }: { limits: Limits }): React.JSX.Element {
  return <>
    <h4>Limits in force</h4>
    <dl className="settings-limits-table" data-test="settings-limits">
      <dt>File tools</dt><dd>{refusalText(limits.readsStayInWorkspace, 'Refuse every path outside the working directory', 'May read outside the working directory when a rule or a question allows it')}</dd>
      <dt>Bypass mode</dt><dd>{refusalText(limits.bypassUnreachable, 'Unreachable', 'Reachable')}</dd>
      <dt>Command time limit</dt><dd>{runLimit(limits.run.defaultSeconds, 'seconds')}</dd>
      <dt>Longest a call may ask for</dt><dd>{runLimit(limits.run.maxSeconds, 'seconds')}</dd>
      <dt>Command output shown to the model</dt><dd>{runLimit(limits.run.maxOutput, 'bytes')}</dd>
    </dl>
    {limits.unreadable.map(item => <Alert key={`${item.name}:${item.path}`} type="warning" role="alert">
      <code>permissions.{item.name}</code> in <span className="settings-path">{item.path}</span> is not true or false, so it has no effect.
    </Alert>)}
  </>
}

/**
 * The agent's configuration, as one page of stacked sections.
 *
 * `onDirty` reports unsaved hook edits, so the page around it can ask before it is left.
 */
export function AgentSettings({ session, onChanged, onDirty }: {
  session?: string
  onChanged: () => void
  onDirty: (dirty: boolean) => void
}): React.JSX.Element {
  const [report, setReport] = useState<Report | null>(null)
  const [document, setDocument] = useState<HooksDocument | null>(null)
  const [hooks, setHooks] = useState<Hook[]>([])
  const [problem, setProblem] = useState('')
  const [status, setStatus] = useState('')
  const [busy, setBusy] = useState(false)
  const [dirty, setDirty] = useState(false)
  useEffect(() => { onDirty(dirty) }, [dirty, onDirty])
  const load = async () => {
    setBusy(true); setProblem('')
    try {
      const result = await window.bravebot.request<Report>('settings.inspect', { session })
      if (result.error) throw new Error(result.error.message)
      if (result.ok) {
        if (!Array.isArray(result.ok.providers) || !Array.isArray(result.ok.layers) || !result.ok.managed || !result.ok.network) throw new Error('This agent did not return configuration details. Rebuild or reinstall the app, then retry.')
        setReport(result.ok)
      }
    } catch (error) { setProblem(String(error)) } finally { setBusy(false) }
  }
  useEffect(() => { void load() }, [session])
  /** What the agent read out of the hooks file. The panel draws that and parses nothing itself. */
  const inspectHooks = async (): Promise<HooksDocument> => {
    const result = await window.bravebot.request<HooksDocument>('hooks.inspect')
    if (result.error) throw new Error(result.error.message)
    if (!result.ok || !Array.isArray(result.ok.hooks) || typeof result.ok.entire !== 'boolean') throw new Error('This agent did not return its hooks file. Rebuild or reinstall the app, then retry.')
    return result.ok
  }
  const loadHooks = async () => {
    setBusy(true); setProblem(''); setStatus('')
    try { const doc = await inspectHooks(); setDocument(doc); setHooks(doc.hooks); setDirty(false) }
    catch (error) { setProblem(String(error)) } finally { setBusy(false) }
  }
  useEffect(() => { void loadHooks() }, [])
  const change = (index: number, hook: Hook) => { setHooks(rows => rows.map((row, i) => i === index ? hook : row)); setDirty(true); setStatus('') }
  const save = async () => {
    if (!document) return
    // A hook with no program is not one the agent reads, so writing it would leave a file this
    // panel then refuses to edit. The agent still decides what a hook is; the form declines to
    // send a field somebody left empty.
    if (hooks.some(hook => !hook.run[0]?.trim())) { setProblem('Enter a program for every hook, or remove the hook.'); return }
    setBusy(true); setProblem('')
    const text = composeHooks(hooks)
    try {
      await window.bravebot.saveHooks(text, document.text)
      // The write landed, so this is the file now, whatever happens next. Without this a reading
      // that fails here leaves the panel expecting the text from before the save, and every retry
      // is refused as stale.
      setDocument({ ...document, text })
      // Read back rather than assumed: what the panel shows afterwards is what the reader a turn
      // fires hooks out of makes of the file, including an entry it could not use.
      const saved = await inspectHooks()
      setDocument(saved); setHooks(saved.hooks); setDirty(false); setStatus('Hooks saved. They apply when the next turn starts.')
    } catch (error) { setProblem(String(error)) } finally { setBusy(false) }
  }
  const select = async (clear: boolean) => {
    setBusy(true); setProblem(''); setStatus('')
    try {
      const next = await window.bravebot.selectSettings(clear)
      if (next) { setReport(next); onChanged(); setStatus(clear ? 'Run override cleared.' : 'Run override selected. Future turns and model discovery use it.'); await load() }
    } catch (error) { setProblem(String(error)) } finally { setBusy(false) }
  }
  // A file the agent passed over in part is the person's to edit: composing it back from the
  // entries it did read would drop the rest.
  const editable = !!document && document.entire
  return <div className="agent-settings settings-body" data-test="settings-body">
    {problem && <div className="settings-error"><Alert type="error" role="alert" data-test="settings-error">{problem}</Alert><Button size="small" kind="outline" isDisabled={busy} onClick={() => void load()}>Retry diagnostics</Button></div>}
    {status && <Alert type="success" size="small" role="status">{status}</Alert>}
    {busy && <p role="status" className="settings-busy"><ProgressRing mode="indeterminate" /> Working…</p>}
    <SettingsGroup title="Connection">
      {report && <>
        <section className="settings-block settings-status"><h3>{report.configured ? 'Model service configured' : 'Choose a model service'}</h3>
          {report.problem && <p>{report.problem}</p>}
          <dl>
            <dt>Agent build</dt><dd>{report.build}</dd>
            <dt>Default model</dt><dd>{report.model ?? 'Not configured'}</dd>
            {report.providers.map((p, i) => <div key={i} className="settings-dl-row"><dt>{p.name}</dt><dd>Credential {p.credential}</dd></div>)}
            {report.brave && <><dt>Brave service</dt><dd>Brave service enabled</dd></>}
            {report.bedrock && <><dt>AWS Bedrock</dt><dd>AWS Bedrock enabled</dd></>}
          </dl>
          <Collapse title="Set up a model service" isOpen={!report.configured ? true : undefined} data-test="settings-setup">
            <div className="setup-routes"><section><h4>Gateway or local model</h4><p>Add a provider to your agent settings, or select a file in Run settings. Credential-free local services need no token.</p><pre>{'{"provider":{"local":{"options":{"baseURL":"http://localhost:11434/v1"},"models":{"your-model":{}}}},"model":"local/your-model"}'}</pre><p>For authenticated services, name the credential environment variable in the provider’s <code>env</code> array.</p></section>
            <section><h4>AWS Bedrock</h4><p>Configure your AWS credentials and enable Bedrock in your agent settings or environment. Your AWS account must have access to the selected model.</p><pre>BRAVEBOT_USE_BEDROCK=1{'\n'}AWS_REGION=us-east-1{'\n'}AWS_PROFILE=your-profile{'\n'}ANTHROPIC_DEFAULT_SONNET_MODEL=your-model-id-or-inference-profile-arn</pre></section>
            <section><h4>Brave service</h4><p>Use a configured Brave build, or launch from a shell with <code>SERVICES_KEY_AICHAT</code>, <code>BRAVE_SERVICES_KEY_ID</code> and <code>BRAVE_AI_CHAT_ENDPOINT</code> set to your supplied credentials and endpoint. Credentials are not stored in this window.</p></section></div>
          </Collapse>
        </section>
        <section className="settings-block"><h3>Network</h3>
          <dl><dt>Certificate roots</dt><dd>{report.network.roots.length ? report.network.roots.join(', ') : 'Bundled roots'}</dd><dt>Proxy</dt><dd>{report.network.proxy ?? 'No proxy configured'}{report.network.authenticated ? ' · authenticated' : ''}</dd><dt>Proxy bypass</dt><dd>{report.network.noProxy ?? 'None'}</dd></dl>
          {(report.network.problem || report.network.trustsNothing || report.network.unusableProxy) && <Alert type="warning" role="alert">{report.network.problem || (report.network.trustsNothing ? 'No usable trust roots.' : `Unsupported proxy: ${report.network.unusableProxy}`)}</Alert>}
          <p>For a custom certificate authority, set <code>SSL_CERT_FILE</code> or <code>SSL_CERT_DIR</code> before opening the app. These replace bundled roots. Proxy and certificate changes require restarting the app.</p>
        </section>
        <section className="settings-block"><h3>Diagnostics</h3>
          <p>Read the agent’s configuration again, after changing a file or the environment.</p>
          <div className="settings-actions"><Button size="small" kind="outline" onClick={() => void load()} isDisabled={busy}><Icon name="refresh" slot="icon-before" />Refresh diagnostics</Button></div>
        </section>
        <section className="settings-block"><h3>Managed configuration</h3>{report.managed.path ? <><p className="settings-path">{report.managed.path}</p><p>{report.managed.keys.length ? `Locked by your administrator: ${report.managed.keys.join(', ')}` : 'File found; no recognized values are pinned.'}</p></> : <p>No administrator-managed configuration found.</p>}<p>Administrator-pinned destinations take precedence over your settings and environment.</p></section>
      </>}
    </SettingsGroup>
    <SettingsGroup title="Hooks" note={dirty ? 'Unsaved changes' : undefined}>
      <section className="settings-block"><h3>Your hooks</h3>
        <p>Hooks are shared with the terminal client. Run your own programs at specific moments. These commands run with your account’s permissions in the project directory. They cannot approve or block the agent.</p>
        {document && <div className="settings-box">
          <p className="settings-path">{document.path}</p>
          {document.entire && hooks.length === 0 && <p>No hooks configured.</p>}
        </div>}
        {document && !document.entire && <Alert type="warning" role="alert">{document.text === null
          ? 'The agent could not read this file, so it declares no hooks. Open it yourself to see why.'
          : 'The agent did not read all of this file, so saving it from here could drop what it passed over. Edit it directly.'}</Alert>}
        {/* A disabled fieldset does not reach into Leo's shadow roots, so each control is told. */}
        {hooks.map((hook, index) => <fieldset key={index} disabled={busy || !editable} className="hook-editor"><legend>Hook {index + 1}</legend>
          <Dropdown value={hook.on} disabled={busy || !editable} data-test={`hook-when-${index}`}
            onChange={(detail) => change(index, { ...hook, on: String(detail.value) as Hook['on'] })}>
            <span slot="label">When</span>
            <leo-option value="turn-started">Turn starts</leo-option>
            <leo-option value="tool-finished">Tool finishes</leo-option>
            <leo-option value="turn-finished">Turn ends</leo-option>
          </Dropdown>
          {(hook.on === 'tool-finished' || hook.tool !== null) &&
            <Input value={hook.tool ?? ''} placeholder="All tools" disabled={busy || !editable}
              onInput={(event) => { const value = fieldText(event); if (value !== null) change(index, { ...hook, tool: value.trim() || null }) }}>Tool filter (optional)</Input>}
          {!dirty && document?.hooks[index]?.firesForNothing && <Alert type="warning" role="alert">This hook fires for nothing: only a finished tool call carries a tool name. Clear the filter, or choose Tool finishes.</Alert>}
          <Input value={hook.run[0]} placeholder="/path/to/program" disabled={busy || !editable} data-test={`hook-program-${index}`}
            onInput={(event) => { const value = fieldText(event); if (value !== null) change(index, { ...hook, run: [value, ...hook.run.slice(1)] }) }}>Program</Input>
          {hook.run.slice(1).map((argument, i) => <div key={i} className="hook-argument">
            <Input value={argument} disabled={busy || !editable} onInput={(event) => { const value = fieldText(event); if (value !== null) change(index, { ...hook, run: hook.run.map((word, j) => j === i + 1 ? value : word) }) }}>{`Argument ${i + 1}`}</Input>
          <Button size="small" kind="plain-faint" isDisabled={busy || !editable} onClick={() => change(index, { ...hook, run: hook.run.filter((_, j) => j !== i + 1) })} aria-label={`Remove argument ${i + 1} from hook ${index + 1}`}>Remove</Button></div>)}
          <div className="settings-actions">
            <Button size="small" kind="plain" isDisabled={busy || !editable} onClick={() => change(index, { ...hook, run: [...hook.run, ''] })}>Add argument</Button>
            <Button size="small" kind="plain-faint" isDisabled={busy || !editable} onClick={() => { setHooks(rows => rows.filter((_, i) => i !== index)); setDirty(true) }}>Remove hook</Button>
          </div>
        </fieldset>)}
        <p className="settings-caption">Arguments are passed exactly as entered; shell syntax is not interpreted. Failed hooks appear in the turn’s notices.</p>
        <div className="settings-actions">
          <Button size="small" kind="filled" isDisabled={!dirty || busy || !editable} onClick={() => void save()} data-test="hook-save">Save hooks</Button>
          <Button size="small" kind="outline" isDisabled={!editable || busy} onClick={() => { setHooks(rows => [...rows, { on: 'turn-finished', tool: null, run: [''], firesForNothing: false }]); setDirty(true) }} data-test="hook-add">Add hook</Button>
          <Button size="small" kind="plain-faint" isDisabled={busy} onClick={() => { if (!dirty || window.confirm('Discard unsaved hook changes and reload?')) void loadHooks() }}>Reload hooks</Button>
        </div>
      </section>
    </SettingsGroup>
    <SettingsGroup title="Run settings">
      {report && <>
        <section className="settings-block"><h3>Model and connection override</h3>
          <p>Choose a JSON settings file for this app run. It applies to future turns and model discovery, and is cleared when the app exits. Running turns keep their configuration. Terminal preferences and settings-file permission grants do not change this app’s approval controls.</p>
          <div className="settings-box"><p className="settings-path">{report.selected ?? 'No override selected'}</p></div>
          <div className="settings-actions">
            <Button size="small" kind="outline" isDisabled={busy} onClick={() => void select(false)}>Choose settings file…</Button>
            <Button size="small" kind="plain-faint" isDisabled={busy || !report.selected} onClick={() => void select(true)}>Clear override</Button>
          </div>
        </section>
        <section className="settings-block"><h3>Effective configuration</h3>
          <dl><dt>Default model</dt><dd>{report.model ?? 'Not configured'}</dd></dl>
          <p>Files merge in this order: home → project → project-local → selected override. Environment and built-in values can take precedence; administrator-pinned destinations always win.</p>
          <div>
            <h4>Loaded files, in order</h4>{report.layers.length ? <ol>{report.layers.map(path => <li key={path} className="settings-path">{path}</li>)}</ol> : <p>No settings files loaded.</p>}
          </div>
          {report.overrides.map(item => <p key={item.name}><code>{item.name}</code> overridden by <span className="settings-path">{item.path}</span></p>)}
          {(report.ignored ?? []).map(item => <p key={`${item.name}:${item.path}`}><code>{item.name}</code> in <span className="settings-path">{item.path}</span> is ignored. Only your home settings file or a file you choose above can set it.</p>)}
          {report.limits && <div><LimitsInForce limits={report.limits} /></div>}
          {report.managed.keys.length > 0 && <p>Managed values: {report.managed.keys.join(', ')}. This override cannot change them.</p>}
        </section>
      </>}
    </SettingsGroup>
  </div>
}
