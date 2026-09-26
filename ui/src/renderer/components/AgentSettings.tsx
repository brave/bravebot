import { useEffect, useState } from 'react'
import { XIcon } from 'lucide-react'
import { SCRIM } from '@/lib/utils'
import {
  Alert,
  AlertAction,
  AlertDescription,
} from '@/components/ui/alert'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { Button } from '@/components/ui/button'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import {
  Dialog,
  DialogContent,
  DialogTitle,
} from '@/components/ui/dialog'
import {
  Field,
  FieldGroup,
  FieldLabel,
  FieldLegend,
  FieldSet,
} from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import {
  NativeSelect,
  NativeSelectOption,
} from '@/components/ui/native-select'
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from '@/components/ui/tabs'
import type { AgentSettings as Report, Hook, HooksDocument } from '../../shared/agent-settings'
import { composeHooks } from '../../shared/agent-settings'

const TABS = ['Connection', 'Hooks', 'Run settings'] as const
type Tab = (typeof TABS)[number]

/**
 * The shape every section of this panel is read in: a rule under each section, a two-column
 * definition list for the facts, and a monospaced block for anything quoted out of a file.
 * Stated once here rather than on each of the three tab bodies, which would let them drift.
 *
 * Below 640px the definition lists stack. Two columns in that width leaves a settings path with
 * about three characters to the line, which is not reading it so much as spelling it.
 */
const BODY = [
  'settings-body',
  '[&_section]:border-b [&_section]:border-border [&_section]:py-3',
  '[&_h3]:my-2 [&_h3]:text-base [&_h3]:font-semibold [&_h4]:font-semibold',
  '[&_p]:leading-normal',
  '[&_ol]:list-decimal [&_ol]:pl-5',
  '[&_dl]:grid [&_dl]:grid-cols-[140px_1fr] [&_dl]:gap-x-4 [&_dl]:gap-y-2',
  '[&_dt]:text-muted-foreground [&_dd]:m-0 [&_dd]:wrap-anywhere',
  '[&_pre]:rounded-sm [&_pre]:bg-background [&_pre]:p-2.5 [&_pre]:text-xs [&_pre]:wrap-anywhere [&_pre]:whitespace-pre-wrap',
  'max-[640px]:[&_dl]:grid-cols-[1fr] max-[640px]:[&_dl]:gap-[5px] max-[640px]:[&_dd]:mb-2.5',
].join(' ')

/** A path or a file name, wherever this panel prints one. */
const PATH = 'settings-path font-mono text-xs wrap-anywhere'

/** A row of buttons at the end of a section, wrapping rather than overflowing a narrow window. */
const ACTIONS = 'settings-actions mt-3.5 mb-1 flex flex-wrap gap-2'

/** The chosen tab is named in the accent rather than filled, so the strip stays a strip. */
const TAB_TRIGGER = [
  'flex-none data-active:border-primary data-active:bg-transparent data-active:text-primary',
  'group-data-[variant=default]/tabs-list:data-active:shadow-none',
  'dark:data-active:border-primary dark:data-active:bg-transparent',
].join(' ')

