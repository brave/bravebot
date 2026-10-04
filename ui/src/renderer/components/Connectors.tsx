import { useEffect, useRef, useState } from 'react'
import { SettingsGroup } from './SettingsGroup'
import { Alert, Button, ControlItem, Icon, Input, Label, ProgressRing, SegmentedControl, type IconName } from '../nala'
import {
  CATALOG,
  STANDING_LABEL,
  drawCommand,
  expandHome,
  formOf,
  inCatalog,
  splitCommand,
  standing,
  type CatalogEntry,
  type Connector,
  type ConnectorForm,
  type ConnectorList,
  type ConnectorPreview,
  type Standing,
} from '../../shared/connectors'

const fieldText = (event: { value?: unknown; target?: EventTarget | null }): string | null => {
  if (typeof event.value === 'string') return event.value
  const target = event.target
  if (target && typeof target === 'object' && 'value' in target && typeof target.value === 'string') return target.value
  return null
}

async function call<T>(method: string, params: Record<string, unknown> = {}): Promise<T> {
  const answer = await window.bravebot.request<T>(method, params)
  if (answer.error) throw new Error(answer.error.message)
  return answer.ok as T
}

/** Where the page is. A review is a page of its own, reached from a form and returning to it. */
type Page =
  | { page: 'list' }
  | { page: 'catalog'; alias: string }
  | { page: 'custom'; alias: string }
  | { page: 'add' }
  | { page: 'review'; form: ConnectorForm; preview: ConnectorPreview; replace: boolean; back: Page }

const STANDING_COLOR: Record<Standing, 'green' | 'neutral' | 'yellow'> = {
  connected: 'green',
  off: 'neutral',
  'not-set-up': 'neutral',
  attention: 'yellow',
}

function StandingLabel({ connector }: { connector: Connector | undefined }): React.JSX.Element {
  const stands = standing(connector)
  return (
    <Label mode={stands === 'connected' ? 'loud' : 'outline'} color={STANDING_COLOR[stands]} className={`connector-standing ${stands}`} data-test="connector-standing">
      {STANDING_LABEL[stands]}
    </Label>
  )
}

/** A declaration as it stands: what it runs or reaches, what it receives, and what it may touch. */
function Declared({ connector }: { connector: Connector | ConnectorPreview }): React.JSX.Element {
  return (
    <dl className="connector-declaration" data-test="connector-declaration">
      {connector.transport === 'http' ? (
        <><dt>Reaches</dt><dd><code>{connector.url}</code></dd></>
      ) : (
        <><dt>Runs</dt><dd><code>{drawCommand(connector.command)}</code></dd></>
      )}
      {!!connector.variables?.length && (
        <><dt>Receives</dt><dd>{connector.variables.map((variable, index) => (
          <span key={variable.name}>{index > 0 && ', '}<code>{variable.name}</code>{variable.stored && ' (stored)'}</span>
        ))}</dd></>
      )}
      {!!connector.reads?.length && (
        <><dt>May read</dt><dd>{connector.reads.map((path) => <div key={path}><code>{path}</code></div>)}</dd></>
      )}
      {connector.directory && <><dt>Runs in, and may write</dt><dd><code>{connector.directory}</code></dd></>}
      {connector.digest && <><dt>Fingerprint</dt><dd><code>{connector.digest}</code></dd></>}
    </dl>
  )
}

function Warnings({ connector }: { connector: Connector }): React.JSX.Element {
  return (
    <>
      {connector.problem && <Alert type="error" role="alert">This connector cannot be used: {connector.problem}</Alert>}
      {connector.refused && <Alert type="error" role="alert">Your administrator keeps it from starting: {connector.refused}</Alert>}
      {connector.changed && <Alert type="warning" role="alert">Its setup changed since you connected it, so each conversation asks before starting it. Review and connect it again to approve the change.</Alert>}
    </>
  )
}

