import { useEffect, useId, useState } from 'react'
import { Modal } from './Modal'
import type { AgentSettings as Report, Hook, HooksDocument } from '../../shared/agent-settings'
import { parseHooks } from '../../shared/agent-settings'

export function AgentSettings({ session, onClose, onChanged }: { session?: string; onClose: () => void; onChanged: () => void }): React.JSX.Element {
  const id = useId()
  const tabs = ['Connection', 'Hooks', 'Run settings']
  const [tab, setTab] = useState('Connection')
  const [report, setReport] = useState<Report | null>(null)
  const [document, setDocument] = useState<HooksDocument | null>(null)
  const [hooks, setHooks] = useState<Hook[]>([])
  const [problem, setProblem] = useState('')
  const [status, setStatus] = useState('')
  const [busy, setBusy] = useState(false)
  const [dirty, setDirty] = useState(false)
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
  const loadHooks = async () => {
    setBusy(true); setProblem(''); setStatus('')
    try { const doc = await window.bravebot.readHooks(); setDocument(doc); setHooks(doc.hooks); setDirty(false) }
    catch (error) { setProblem(String(error)) } finally { setBusy(false) }
  }
  useEffect(() => { if (tab === 'Hooks' && !document) void loadHooks() }, [tab])
  const change = (index: number, hook: Hook) => { setHooks(rows => rows.map((row, i) => i === index ? hook : row)); setDirty(true); setStatus('') }
  const save = async () => {
    if (!document) return
    setBusy(true); setProblem('')
    try {
      const text = JSON.stringify({ hooks }, null, 2) + '\n'
      parseHooks(text)
      const saved = await window.bravebot.saveHooks(text, document.text)
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
  const close = () => { if (!dirty || window.confirm('Discard unsaved hook changes?')) onClose() }
  return <Modal title="Agent settings" onClose={busy ? undefined : close} className="agent-settings">
    <div className="settings-heading"><div><h2>Agent settings</h2><p>Configuration and automation for this app.</p></div><button onClick={close} disabled={busy} aria-label="Close agent settings">×</button></div>
    <div className="settings-tabs" role="tablist" aria-label="Agent settings sections">
      {tabs.map((name, index) => <button key={name} id={`${id}-tab-${index}`} aria-controls={`${id}-panel`} tabIndex={tab === name ? 0 : -1} onKeyDown={event => {
        const next = event.key === 'ArrowRight' ? (index + 1) % tabs.length : event.key === 'ArrowLeft' ? (index + tabs.length - 1) % tabs.length : event.key === 'Home' ? 0 : event.key === 'End' ? tabs.length - 1 : -1
        if (next >= 0) { event.preventDefault(); setTab(tabs[next]!); globalThis.document.getElementById(`${id}-tab-${next}`)?.focus() }
      }} role="tab" aria-selected={tab === name} onClick={() => { setTab(name); setProblem(''); setStatus('') }}>{name}{name === 'Hooks' && dirty ? ' •' : ''}</button>)}
    </div>
    {problem && <div className="settings-error"><p role="alert">{problem}</p>{tab !== 'Hooks' && <button disabled={busy} onClick={() => void load()}>Retry diagnostics</button>}</div>}
    {status && <p role="status">{status}</p>}
    {busy && <p role="status">Working…</p>}
    <div id={`${id}-panel`} role="tabpanel" aria-labelledby={`${id}-tab-${tabs.indexOf(tab)}`} className="settings-body">
      {tab === 'Connection' && report && <>
        <section><h3>{report.configured ? 'Model service configured' : 'Choose a model service'}</h3>
          {report.problem && <p>{report.problem}</p>}
          <dl><dt>Agent build</dt><dd>{report.build}</dd><dt>Default model</dt><dd>{report.model ?? 'Not configured'}</dd></dl>
          {report.providers.map((p, i) => <p key={i}><strong>{p.name}</strong> · Credential {p.credential}</p>)}
          {report.brave && <p>Brave service enabled</p>}{report.bedrock && <p>AWS Bedrock enabled</p>}
        </section>
        <details open={!report.configured}><summary>Set up a model service</summary>
          <div className="setup-routes"><section><h4>Gateway or local model</h4><p>Add a provider to your agent settings, or select a file in Run settings. Credential-free local services need no token.</p><pre>{'{"provider":{"local":{"options":{"baseURL":"http://localhost:11434/v1"},"models":{"your-model":{}}}},"model":"local/your-model"}'}</pre><p>For authenticated services, name the credential environment variable in the provider’s <code>env</code> array.</p></section>
          <section><h4>AWS Bedrock</h4><p>Configure your AWS credentials and enable Bedrock in your agent settings or environment. Your AWS account must have access to the selected model.</p><pre>BRAVEBOT_USE_BEDROCK=1{'\n'}AWS_REGION=us-east-1{'\n'}AWS_PROFILE=your-profile{'\n'}ANTHROPIC_DEFAULT_SONNET_MODEL=your-model-id-or-inference-profile-arn</pre></section>
          <section><h4>Brave service</h4><p>Use a configured Brave build, or launch from a shell with <code>SERVICES_KEY_AICHAT</code>, <code>BRAVE_SERVICES_KEY_ID</code> and <code>BRAVE_AI_CHAT_ENDPOINT</code> set to your supplied credentials and endpoint. Credentials are not stored in this window.</p></section></div>
        </details>
        <section><h3>Network</h3><dl><dt>Certificate roots</dt><dd>{report.network.roots.length ? report.network.roots.join(', ') : 'Bundled roots'}</dd><dt>Proxy</dt><dd>{report.network.proxy ?? 'No proxy configured'}{report.network.authenticated ? ' · authenticated' : ''}</dd><dt>Proxy bypass</dt><dd>{report.network.noProxy ?? 'None'}</dd></dl>
          {(report.network.problem || report.network.trustsNothing || report.network.unusableProxy) && <p role="alert">{report.network.problem || (report.network.trustsNothing ? 'No usable trust roots.' : `Unsupported proxy: ${report.network.unusableProxy}`)}</p>}
          <p>For a custom certificate authority, set <code>SSL_CERT_FILE</code> or <code>SSL_CERT_DIR</code> before opening the app. These replace bundled roots. Proxy and certificate changes require restarting the app.</p>
        </section>
        <section><h3>Managed configuration</h3>{report.managed.path ? <><p>{report.managed.path}</p><p>{report.managed.keys.length ? `Locked by your administrator: ${report.managed.keys.join(', ')}` : 'File found; no recognized values are pinned.'}</p></> : <p>No administrator-managed configuration found.</p>}<p>Administrator-pinned destinations take precedence over your settings and environment.</p></section>
        <button onClick={() => void load()} disabled={busy}>Refresh diagnostics</button>
      </>}
      {tab === 'Hooks' && <>
        <p>Hooks are shared with the terminal client. Run your own programs at specific moments. These commands run with your account’s permissions in the project directory. They cannot approve or block the agent.</p>
        {document && <p className="settings-path">{document.path}</p>}
        {document && hooks.length === 0 && <p>No hooks configured.</p>}
        {hooks.map((hook, index) => <fieldset key={index} disabled={busy} className="hook-editor"><legend>Hook {index + 1}</legend>
          <label>When<select value={hook.on} onChange={e => change(index, { ...hook, on: e.target.value as Hook['on'], tool: undefined })}><option value="turn-started">Turn starts</option><option value="tool-finished">Tool finishes</option><option value="turn-finished">Turn ends</option></select></label>
          {hook.on === 'tool-finished' && <label>Tool filter (optional)<input value={hook.tool ?? ''} placeholder="All tools" onChange={e => change(index, { ...hook, tool: e.target.value || undefined })} /></label>}
          <label>Program<input value={hook.run[0]} placeholder="/path/to/program" onChange={e => change(index, { ...hook, run: [e.target.value, ...hook.run.slice(1)] })} /></label>
          {hook.run.slice(1).map((argument, i) => <div key={i} className="hook-argument"><label>Argument {i + 1}<input value={argument} onChange={e => change(index, { ...hook, run: hook.run.map((word, j) => j === i + 1 ? e.target.value : word) })} /></label><button onClick={() => change(index, { ...hook, run: hook.run.filter((_, j) => j !== i + 1) })} aria-label={`Remove argument ${i + 1} from hook ${index + 1}`}>Remove</button></div>)}
          <div className="settings-actions"><button onClick={() => change(index, { ...hook, run: [...hook.run, ''] })}>Add argument</button><button onClick={() => { setHooks(rows => rows.filter((_, i) => i !== index)); setDirty(true) }}>Remove hook</button></div>
        </fieldset>)}
        <p>Arguments are passed exactly as entered; shell syntax is not interpreted. Failed hooks appear in the turn’s notices.</p>
        <div className="settings-actions"><button disabled={!document || busy} onClick={() => { setHooks(rows => [...rows, { on: 'turn-finished', run: [''] }]); setDirty(true) }}>Add hook</button><button disabled={busy} onClick={() => { if (!dirty || window.confirm('Discard unsaved hook changes and reload?')) void loadHooks() }}>Reload hooks</button><button disabled={!dirty || busy} onClick={() => void save()}>Save hooks</button></div>
      </>}
      {tab === 'Run settings' && report && <>
        <h3>Model and connection override</h3><p>Choose a JSON settings file for this app run. It applies to future turns and model discovery, and is cleared when the app exits. Running turns keep their configuration. Terminal preferences and settings-file permission grants do not change this app’s approval controls.</p>
        <p className="settings-path">{report.selected ?? 'No override selected'}</p><div className="settings-actions"><button disabled={busy} onClick={() => void select(false)}>Choose settings file…</button><button disabled={busy || !report.selected} onClick={() => void select(true)}>Clear override</button></div>
        <h3>Effective configuration</h3><p>Default model: <strong>{report.model ?? 'Not configured'}</strong></p>
        <p>Files merge in this order: home → project → project-local → selected override. Environment and built-in values can take precedence; administrator-pinned destinations always win.</p>
        <h4>Loaded files, in order</h4>{report.layers.length ? <ol>{report.layers.map(path => <li key={path} className="settings-path">{path}</li>)}</ol> : <p>No settings files loaded.</p>}
        {report.overrides.map(item => <p key={item.name}><code>{item.name}</code> overridden by <span className="settings-path">{item.path}</span></p>)}
        {report.managed.keys.length > 0 && <p>Managed values: {report.managed.keys.join(', ')}. This override cannot change them.</p>}
      </>}
    </div>
  </Modal>
}
