import { useEffect, useState } from 'react'
import type { AgentSettings as Report } from '../../shared/agent-settings'
import { useDelayedFlag } from '../hooks'
import { t } from '../i18n'
import { Alert, Button, Collapse } from '../nala'
import { Accounts } from './Accounts'
import { SettingsGroup } from './SettingsGroup'
import { Skeleton } from './Skeleton'

/** The ways to a model service: the sign-ins under Accounts first, then the ones made in settings. */
export function SetupRoutes(): React.JSX.Element {
  return <div className="setup-routes"><section><h4>{t.uiAccountsSetupHeading()}</h4><p data-test="settings-setup-accounts">{t.uiAccountsSetupPointer()}</p></section>
    <section><h4>A local model or another service</h4><p>Add a provider to your agent settings, or select a file in Run settings. Credential-free local services need no token.</p><pre>{'{"provider":{"local":{"options":{"baseURL":"http://localhost:11434/v1"},"models":{"your-model":{}}}},"model":"local/your-model"}'}</pre><p data-test="settings-setup-gateway-key">For a service that needs a key, enter it on its row under Accounts, or name the credential environment variable in the provider’s <code>env</code> array.</p></section>
    <section><h4>AWS Bedrock</h4><p>Name your AWS profile, region and model in your agent settings or environment. Your AWS account must have access to the selected model.</p><pre>AWS_REGION=us-east-1{'\n'}AWS_PROFILE=your-profile{'\n'}ANTHROPIC_DEFAULT_SONNET_MODEL=your-model-id-or-inference-profile-arn</pre><p data-test="settings-setup-bedrock">Then sign in to the profile under Accounts, which turns Bedrock on. Where the profile needs no sign-in, set <code>BRAVEBOT_USE_BEDROCK=1</code> as well.</p></section></div>
}

/** What a provider block's credential stands at, as `settings.inspect` words it. */
function credentialText(credential: string): string {
  if (credential === 'configured') return 'Credentials configured'
  if (credential === 'missing') return 'Credentials missing'
  if (credential === 'not required') return 'Credentials not required'
  return credential
}

/**
 * The model services: the accounts this app can sign in to with what each offers the model picker,
 * then which service answers and what each configured provider is missing, and the ways to set one up.
 *
 * `onChanged` is called when a sign-in, a key or a sign-out changes what the agent can reach.
 */
export function ProvidersSettings({ session, onChanged }: { session?: string; onChanged: () => void }): React.JSX.Element {
  const [report, setReport] = useState<Report | null>(null)
  const [problem, setProblem] = useState('')
  const [busy, setBusy] = useState(false)
  const showLoading = useDelayedFlag(busy && !report)
  const [pick, setPick] = useState<{ name: string; asked: number } | null>(null)
  const load = async () => {
    setBusy(true); setProblem('')
    try {
      const result = await window.bravebot.request<Report>('settings.inspect', { session })
      if (result.error) throw new Error(result.error.message)
      if (result.ok) {
        if (!Array.isArray(result.ok.providers)) throw new Error('This agent did not return configuration details. Rebuild or reinstall the app, then retry.')
        setReport(result.ok)
      }
    } catch (error) { setProblem(String(error)) } finally { setBusy(false) }
  }
  useEffect(() => { void load() }, [session])
  return <div className="providers-settings settings-body" data-test="settings-body">
    {problem && <div className="settings-error"><Alert type="error" role="alert" data-test="settings-error">{problem}</Alert><Button size="small" kind="outline" isDisabled={busy} onClick={() => void load()}>Retry diagnostics</Button></div>}
    {busy && !report && <Skeleton label="Loading the model service…" kind="lines" count={5} delayed={!showLoading} />}
    <SettingsGroup title={t.uiAccountsHeading()}>
      <Accounts session={session} refresh={0} onChanged={() => { onChanged(); void load() }} pick={pick} />
    </SettingsGroup>
    {report && <SettingsGroup title={report.configured ? 'Model service configured' : 'Choose a model service'}>
      <section className="settings-block settings-status" data-test="settings-status">
        {report.problem && <p>{report.problem}</p>}
        <dl>
          <dt>Agent build</dt><dd>{report.build}</dd>
          <dt>Default model</dt><dd>{report.model ?? 'Not configured'}</dd>
          {report.providers.map((provider) => <div className="settings-dl-row" key={provider.name} data-test="settings-providers">
            <dt>{provider.name}</dt>
            <dd><span>{credentialText(provider.credential)}</span>
              {provider.credential === 'missing' && <Button size="tiny" kind="plain" className="settings-provider-fix" onClick={() => setPick({ name: provider.name, asked: Date.now() })} data-test="settings-provider-fix">Fix</Button>}
            </dd>
          </div>)}
          {report.brave && <><dt>Brave service</dt><dd>Brave service enabled</dd></>}
          {report.bedrock && <><dt>AWS Bedrock</dt><dd>AWS Bedrock enabled</dd></>}
        </dl>
        <Collapse title="Set up a model service" isOpen={!report.configured ? true : undefined} data-test="settings-setup">
          <SetupRoutes />
        </Collapse>
      </section>
    </SettingsGroup>}
  </div>
}