/** The connectors page of the settings: what is offered, what is connected, and the review before anything is. */
export function Connectors({ onBack }: { onBack: (step: (() => void) | null) => void }): React.JSX.Element {
  const [list, setList] = useState<ConnectorList | null>(null)
  const [page, setPage] = useState<Page>({ page: 'list' })
  const [problem, setProblem] = useState('')
  const [status, setStatus] = useState('')
  const [busy, setBusy] = useState(false)

  const run = async <T,>(work: () => Promise<T>): Promise<T | undefined> => {
    setBusy(true)
    setProblem('')
    try {
      return await work()
    } catch (error) {
      setProblem(error instanceof Error ? error.message : String(error))
      return undefined
    } finally {
      setBusy(false)
    }
  }
  const load = () => run(async () => setList(await call<ConnectorList>('connectors.list')))
  useEffect(() => { void load() }, [])

  const declared = (alias: string): Connector | undefined => list?.connectors.find((connector) => connector.alias === alias)
  const custom = (list?.connectors ?? []).filter((connector) => !inCatalog(connector.alias))
  const go = (next: Page) => { setProblem(''); setStatus(''); setPage(next) }

  /** Resolve a form and put what it resolves to on the review page. Nothing is written. */
  const review = (form: ConnectorForm, replace: boolean, back: Page) => run(async () => {
    const preview = await call<ConnectorPreview>('connectors.preview', { ...form })
    go({ page: 'review', form, preview, replace, back })
  })
  const connect = (form: ConnectorForm, preview: ConnectorPreview, replace: boolean) => run(async () => {
    setList(await call<ConnectorList>('connectors.connect', { ...form, fingerprint: preview.fingerprint, replace }))
    go(inCatalog(form.alias) ? { page: 'catalog', alias: form.alias } : { page: 'custom', alias: form.alias })
    setStatus(`${form.alias} is connected. New conversations start it.`)
  })
  const disconnect = (alias: string) => run(async () => {
    setList(await call<ConnectorList>('connectors.disconnect', { alias }))
    setStatus(`${alias} is disconnected. Its setup is kept, so connecting it again asks nothing new.`)
  })
  const remove = (alias: string) => run(async () => {
    if (!window.confirm(`Remove ${alias}? Its setup, any stored token and its approval are deleted.`)) return
    setList(await call<ConnectorList>('connectors.remove', { alias }))
    go({ page: 'list' })
    setStatus(`${alias} was removed.`)
  })

  const busyNow = useRef(busy)
  busyNow.current = busy
  const unavailable = list && (list.unavailable || !list.writable)
  const back = page.page === 'list' ? null : page.page === 'review' ? page.back : { page: 'list' } as Page
  // The settings view draws the arrow that goes back one page in its header.
  useEffect(() => {
    onBack(back ? () => { if (!busyNow.current) go(back) } : null)
    return () => onBack(null)
  }, [page])

  return (
    <div className="settings-body connectors">
      {(problem || status || busy || unavailable) && (
        <div className="connectors-alerts">
          {problem && <Alert type="error" role="alert" data-test="connectors-error">{problem}</Alert>}
          {status && <Alert type="success" size="small" role="status" data-test="connectors-status">{status}</Alert>}
          {busy && <p role="status" className="settings-busy"><ProgressRing mode="indeterminate" /> Working…</p>}
          {unavailable && <Alert type="warning" role="alert">{list?.unavailable ?? 'Nothing is being written in this session, so connectors cannot be changed.'}</Alert>}
        </div>
      )}

      {page.page === 'list' && list && (
        <div className="connectors-list" data-test="connectors-list">
          <SettingsGroup title="Services">
            <div className="settings-block">
              <div className="connector-grid">
                {CATALOG.map((entry) => (
                  <button key={entry.alias} type="button" className="connector-card" data-test={`connector-${entry.alias}`} onClick={() => go({ page: 'catalog', alias: entry.alias })}>
                    <Icon name={entry.icon as IconName} className="connector-icon" />
                    <span className="connector-card-text">
                      <span className="connector-name">{entry.name}</span>
                      <span className="connector-summary">{entry.summary}</span>
                    </span>
                    <StandingLabel connector={declared(entry.alias)} />
                  </button>
                ))}
              </div>
            </div>
          </SettingsGroup>

          <SettingsGroup title="Your connectors">
            <div className="settings-block">
              {custom.length === 0 ? (
                <p className="connectors-empty">No custom connectors yet. Add one by its URL, or by the command that starts it on this computer.</p>
              ) : (
                <ul className="connector-rows">
                  {custom.map((connector) => (
                    <li key={connector.alias}>
                      <button type="button" className="connector-row" data-test={`connector-${connector.alias}`} onClick={() => go({ page: 'custom', alias: connector.alias })}>
                        <Icon name="plug" className="connector-icon" />
                        <span className="connector-card-text">
                          <span className="connector-name">{connector.alias}</span>
                          <code className="connector-summary">{connector.url ?? connector.command?.join(' ') ?? connector.problem}</code>
                        </span>
                        <StandingLabel connector={connector} />
                      </button>
                    </li>
                  ))}
                </ul>
              )}
              <div className="settings-actions">
                <Button size="small" kind="outline" isDisabled={!!unavailable} onClick={() => go({ page: 'add' })} data-test="connector-add">
                  <Icon name="plus-add" slot="icon-before" />Add custom connector
                </Button>
              </div>
            </div>
          </SettingsGroup>
          <p className="connectors-foot">
            A connected server starts with each new conversation. The first time, you are asked to offer its tools to the model, and every call is put to you.
            Setup is kept in <code>~/.bravebot/mcp.json</code>, the same file <code>bravebot mcp</code> writes.
          </p>
        </div>
      )}

      {page.page === 'catalog' && list && (
        <CatalogPage
          key={page.alias}
          entry={CATALOG.find((entry) => entry.alias === page.alias)!}
          connector={declared(page.alias)}
          home={list.home}
          busy={busy || !!unavailable}
          onReview={(form) => void review(form, true, page)}
          onDisconnect={() => void disconnect(page.alias)}
          onRemove={() => void remove(page.alias)}
        />
      )}

      {page.page === 'custom' && list && (() => {
        const connector = declared(page.alias)
        if (!connector) return <p>{page.alias} is no longer declared.</p>
        return (
          <section className="connector-page" data-test="connector-page">
            <div className="connector-page-head">
              <Icon name="plug" className="connector-icon" />
              <h3>{connector.alias}</h3>
              <StandingLabel connector={connector} />
            </div>
            <Warnings connector={connector} />
            {!connector.problem && <Declared connector={connector} />}
            <div className="connector-page-actions">
              {connector.connected ? (
                <Button size="small" kind="outline" isDisabled={busy || !!unavailable} onClick={() => void disconnect(connector.alias)} data-test="connector-disconnect">Disconnect</Button>
              ) : !connector.problem && (
                <Button size="small" kind="filled" isDisabled={busy || !!unavailable} onClick={() => void review(formOf(connector), true, page)} data-test="connector-review">Connect…</Button>
              )}
              <Button size="small" kind="plain-faint" isDisabled={busy || !!unavailable} onClick={() => void remove(connector.alias)} data-test="connector-remove">Remove</Button>
            </div>
          </section>
        )
      })()}

      {page.page === 'add' && list && (
        <AddCustom home={list.home} busy={busy || !!unavailable} onReview={(form) => void review(form, false, page)} />
      )}

      {page.page === 'review' && (
        <section className="connector-page connector-review" data-test="connector-review-page">
          <div className="connector-page-head">
            <Icon name={(CATALOG.find((entry) => entry.alias === page.form.alias)?.icon ?? 'plug') as IconName} className="connector-icon" />
            <h3>Connect {page.form.alias}?</h3>
          </div>
          <p>Check that this is what you mean to run or reach. Connecting declares and approves exactly this, and every new conversation starts it.</p>
          <Declared connector={page.preview} />
          {page.preview.fetching.length > 0 && (
            <Alert type="warning" role="alert">
              <ul className="connector-warnings">{page.preview.fetching.map((line, index) => <li key={index}>{line}</li>)}</ul>
              Approving it approves whatever it fetches each time it starts.
            </Alert>
          )}
          {page.preview.refused && <Alert type="error" role="alert">Your administrator keeps it from starting: {page.preview.refused}</Alert>}
          {page.preview.exists && !page.preview.same && !page.replace && <Alert type="error" role="alert">A connector named {page.form.alias} exists already. Go back and choose another name.</Alert>}
          {page.preview.exists && !page.preview.same && page.replace && <Alert type="info">This replaces the current setup of {page.form.alias}.</Alert>}
          <p className="connectors-foot">
            {page.preview.transport === 'http'
              ? 'Every request to it goes through the same network checks as any other. '
              : 'It runs confined: it reads and writes its own directory, receives only the variables above, and starts with none of this app’s environment. '}
            Each conversation still asks before offering its tools to the model, and before every call.
          </p>
          <div className="connector-page-actions">
            <Button size="small" kind="filled" isDisabled={busy || !!page.preview.refused || (page.preview.exists && !page.preview.same && !page.replace)}
              onClick={() => void connect(page.form, page.preview, page.replace)} data-test="connector-connect">
              Connect
            </Button>
          </div>
        </section>
      )}
    </div>
  )
}

