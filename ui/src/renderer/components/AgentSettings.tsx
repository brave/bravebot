import { useEffect, useState } from 'react'
import { DialogClose, Modal } from './Modal'
import type { AgentSettings as Report, Hook, HooksDocument } from '../../shared/agent-settings'
import { composeHooks } from '../../shared/agent-settings'
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from './ui/accordion'
import { Alert, AlertDescription } from './ui/alert'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from './ui/alert-dialog'
import { Button } from './ui/button'
import { ButtonGroup } from './ui/button-group'
import { Card, CardContent, CardHeader, CardTitle } from './ui/card'
import { Field, FieldLabel, FieldLegend, FieldSet } from './ui/field'
import { Input } from './ui/input'
import { InputGroup, InputGroupAddon, InputGroupButton, InputGroupInput } from './ui/input-group'
import { Item, ItemContent, ItemGroup } from './ui/item'
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from './ui/select'
import { Separator } from './ui/separator'
import { Spinner } from './ui/spinner'
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs'

export function AgentSettings({ session, onClose, onChanged }: { session?: string; onClose: () => void; onChanged: () => void }): React.JSX.Element {
  const tabs = ['Connection', 'Hooks', 'Run settings']
  const [tab, setTab] = useState('Connection')
  const [report, setReport] = useState<Report | null>(null)
  const [document, setDocument] = useState<HooksDocument | null>(null)
  const [hooks, setHooks] = useState<Hook[]>([])
  const [problem, setProblem] = useState('')
  const [status, setStatus] = useState('')
  const [busy, setBusy] = useState(false)
  const [dirty, setDirty] = useState(false)
  const [discardAction, setDiscardAction] = useState<'close' | 'reload' | null>(null)
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
  useEffect(() => { if (tab === 'Hooks' && !document) void loadHooks() }, [tab])
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
  const close = () => {
    if (busy) return
    if (dirty) setDiscardAction('close')
    else onClose()
  }
  const reloadHooks = () => {
    if (busy) return
    if (dirty) setDiscardAction('reload')
    else void loadHooks()
  }
  const discard = () => {
    const action = discardAction
    setDiscardAction(null)
    if (action === 'close') onClose()
    else if (action === 'reload') void loadHooks()
  }
  // A file the agent passed over in part is the person's to edit: composing it back from the
  // entries it did read would drop the rest.
  const editable = !!document && document.entire
  return <Modal title="Agent settings" onClose={busy ? undefined : close} className="agent-settings w-[min(720px,calc(100vw-40px))]">
    <div className="settings-heading flex items-start justify-between gap-3"><div><h2>Agent settings</h2><p>Configuration and automation for this app.</p></div><DialogClose asChild><Button variant="ghost" size="icon-sm" onClick={close} disabled={busy} aria-label="Close agent settings">×</Button></DialogClose></div>
    <Tabs value={tab} onValueChange={(name) => { setTab(name); setProblem(''); setStatus('') }}>
    <TabsList className="settings-tabs" variant="line" aria-label="Agent settings sections">
      {tabs.map((name) => <TabsTrigger key={name} value={name}>{name}{name === 'Hooks' && dirty ? ' •' : ''}</TabsTrigger>)}
    </TabsList>
    {problem && <Alert variant="destructive" className="settings-error"><AlertDescription><p>{problem}</p>{tab !== 'Hooks' && <Button variant="outline" disabled={busy} onClick={() => void load()}>Retry diagnostics</Button>}</AlertDescription></Alert>}
    {status && <Alert role="status"><AlertDescription>{status}</AlertDescription></Alert>}
    {busy && <Alert role="status"><Spinner aria-hidden="true" /><AlertDescription>Working…</AlertDescription></Alert>}
    <TabsContent value="Connection" className="settings-body">
      {report && <>
        <Card role="region" aria-labelledby="connection-status-title">
          <CardHeader><CardTitle><h3 id="connection-status-title">{report.configured ? 'Model service configured' : 'Choose a model service'}</h3></CardTitle></CardHeader>
          <CardContent>
            {report.problem && <Alert variant="destructive"><AlertDescription>{report.problem}</AlertDescription></Alert>}
            <dl><dt>Agent build</dt><dd>{report.build}</dd><dt>Default model</dt><dd>{report.model ?? 'Not configured'}</dd></dl>
            <ItemGroup>
              {report.providers.map((p, i) => <Item key={i} size="sm" role="listitem"><ItemContent><p><strong>{p.name}</strong> · Credential {p.credential}</p></ItemContent></Item>)}
              {report.brave && <Item size="sm" role="listitem"><ItemContent><p>Brave service enabled</p></ItemContent></Item>}
              {report.bedrock && <Item size="sm" role="listitem"><ItemContent><p>AWS Bedrock enabled</p></ItemContent></Item>}
            </ItemGroup>
          </CardContent>
        </Card>
        <Accordion key={report.configured ? 'configured' : 'unconfigured'} type="single" collapsible defaultValue={report.configured ? undefined : 'model-service-setup'}>
          <AccordionItem value="model-service-setup">
            <AccordionTrigger>Set up a model service</AccordionTrigger>
            <AccordionContent>
              <div className="setup-routes flex flex-col gap-3">
                <Card><CardHeader><CardTitle><h4>Gateway or local model</h4></CardTitle></CardHeader><CardContent><p>Add a provider to your agent settings, or select a file in Run settings. Credential-free local services need no token.</p><pre>{'{"provider":{"local":{"options":{"baseURL":"http://localhost:11434/v1"},"models":{"your-model":{}}}},"model":"local/your-model"}'}</pre><p>For authenticated services, name the credential environment variable in the provider’s <code>env</code> array.</p></CardContent></Card>
                <Card><CardHeader><CardTitle><h4>AWS Bedrock</h4></CardTitle></CardHeader><CardContent><p>Configure your AWS credentials and enable Bedrock in your agent settings or environment. Your AWS account must have access to the selected model.</p><pre>BRAVEBOT_USE_BEDROCK=1{'\n'}AWS_REGION=us-east-1{'\n'}AWS_PROFILE=your-profile{'\n'}ANTHROPIC_DEFAULT_SONNET_MODEL=your-model-id-or-inference-profile-arn</pre></CardContent></Card>
                <Card><CardHeader><CardTitle><h4>Brave service</h4></CardTitle></CardHeader><CardContent><p>Use a configured Brave build, or launch from a shell with <code>SERVICES_KEY_AICHAT</code>, <code>BRAVE_SERVICES_KEY_ID</code> and <code>BRAVE_AI_CHAT_ENDPOINT</code> set to your supplied credentials and endpoint. Credentials are not stored in this window.</p></CardContent></Card>
              </div>
            </AccordionContent>
          </AccordionItem>
        </Accordion>
        <Card role="region" aria-labelledby="network-title">
          <CardHeader><CardTitle><h3 id="network-title">Network</h3></CardTitle></CardHeader>
          <CardContent>
            <dl><dt>Certificate roots</dt><dd>{report.network.roots.length ? report.network.roots.join(', ') : 'Bundled roots'}</dd><dt>Proxy</dt><dd>{report.network.proxy ?? 'No proxy configured'}{report.network.authenticated ? ' · authenticated' : ''}</dd><dt>Proxy bypass</dt><dd>{report.network.noProxy ?? 'None'}</dd></dl>
            {(report.network.problem || report.network.trustsNothing || report.network.unusableProxy) && <Alert variant="destructive"><AlertDescription>{report.network.problem || (report.network.trustsNothing ? 'No usable trust roots.' : `Unsupported proxy: ${report.network.unusableProxy}`)}</AlertDescription></Alert>}
            <p>For a custom certificate authority, set <code>SSL_CERT_FILE</code> or <code>SSL_CERT_DIR</code> before opening the app. These replace bundled roots. Proxy and certificate changes require restarting the app.</p>
          </CardContent>
        </Card>
        <Card role="region" aria-labelledby="managed-configuration-title">
          <CardHeader><CardTitle><h3 id="managed-configuration-title">Managed configuration</h3></CardTitle></CardHeader>
          <CardContent>{report.managed.path ? <><p>{report.managed.path}</p><p>{report.managed.keys.length ? `Locked by your administrator: ${report.managed.keys.join(', ')}` : 'File found; no recognized values are pinned.'}</p></> : <p>No administrator-managed configuration found.</p>}<p>Administrator-pinned destinations take precedence over your settings and environment.</p></CardContent>
        </Card>
        <Button variant="outline" onClick={() => void load()} disabled={busy}>Refresh diagnostics</Button>
      </>}
    </TabsContent>
    <TabsContent value="Hooks" className="settings-body">
      <>
        <p>Hooks are shared with the terminal client. Run your own programs at specific moments. These commands run with your account’s permissions in the project directory. They cannot approve or block the agent.</p>
        {document && <p className="settings-path font-mono">{document.path}</p>}
        {document && !document.entire && <Alert variant="destructive"><AlertDescription>{document.text === null
          ? 'The agent could not read this file, so it declares no hooks. Open it yourself to see why.'
          : 'The agent did not read all of this file, so saving it from here could drop what it passed over. Edit it directly.'}</AlertDescription></Alert>}
        {document && document.entire && hooks.length === 0 && <p>No hooks configured.</p>}
        {hooks.map((hook, index) => <FieldSet key={index} disabled={busy || !editable} className="hook-editor"><FieldLegend>Hook {index + 1}</FieldLegend>
          <Field><FieldLabel htmlFor={`hook-${index}-when`}>When</FieldLabel><Select value={hook.on} onValueChange={value => change(index, { ...hook, on: value as Hook['on'] })}><SelectTrigger id={`hook-${index}-when`} className="w-full"><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem value="turn-started">Turn starts</SelectItem><SelectItem value="tool-finished">Tool finishes</SelectItem><SelectItem value="turn-finished">Turn ends</SelectItem></SelectGroup></SelectContent></Select></Field>
          {(hook.on === 'tool-finished' || hook.tool !== null) && <Field><FieldLabel htmlFor={`hook-${index}-tool`}>Tool filter (optional)</FieldLabel><Input id={`hook-${index}-tool`} value={hook.tool ?? ''} placeholder="All tools" onChange={e => change(index, { ...hook, tool: e.target.value.trim() || null })} /></Field>}
          {!dirty && document?.hooks[index]?.firesForNothing && <Alert variant="destructive"><AlertDescription>This hook fires for nothing: only a finished tool call carries a tool name. Clear the filter, or choose Tool finishes.</AlertDescription></Alert>}
          <Field><FieldLabel htmlFor={`hook-${index}-program`}>Program</FieldLabel><Input id={`hook-${index}-program`} value={hook.run[0]} placeholder="/path/to/program" onChange={e => change(index, { ...hook, run: [e.target.value, ...hook.run.slice(1)] })} /></Field>
          {hook.run.slice(1).map((argument, i) => <Field key={i} className="hook-argument"><FieldLabel htmlFor={`hook-${index}-argument-${i}`}>Argument {i + 1}</FieldLabel><InputGroup><InputGroupInput id={`hook-${index}-argument-${i}`} value={argument} onChange={e => change(index, { ...hook, run: hook.run.map((word, j) => j === i + 1 ? e.target.value : word) })} /><InputGroupAddon align="inline-end"><InputGroupButton variant="outline" onClick={() => change(index, { ...hook, run: hook.run.filter((_, j) => j !== i + 1) })} aria-label={`Remove argument ${i + 1} from hook ${index + 1}`}>Remove</InputGroupButton></InputGroupAddon></InputGroup></Field>)}
          <ButtonGroup className="settings-actions"><Button variant="outline" onClick={() => change(index, { ...hook, run: [...hook.run, ''] })}>Add argument</Button><Button variant="destructive" onClick={() => { setHooks(rows => rows.filter((_, i) => i !== index)); setDirty(true) }}>Remove hook</Button></ButtonGroup>
        </FieldSet>)}
        <p>Arguments are passed exactly as entered; shell syntax is not interpreted. Failed hooks appear in the turn’s notices.</p>
        <ButtonGroup className="settings-actions"><Button variant="outline" disabled={!editable || busy} onClick={() => { setHooks(rows => [...rows, { on: 'turn-finished', tool: null, run: [''], firesForNothing: false }]); setDirty(true) }}>Add hook</Button><Button variant="outline" disabled={busy} onClick={reloadHooks}>Reload hooks</Button><Button disabled={!dirty || busy || !editable} onClick={() => void save()}>Save hooks</Button></ButtonGroup>
      </>
    </TabsContent>
    <TabsContent value="Run settings" className="settings-body">
      {report && <>
        <Card role="region" aria-labelledby="run-override-title">
          <CardHeader><CardTitle><h3 id="run-override-title">Model and connection override</h3></CardTitle></CardHeader>
          <CardContent>
            <p>Choose a JSON settings file for this app run. It applies to future turns and model discovery, and is cleared when the app exits. Running turns keep their configuration. Terminal preferences and settings-file permission grants do not change this app’s approval controls.</p>
            <p className="settings-path font-mono">{report.selected ?? 'No override selected'}</p><ButtonGroup className="settings-actions"><Button disabled={busy} onClick={() => void select(false)}>Choose settings file…</Button><Button variant="outline" disabled={busy || !report.selected} onClick={() => void select(true)}>Clear override</Button></ButtonGroup>
          </CardContent>
          <Separator />
          <CardHeader><CardTitle><h3>Effective configuration</h3></CardTitle></CardHeader>
          <CardContent>
            <p>Default model: <strong>{report.model ?? 'Not configured'}</strong></p>
            <p>Files merge in this order: home → project → project-local → selected override. Environment and built-in values can take precedence; administrator-pinned destinations always win.</p>
            <h4>Loaded files, in order</h4>{report.layers.length ? <ol>{report.layers.map(path => <Item asChild size="sm" key={path}><li className="settings-path font-mono">{path}</li></Item>)}</ol> : <p>No settings files loaded.</p>}
            <ItemGroup>{report.overrides.map(item => <Item size="sm" key={item.name} role="listitem"><ItemContent><p><code>{item.name}</code> overridden by <span className="settings-path font-mono">{item.path}</span></p></ItemContent></Item>)}</ItemGroup>
            {report.managed.keys.length > 0 && <p>Managed values: {report.managed.keys.join(', ')}. This override cannot change them.</p>}
          </CardContent>
        </Card>
      </>}
    </TabsContent>
    </Tabs>
    <AlertDialog open={discardAction !== null} onOpenChange={(open) => { if (!open) setDiscardAction(null) }}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{discardAction === 'reload' ? 'Reload hooks without saving?' : 'Close without saving?'}</AlertDialogTitle>
          <AlertDialogDescription>{discardAction === 'reload' ? 'Discard unsaved hook changes and reload?' : 'Discard unsaved hook changes?'}</AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={busy}>Keep editing</AlertDialogCancel>
          <AlertDialogAction variant="destructive" disabled={busy} onClick={discard}>{discardAction === 'reload' ? 'Discard and reload' : 'Discard and close'}</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  </Modal>
}