export function AgentSettings({ session, onClose, onChanged }: { session?: string; onClose: () => void; onChanged: () => void }): React.JSX.Element {
  const [tab, setTab] = useState<Tab>('Connection')
  const [report, setReport] = useState<Report | null>(null)
  const [document, setDocument] = useState<HooksDocument | null>(null)
  const [hooks, setHooks] = useState<Hook[]>([])
  const [problem, setProblem] = useState('')
  const [status, setStatus] = useState('')
  const [busy, setBusy] = useState(false)
  const [dirty, setDirty] = useState(false)
  const [discard, setDiscard] = useState<'close' | 'reload' | null>(null)
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
  const close = () => { if (!dirty) onClose(); else setDiscard('close') }
  const confirmDiscard = () => {
    const next = discard
    setDiscard(null)
    if (next === 'close') onClose()
    else if (next === 'reload') void loadHooks()
  }
  // A file the agent passed over in part is the person's to edit: composing it back from the
  // entries it did read would drop the rest.
  const editable = !!document && document.entire
  return (
    <>
    <Dialog open onOpenChange={(next) => { if (!next && !busy) close() }}>
      <DialogContent
        className="modal agent-settings max-h-[calc(100dvh-40px)] w-[min(760px,calc(100vw-32px))] overflow-auto sm:max-w-none"
        showCloseButton={false}
        overlayClassName={SCRIM}
      >
        <DialogTitle className="sr-only">Agent settings</DialogTitle>
        <div className="settings-heading flex items-start justify-between gap-5">
          <div>
            <h2 className="mb-2 text-xl font-semibold">Agent settings</h2>
            <p className="mt-0 text-muted-foreground">Configuration and automation for this app.</p>
          </div>
          <Button variant="ghost" size="icon-sm" onClick={close} disabled={busy} aria-label="Close agent settings">
            <XIcon />
          </Button>
        </div>
        <Tabs
          value={tab}
          onValueChange={(value) => { setTab(value as Tab); setProblem(''); setStatus('') }}
          className="gap-0"
        >
          <TabsList
            className="settings-tabs my-3 w-full justify-start gap-1.5 rounded-none border-b border-border bg-transparent p-0 pb-2.5 group-data-horizontal/tabs:h-auto max-[640px]:flex-wrap"
            aria-label="Agent settings sections"
          >
            {TABS.map((name) => (
              <TabsTrigger key={name} value={name} className={TAB_TRIGGER}>
                {name}{name === 'Hooks' && dirty ? ' •' : ''}
              </TabsTrigger>
            ))}
          </TabsList>
          {problem && (
            <Alert variant="destructive" className="settings-error border-l-[3px] border-l-primary pl-3">
              <AlertDescription>{problem}</AlertDescription>
              {tab !== 'Hooks' && (
                <AlertAction>
                  <Button size="sm" variant="outline" disabled={busy} onClick={() => void load()}>Retry diagnostics</Button>
                </AlertAction>
              )}
            </Alert>
          )}
          {status && <p role="status">{status}</p>}
          {busy && <p role="status">Working…</p>}
          <TabsContent value="Connection" className={BODY}>
            {report && <>
              <section>
                <h3>{report.configured ? 'Model service configured' : 'Choose a model service'}</h3>
                {report.problem && <p>{report.problem}</p>}
                <dl><dt>Agent build</dt><dd>{report.build}</dd><dt>Default model</dt><dd>{report.model ?? 'Not configured'}</dd></dl>
                {report.providers.map((p, i) => <p key={i}><strong>{p.name}</strong> · Credential {p.credential}</p>)}
                {report.brave && <p>Brave service enabled</p>}{report.bedrock && <p>AWS Bedrock enabled</p>}
              </section>
              <Collapsible defaultOpen={!report.configured}>
                <CollapsibleTrigger className="settings-setup-trigger">Set up a model service</CollapsibleTrigger>
                <CollapsibleContent>
                  <div className="setup-routes">
                    <section>
                      <h4>Gateway or local model</h4>
                      <p>Add a provider to your agent settings, or select a file in Run settings. Credential-free local services need no token.</p>
                      <pre>{'{"provider":{"local":{"options":{"baseURL":"http://localhost:11434/v1"},"models":{"your-model":{}}}},"model":"local/your-model"}'}</pre>
                      <p>For authenticated services, name the credential environment variable in the provider’s <code>env</code> array.</p>
                    </section>
                    <section>
                      <h4>AWS Bedrock</h4>
                      <p>Configure your AWS credentials and enable Bedrock in your agent settings or environment. Your AWS account must have access to the selected model.</p>
                      <pre>BRAVEBOT_USE_BEDROCK=1{'\n'}AWS_REGION=us-east-1{'\n'}AWS_PROFILE=your-profile{'\n'}ANTHROPIC_DEFAULT_SONNET_MODEL=your-model-id-or-inference-profile-arn</pre>
                    </section>
                    <section>
                      <h4>Brave service</h4>
                      <p>Use a configured Brave build, or launch from a shell with <code>SERVICES_KEY_AICHAT</code>, <code>BRAVE_SERVICES_KEY_ID</code> and <code>BRAVE_AI_CHAT_ENDPOINT</code> set to your supplied credentials and endpoint. Credentials are not stored in this window.</p>
                    </section>
                  </div>
                </CollapsibleContent>
              </Collapsible>
              <section>
                <h3>Network</h3>
                <dl>
                  <dt>Certificate roots</dt><dd>{report.network.roots.length ? report.network.roots.join(', ') : 'Bundled roots'}</dd>
                  <dt>Proxy</dt><dd>{report.network.proxy ?? 'No proxy configured'}{report.network.authenticated ? ' · authenticated' : ''}</dd>
                  <dt>Proxy bypass</dt><dd>{report.network.noProxy ?? 'None'}</dd>
                </dl>
                {(report.network.problem || report.network.trustsNothing || report.network.unusableProxy) && (
                  <Alert variant="destructive">
                    <AlertDescription>{report.network.problem || (report.network.trustsNothing ? 'No usable trust roots.' : `Unsupported proxy: ${report.network.unusableProxy}`)}</AlertDescription>
                  </Alert>
                )}
                <p>For a custom certificate authority, set <code>SSL_CERT_FILE</code> or <code>SSL_CERT_DIR</code> before opening the app. These replace bundled roots. Proxy and certificate changes require restarting the app.</p>
              </section>
              <section>
                <h3>Managed configuration</h3>
                {report.managed.path ? <>
                  <p>{report.managed.path}</p>
                  <p>{report.managed.keys.length ? `Locked by your administrator: ${report.managed.keys.join(', ')}` : 'File found; no recognized values are pinned.'}</p>
                </> : <p>No administrator-managed configuration found.</p>}
                <p>Administrator-pinned destinations take precedence over your settings and environment.</p>
              </section>
              <Button onClick={() => void load()} disabled={busy}>Refresh diagnostics</Button>
            </>}
          </TabsContent>
          <TabsContent value="Hooks" className={BODY}>
            <p>Hooks are shared with the terminal client. Run your own programs at specific moments. These commands run with your account’s permissions in the project directory. They cannot approve or block the agent.</p>
            {document && <p className={PATH}>{document.path}</p>}
            {document && !document.entire && (
              <Alert variant="destructive">
                <AlertDescription>{document.text === null
                  ? 'The agent could not read this file, so it declares no hooks. Open it yourself to see why.'
                  : 'The agent did not read all of this file, so saving it from here could drop what it passed over. Edit it directly.'}</AlertDescription>
              </Alert>
            )}
            {document && document.entire && hooks.length === 0 && <p>No hooks configured.</p>}
            {hooks.map((hook, index) => (
              <FieldSet key={index} disabled={busy || !editable} className="hook-editor my-4 rounded-[10px] border border-border p-4">
                <FieldLegend>Hook {index + 1}</FieldLegend>
                <FieldGroup>
                  <Field>
                    <FieldLabel htmlFor={`hook-on-${index}`}>When</FieldLabel>
                    <NativeSelect
                      id={`hook-on-${index}`}
                      value={hook.on}
                      onChange={e => change(index, { ...hook, on: e.target.value as Hook['on'] })}
                    >
                      <NativeSelectOption value="turn-started">Turn starts</NativeSelectOption>
                      <NativeSelectOption value="tool-finished">Tool finishes</NativeSelectOption>
                      <NativeSelectOption value="turn-finished">Turn ends</NativeSelectOption>
                    </NativeSelect>
                  </Field>
                  {(hook.on === 'tool-finished' || hook.tool !== null) && (
                    <Field>
                      <FieldLabel htmlFor={`hook-tool-${index}`}>Tool filter (optional)</FieldLabel>
                      <Input
                        id={`hook-tool-${index}`}
                        value={hook.tool ?? ''}
                        placeholder="All tools"
                        onChange={e => change(index, { ...hook, tool: e.target.value.trim() || null })}
                      />
                    </Field>
                  )}
                  {!dirty && document?.hooks[index]?.firesForNothing && (
                    <Alert variant="destructive">
                      <AlertDescription>This hook fires for nothing: only a finished tool call carries a tool name. Clear the filter, or choose Tool finishes.</AlertDescription>
                    </Alert>
                  )}
                  <Field>
                    <FieldLabel htmlFor={`hook-program-${index}`}>Program</FieldLabel>
                    <Input
                      id={`hook-program-${index}`}
                      value={hook.run[0]}
                      placeholder="/path/to/program"
                      onChange={e => change(index, { ...hook, run: [e.target.value, ...hook.run.slice(1)] })}
                    />
                  </Field>
                  {hook.run.slice(1).map((argument, i) => (
                    <div key={i} className="hook-argument flex items-center gap-2">
                      <Field className="flex-1">
                        <FieldLabel htmlFor={`hook-arg-${index}-${i}`}>Argument {i + 1}</FieldLabel>
                        <Input
                          id={`hook-arg-${index}-${i}`}
                          value={argument}
                          onChange={e => change(index, { ...hook, run: hook.run.map((word, j) => j === i + 1 ? e.target.value : word) })}
                        />
                      </Field>
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={() => change(index, { ...hook, run: hook.run.filter((_, j) => j !== i + 1) })}
                        aria-label={`Remove argument ${i + 1} from hook ${index + 1}`}
                      >
                        Remove
                      </Button>
                    </div>
                  ))}
                  <div className={ACTIONS}>
                    <Button variant="outline" size="sm" onClick={() => change(index, { ...hook, run: [...hook.run, ''] })}>Add argument</Button>
                    <Button variant="outline" size="sm" onClick={() => { setHooks(rows => rows.filter((_, i) => i !== index)); setDirty(true) }}>Remove hook</Button>
                  </div>
                </FieldGroup>
              </FieldSet>
            ))}
            <p>Arguments are passed exactly as entered; shell syntax is not interpreted. Failed hooks appear in the turn’s notices.</p>
            <div className={ACTIONS}>
              <Button
                disabled={!editable || busy}
                onClick={() => { setHooks(rows => [...rows, { on: 'turn-finished', tool: null, run: [''], firesForNothing: false }]); setDirty(true) }}
              >
                Add hook
              </Button>
              <Button
                variant="outline"
                disabled={busy}
                onClick={() => { if (!dirty) void loadHooks(); else setDiscard('reload') }}
              >
                Reload hooks
              </Button>
              <Button disabled={!dirty || busy || !editable} onClick={() => void save()}>Save hooks</Button>
            </div>
          </TabsContent>
          <TabsContent value="Run settings" className={BODY}>
            {report && <>
              <h3>Model and connection override</h3>
              <p>Choose a JSON settings file for this app run. It applies to future turns and model discovery, and is cleared when the app exits. Running turns keep their configuration. Terminal preferences and settings-file permission grants do not change this app’s approval controls.</p>
              <p className={PATH}>{report.selected ?? 'No override selected'}</p>
              <div className={ACTIONS}>
                <Button disabled={busy} onClick={() => void select(false)}>Choose settings file…</Button>
                <Button variant="outline" disabled={busy || !report.selected} onClick={() => void select(true)}>Clear override</Button>
              </div>
              <h3>Effective configuration</h3>
              <p>Default model: <strong>{report.model ?? 'Not configured'}</strong></p>
              <p>Files merge in this order: home → project → project-local → selected override. Environment and built-in values can take precedence; administrator-pinned destinations always win.</p>
              <h4>Loaded files, in order</h4>
              {report.layers.length ? <ol>{report.layers.map(path => <li key={path} className={PATH}>{path}</li>)}</ol> : <p>No settings files loaded.</p>}
              {report.overrides.map(item => <p key={item.name}><code>{item.name}</code> overridden by <span className={PATH}>{item.path}</span></p>)}
              {report.managed.keys.length > 0 && <p>Managed values: {report.managed.keys.join(', ')}. This override cannot change them.</p>}
            </>}
          </TabsContent>
        </Tabs>
      </DialogContent>
    </Dialog>
    <AlertDialog open={discard !== null} onOpenChange={(open) => { if (!open) setDiscard(null) }}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Discard unsaved hook changes?</AlertDialogTitle>
          <AlertDialogDescription>
            {discard === 'reload'
              ? 'Reload will discard unsaved hook changes and read the file again.'
              : 'Closing will discard unsaved hook changes.'}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Keep editing</AlertDialogCancel>
          <AlertDialogAction variant="destructive" onClick={confirmDiscard}>Discard</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
    </>
  )
}