/** A catalog connector's page: how it stands, its setup, and the form that sets it up or changes it. */
function CatalogPage({ entry, connector, home, busy, onReview, onDisconnect, onRemove }: {
  entry: CatalogEntry
  connector: Connector | undefined
  home: string | null
  busy: boolean
  onReview: (form: ConnectorForm) => void
  onDisconnect: () => void
  onRemove: () => void
}): React.JSX.Element {
  const set = connector && !connector.problem
  const [values, setValues] = useState<Record<string, string>>(() => ({
    ...Object.fromEntries(entry.fields.map((field) => [field.key, field.initial ?? ''])),
    ...(set ? entry.values(connector) : {}),
  }))
  const stored = new Set((connector?.variables ?? []).filter((variable) => variable.stored).map((variable) => variable.name))
  const keepable = (key: string) => set && entry.fields.some((field) => field.key === key && field.kind === 'secret') && stored.size > 0
  const missing = entry.fields.some((field) => !values[field.key]?.trim() && !(field.kind === 'secret' && keepable(field.key)))

  const submit = () => {
    const expanded = Object.fromEntries(entry.fields.map((field) => [
      field.key,
      field.kind === 'path' ? expandHome(values[field.key]!.trim(), home) : values[field.key]!.trim(),
    ]))
    const kept = new Set(entry.fields.filter((field) => field.kind === 'secret' && !values[field.key]?.trim() && keepable(field.key)).map((field) => field.key))
    onReview(entry.build(expanded, kept))
  }

  return (
    <section className="connector-page" data-test="connector-page">
      <div className="connector-page-head">
        <Icon name={entry.icon as IconName} className="connector-icon" />
        <h3>{entry.name}</h3>
        <StandingLabel connector={connector} />
      </div>
      <p>{entry.summary}</p>
      {connector && <Warnings connector={connector} />}

      {set && (
        <>
          <h4>Current setup</h4>
          <Declared connector={connector} />
          <div className="connector-page-actions">
            {connector.connected && <Button size="small" kind="outline" isDisabled={busy} onClick={onDisconnect} data-test="connector-disconnect">Disconnect</Button>}
            <Button size="small" kind="plain-faint" isDisabled={busy} onClick={onRemove} data-test="connector-remove">Remove</Button>
          </div>
        </>
      )}

      <h4>{set ? 'Change setup' : 'Set up'}</h4>
      {!set && (
        <p className="connector-before">
          {entry.before} <a href={entry.guide} target="_blank" rel="noreferrer">Read the setup guide</a>
        </p>
      )}
      <form className="connector-form" onSubmit={(event) => { event.preventDefault(); if (!missing && !busy) submit() }}>
        {entry.fields.map((field) => (
          <div key={field.key} className="connector-field">
            <Input
              type={field.kind === 'secret' ? 'password' : 'text'}
              value={values[field.key] ?? ''}
              placeholder={field.kind === 'secret' && keepable(field.key) ? 'Leave blank to keep the stored value' : field.initial}
              disabled={busy}
              data-test={`connector-field-${field.key}`}
              onInput={(event) => { const value = fieldText(event); if (value !== null) setValues((old) => ({ ...old, [field.key]: value })) }}
            >{field.label}</Input>
            <small>{field.help}</small>
          </div>
        ))}
        <div className="connector-page-actions">
          <Button size="small" kind="filled" isDisabled={busy || missing} onClick={submit} data-test="connector-review">
            {set ? 'Review changes…' : 'Review and connect…'}
          </Button>
        </div>
      </form>
    </section>
  )
}

/** The form for a connector of the person's own: a URL, or a command that starts it here. */
function AddCustom({ home, busy, onReview }: { home: string | null; busy: boolean; onReview: (form: ConnectorForm) => void }): React.JSX.Element {
  const [alias, setAlias] = useState('')
  const [transport, setTransport] = useState<'http' | 'stdio'>('http')
  const [url, setUrl] = useState('')
  const [line, setLine] = useState('')
  const [directory, setDirectory] = useState('')
  const [variables, setVariables] = useState<{ name: string; value: string }[]>([])
  const command = splitCommand(line).map((word, index) => (index === 0 ? expandHome(word, home) : word))
  const missing = !alias.trim() || (transport === 'http' ? !url.trim() : command.length === 0)

  const submit = () => onReview(transport === 'http'
    ? { alias: alias.trim(), transport, url: url.trim() }
    : {
        alias: alias.trim(),
        transport,
        command,
        // An empty value is a name read from the environment when it starts.
        variables: variables.filter((variable) => variable.name.trim()).map((variable) => (
          variable.value === '' ? { name: variable.name.trim() } : { name: variable.name.trim(), value: variable.value })),
        directory: directory.trim() ? expandHome(directory.trim(), home) : undefined,
      })
  const text = (set: (value: string) => void) => (event: { value?: unknown; target?: EventTarget | null }) => {
    const value = fieldText(event)
    if (value !== null) set(value)
  }

  return (
    <section className="connector-page" data-test="connector-add-page">
      <div className="connector-page-head">
        <Icon name="plug" className="connector-icon" />
        <h3>Add custom connector</h3>
      </div>
      <form className="connector-form" onSubmit={(event) => { event.preventDefault(); if (!missing && !busy) submit() }}>
        <div className="connector-field">
          <Input value={alias} placeholder="weather" disabled={busy} data-test="connector-alias" onInput={text(setAlias)}>Name</Input>
          <small>Letters, digits, - and _. Its tools are offered to the model under this name.</small>
        </div>
        <div className="connector-field">
          <SegmentedControl value={transport} size="small" aria-label="How it is reached" data-test="connector-transport"
            onChange={(detail) => { if (detail.value === 'http' || detail.value === 'stdio') setTransport(detail.value) }}>
            <ControlItem value="http">URL</ControlItem>
            <ControlItem value="stdio">Local command</ControlItem>
          </SegmentedControl>
        </div>
        {transport === 'http' ? (
          <div className="connector-field">
            <Input value={url} placeholder="https://example.com/mcp" disabled={busy} data-test="connector-url" onInput={text(setUrl)}>Server URL</Input>
            <small>An http or https address. A sign-in or token in the address is refused.</small>
          </div>
        ) : (
          <>
            <div className="connector-field">
              <Input value={line} placeholder="npx -y some-mcp-server@1.2.3" disabled={busy} data-test="connector-command" onInput={text(setLine)}>Command</Input>
              <small>The program and its arguments, run as written and never through a shell. Quote a word that holds a space.</small>
            </div>
            <fieldset className="connector-variables">
              <legend>Variables</legend>
              {variables.map((variable, index) => (
                <div key={index} className="connector-variable">
                  <Input value={variable.name} placeholder="API_KEY" disabled={busy} data-test={`connector-variable-name-${index}`}
                    onInput={text((value) => setVariables((rows) => rows.map((row, at) => (at === index ? { ...row, name: value } : row))))}>Name</Input>
                  <Input type="password" value={variable.value} placeholder="Read from the environment" disabled={busy} data-test={`connector-variable-value-${index}`}
                    onInput={text((value) => setVariables((rows) => rows.map((row, at) => (at === index ? { ...row, value } : row))))}>Value</Input>
                  <Button size="small" kind="plain-faint" isDisabled={busy} onClick={() => setVariables((rows) => rows.filter((_, at) => at !== index))} aria-label={`Remove variable ${index + 1}`}>
                    <Icon name="trash" />
                  </Button>
                </div>
              ))}
              <Button size="small" kind="plain" isDisabled={busy} onClick={() => setVariables((rows) => [...rows, { name: '', value: '' }])} data-test="connector-variable-add">
                <Icon name="plus-add" slot="icon-before" />Add variable
              </Button>
              <small>A value is stored in ~/.bravebot/mcp.json and handed to this server alone. Leave it blank to pass the variable from your environment when it starts. The server receives nothing else from it.</small>
            </fieldset>
            <div className="connector-field">
              <Input value={directory} placeholder="Optional" disabled={busy} data-test="connector-directory" onInput={text(setDirectory)}>Directory it runs in</Input>
              <small>A directory it may write, such as where it keeps a token. Without one it starts in a temporary directory.</small>
            </div>
          </>
        )}
        <div className="connector-page-actions">
          <Button size="small" kind="filled" isDisabled={busy || missing} onClick={submit} data-test="connector-review">Review and connect…</Button>
        </div>
      </form>
    </section>
  )
}
