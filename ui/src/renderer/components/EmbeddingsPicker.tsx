import { useEffect, useState } from 'react'
import type {
  EmbeddingsChoice, EmbeddingsModel, EmbeddingsOption, EmbeddingsService, EmbeddingsSetAnswer, EmbeddingsSetParams,
  EmbeddingsTest, Retrieval,
} from '../../shared/agent-settings'
import { useDelayedFlag } from '../hooks'
import { Alert, Button, Collapse, Dropdown, Input, ProgressRing, RadioButton } from '../nala'
import { RecallDisclosure } from './RecallSetting'
import { Skeleton } from './Skeleton'

export const SERVICE_LABELS: Record<EmbeddingsService, string> = {
  openai: 'OpenAI', brave: 'Brave', openrouter: 'OpenRouter', custom: 'Custom endpoint',
}

/** The line under a service's name. */
export function serviceDetail(option: EmbeddingsOption, development: boolean): string {
  switch (option.service) {
    case 'openai': return 'Uses your OpenAI key.'
    case 'openrouter': return 'Uses your OpenRouter key.'
    case 'custom': return 'Any OpenAI-compatible /embeddings URL.'
    default: return development ? 'Brave’s embeddings service (development)' : 'Brave’s embeddings service'
  }
}

/** The services listed: those that can be chosen, and the selected one even where it no longer can. */
export function shownOptions(choice: EmbeddingsChoice): EmbeddingsOption[] {
  return choice.options.filter(option => option.available || option.service === choice.selected)
}

/** The radio that reads as chosen: one picked here and not written yet, else the one the settings name or use. */
export function currentService(choice: EmbeddingsChoice, pending: EmbeddingsService | null): EmbeddingsService | null {
  return pending ?? choice.selected ?? choice.inForce
}

/** Whether the option has what it needs to be sent to, so selecting it can be tested. */
export function complete(option: EmbeddingsOption): boolean {
  if (option.service === 'openrouter') return !!option.model
  if (option.service === 'custom') return !!option.model && /^https?:\/\/./.test(option.url ?? '')
  return true
}

/** Choosing a radio: written at once, or, while recall is on, kept here until the option is complete. */
export function chooseStep(state: Retrieval, option: EmbeddingsOption): { kind: 'pending' } | { kind: 'send'; params: EmbeddingsSetParams } {
  if (state.enabled && !complete(option)) return { kind: 'pending' }
  return { kind: 'send', params: { select: option.service } }
}

/** A refused write: under the option kept as it was when it was the one changed, else under the group. */
export function refusalStep(service: EmbeddingsService, before: EmbeddingsService | null, message: string):
  { kind: 'inline'; message: string } | { kind: 'problem'; text: string } {
  if (service !== before) return { kind: 'problem', text: `The embeddings service was not changed: ${message}.` }
  const failed = message.lastIndexOf('test call failed: ')
  return { kind: 'inline', message: failed < 0 ? message : `the test failed: ${message.slice(failed + 'test call failed: '.length)}` }
}

/** OpenRouter's price per input token as dollars per million tokens. */
export function priceText(prompt: string | null): string | null {
  if (prompt === null) return null
  const perMillion = Number(prompt) * 1e6
  if (!Number.isFinite(perMillion) || prompt.trim() === '') return null
  if (perMillion === 0) return 'free'
  return `$${Number(perMillion.toPrecision(2))}/M tokens`
}

/** A model as its menu row reads: name, context length and price. */
export function modelText(model: EmbeddingsModel): string {
  const context = model.contextLength === null ? null
    : model.contextLength >= 1000 ? `${Math.round(model.contextLength / 1000)}k context` : `${model.contextLength} tokens context`
  return [model.name || model.id, context, priceText(model.pricing.prompt)].filter(Boolean).join(' · ')
}

export function sortedModels(models: readonly EmbeddingsModel[]): EmbeddingsModel[] {
  return [...models].sort((a, b) => (a.name || a.id).localeCompare(b.name || b.id))
}

/** What a test call's outcome reads as, under the option it tested; `saved` where the same request wrote the choice. */
export function testText(test: EmbeddingsTest, option: EmbeddingsOption, saved: boolean): string {
  const host = option.host ?? 'the endpoint'
  if (test.ok) return `${saved ? 'Saved and tested' : 'Tested'}. ${host} answered in ${test.millis} ms; each message becomes ${test.dimensions} numbers.`
  const why = failureText(test, option, host)
  return saved ? `Saved, but the test failed: ${why}` : why
}

function failureText(test: Extract<EmbeddingsTest, { ok: false }>, option: EmbeddingsOption, host: string): string {
  switch (test.kind) {
    case 'key': return `${host} refused the key. Check it and try again.`
    case 'not-found': return `Nothing answered at ${option.url ?? host} for ${option.model ?? 'that model'} (404). Check the URL and model.`
    case 'shape': return `${host} answered, but not with embeddings. Check the URL ends in /embeddings.`
    case 'network': return `Couldn’t reach ${host}: ${test.message}.`
    default: return `The test failed: ${test.message}.`
  }
}

/** What fixes the choice, as the group says it. */
export function choiceLockedText(choice: EmbeddingsChoice): string | null {
  if (!choice.locked) return null
  return choice.locked.by === 'managed' ? 'Set by your administrator' : `Set by ${choice.locked.path ?? 'the selected settings file'} for this run`
}

type Models = { status: 'idle' } | { status: 'loading' } | { status: 'ready'; models: EmbeddingsModel[] } | { status: 'error'; message: string }

/** The custom endpoint's fields as typed, before Save and test. */
type Draft = { url: string; model: string; queryPrefix: string; passagePrefix: string; key: string; replacing: boolean }

const draftOf = (option: EmbeddingsOption | undefined): Draft => ({
  url: option?.url ?? '', model: option?.model ?? '', queryPrefix: option?.queryPrefix ?? '', passagePrefix: option?.passagePrefix ?? '',
  key: '', replacing: false,
})

/** Whether the custom fields as typed can be saved. */
export function draftReady(draft: { url: string; model: string }): boolean {
  return /^https?:\/\/./.test(draft.url.trim()) && draft.model.trim() !== ''
}

/** Whether the custom fields as typed differ from what is saved. */
export function draftChanged(draft: Draft, option: EmbeddingsOption | undefined): boolean {
  const saved = draftOf(option)
  return draft.key.trim() !== '' || (['url', 'model', 'queryPrefix', 'passagePrefix'] as const).some(name => draft[name].trim() !== saved[name].trim())
}

/** What Save and test sends for the custom endpoint. A key is sent only where one was typed. */
export function customParams(draft: Draft): EmbeddingsSetParams {
  const key = draft.key.trim()
  return {
    select: 'custom', service: 'custom',
    values: {
      url: draft.url.trim(), model: draft.model.trim(),
      queryPrefix: draft.queryPrefix === '' ? null : draft.queryPrefix,
      passagePrefix: draft.passagePrefix === '' ? null : draft.passagePrefix,
      ...(key ? { key } : {}),
    },
  }
}

/**
 * The embeddings service recall sends to, under its switch (RETRIEVE-35 to RETRIEVE-37): one radio
 * per service that can be chosen, the model or endpoint each keeps, and the test call's outcome.
 */
export function EmbeddingsPicker({ state, choice, onSaved }: {
  state: Retrieval
  choice: EmbeddingsChoice
  onSaved: (state: Retrieval) => void
}): React.JSX.Element {
  const [pending, setPending] = useState<EmbeddingsService | null>(null)
  const [busy, setBusy] = useState(false)
  const [testing, setTesting] = useState<EmbeddingsService | null>(null)
  const [result, setResult] = useState<{ service: EmbeddingsService; test: EmbeddingsTest; saved: boolean } | null>(null)
  const [problem, setProblem] = useState('')
  const [refused, setRefused] = useState<{ service: EmbeddingsService; message: string } | null>(null)
  const [asking, setAsking] = useState<{ url: string; params: EmbeddingsSetParams; service: EmbeddingsService } | null>(null)
  const [moved, setMoved] = useState(false)
  // Leo's radio checks itself; remounting the group puts it back where the state says after a cancel or a refusal.
  const [mount, setMount] = useState(0)
  const [models, setModels] = useState<Models>({ status: 'idle' })
  const [trying, setTrying] = useState<string | null>(null)
  const custom = choice.options.find(option => option.service === 'custom')
  const [draft, setDraft] = useState<Draft>(() => draftOf(custom))
  const loadingShown = useDelayedFlag(models.status === 'loading')

  const current = currentService(choice, pending)
  const locked = choiceLockedText(choice)
  const disabled = busy || !!choice.locked

  const loadModels = async (refresh: boolean): Promise<void> => {
    setModels({ status: 'loading' })
    try {
      const answer = await window.bravebot.request<{ models: EmbeddingsModel[] }>('retrieval.embeddings.models', refresh ? { refresh: true } : {})
      if (answer.ok && Array.isArray(answer.ok.models)) setModels({ status: 'ready', models: sortedModels(answer.ok.models) })
      else setModels({ status: 'error', message: answer.error?.message ?? 'the agent did not answer' })
    } catch (error) {
      setModels({ status: 'error', message: String(error) })
    }
  }

  useEffect(() => {
    if (current === 'openrouter' && models.status === 'idle') void loadModels(false)
  }, [current, models.status])

  const send = async (params: EmbeddingsSetParams, service: EmbeddingsService, retest = false): Promise<void> => {
    setBusy(true); setProblem(''); setResult(null); setRefused(null)
    const selecting = params.select !== undefined || service === choice.selected
    if (selecting) setTesting(service)
    const before = choice.selected ?? choice.inForce
    try {
      const answer = await window.bravebot.request<EmbeddingsSetAnswer>('retrieval.embeddings.set', params)
      const ok = answer.ok
      if (!ok?.state) {
        const message = answer.error?.message ?? 'the agent did not answer'
        const step = refusalStep(service, before, message)
        if (step.kind === 'inline') setRefused({ service, message: step.message })
        else setProblem(step.text)
        setPending(null); setMount(n => n + 1)
        return
      }
      if (ok.needsDisclosure) { setAsking({ url: ok.needsDisclosure, params, service }); return }
      setAsking(null)
      setPending(null)
      onSaved(ok.state)
      if (ok.test) setResult({ service, test: ok.test, saved: !retest })
      const after = ok.state.embeddings?.selected ?? ok.state.embeddings?.inForce ?? null
      if (state.enabled && after !== before) setMoved(true)
      if (service === 'custom') setDraft(draftOf(ok.state.embeddings?.options.find(option => option.service === 'custom')))
    } catch (error) {
      setProblem(`The embeddings service was not changed: ${String(error)}.`)
      setPending(null); setMount(n => n + 1)
    } finally {
      setBusy(false); setTesting(null)
    }
  }

  const choose = (service: EmbeddingsService): void => {
    if (service === current) return
    const option = choice.options.find(each => each.service === service)
    if (!option) return
    setResult(null); setProblem(''); setRefused(null)
    const step = chooseStep(state, option)
    if (step.kind === 'pending') { setPending(service); return }
    void send(step.params, service)
  }

  const hostOf = (url: string): string => url.replace(/^[a-z]+:\/\//i, '').split(/[/?#]/)[0] ?? url

  return <div className="settings-row-alert recall-embeddings" data-test="recall-embeddings">
    <h4 className="recall-embeddings-title" id="recall-embeddings-title">Embeddings service</h4>
    <div className="recall-embeddings-options" role="radiogroup" aria-labelledby="recall-embeddings-title" key={mount}>
      {shownOptions(choice).map(option => {
        const chosen = option.service === current
        const unavailable = !option.available
        // Leo's radio leaves its input enabled when disabled, so a click would still check it.
        return <div key={option.service} className="recall-embeddings-option"
          onClickCapture={event => { if ((disabled || unavailable) && (event.target as Element).tagName === 'LEO-RADIOBUTTON') event.preventDefault() }}
          data-test={unavailable ? `recall-option-${option.service}-unavailable` : `recall-option-${option.service}`}>
          <RadioButton name="recall-embeddings" value={option.service} currentValue={current ?? ''} size="normal"
            isDisabled={disabled || unavailable} onChange={({ value }) => choose(value as EmbeddingsService)}>
            <span className="recall-embeddings-name">{SERVICE_LABELS[option.service]}</span>
          </RadioButton>
          <p className="settings-caption recall-embeddings-detail">{serviceDetail(option, !!state.endpoint?.development && option.service === 'brave')}</p>
          {chosen && <div className="recall-embeddings-fields">
            {unavailable && <Alert type="warning" size="small">
              {SERVICE_LABELS[option.service]} can’t be used: {option.reason ?? 'it is not set up'}. Long conversations are summarised until you choose another service.
            </Alert>}
            {!unavailable && option.service === 'openai' && <Dropdown value={option.model ?? 'text-embedding-3-small'} size="small" disabled={disabled}
              data-test="recall-model-openai"
              onChange={(detail) => {
                if (typeof detail.value === 'string' && detail.value !== option.model) void send({ select: 'openai', service: 'openai', values: { model: detail.value } }, 'openai')
              }}>
              <span slot="label">Model</span>
              {(option.models ?? ['text-embedding-3-small', 'text-embedding-3-large']).map(model => <leo-option key={model} value={model}>{model}</leo-option>)}
            </Dropdown>}
            {!unavailable && option.service === 'openrouter' && <OpenRouterModels option={option} models={models} loadingShown={loadingShown}
              disabled={disabled} onRetry={() => void loadModels(true)}
              trying={trying}
              onChoose={model => { setTrying(model); void send({ select: 'openrouter', service: 'openrouter', values: { model } }, 'openrouter').finally(() => setTrying(null)) }} />}
            {!unavailable && option.service === 'custom' && <CustomFields option={option} draft={draft} disabled={disabled}
              testing={testing === 'custom'}
              setDraft={next => { setDraft(next); if (result?.service === 'custom') setResult(null); setRefused(null) }}
              onSave={() => void send(customParams(draft), 'custom')}
              onRemoveKey={() => void send({ service: 'custom', values: { key: null } }, 'custom')} />}
            <TestOutcome option={option} testing={testing === option.service}
              result={result?.service === option.service ? result : null}
              stale={pending === option.service}
              refused={refused?.service === option.service ? refused.message : null}
              onAgain={() => void send({ select: option.service }, option.service, true)} disabled={disabled} />
          </div>}
        </div>
      })}
    </div>
    {locked && !state.locked && <p className="settings-caption" data-test="recall-embeddings-locked">{locked}</p>}
    {moved && state.enabled && <p className="settings-caption" data-test="recall-reindex">Earlier messages will be indexed again for the new service.</p>}
    {problem && <Alert type="error" role="alert" data-test="recall-embeddings-error">{problem}</Alert>}
    {asking && <RecallDisclosure state={state}
      target={{ service: asking.service, host: hostOf(asking.url), development: false }}
      confirm={`Use ${hostOf(asking.url)}`}
      onCancel={() => { setAsking(null); setPending(null); setMount(n => n + 1) }}
      onConfirm={() => void send({ ...asking.params, disclosed: asking.url }, asking.service)} />}
  </div>
}

function OpenRouterModels({ option, models, loadingShown, disabled, trying, onRetry, onChoose }: {
  option: EmbeddingsOption
  trying: string | null
  models: Models
  loadingShown: boolean
  disabled: boolean
  onRetry: () => void
  onChoose: (model: string) => void
}): React.JSX.Element {
  if (models.status === 'idle' || models.status === 'loading') {
    return <Skeleton label="Loading models…" count={1} delayed={!loadingShown} data-test="recall-models-loading" />
  }
  if (models.status === 'error') {
    return <div className="recall-embeddings-problem" data-test="recall-models-error">
      <Alert type="error" size="small" role="alert">Couldn’t load OpenRouter’s models: {models.message}.</Alert>
      <Button size="small" kind="outline" onClick={onRetry} data-test="recall-models-retry">Try again</Button>
    </div>
  }
  if (models.models.length === 0) return <p className="settings-caption" data-test="recall-models-empty">OpenRouter lists no embedding models.</p>
  const listed = option.model && !models.models.some(model => model.id === option.model)
    ? [{ id: option.model, name: option.model, contextLength: null, pricing: { prompt: null } }, ...models.models]
    : models.models
  return <Dropdown value={trying ?? option.model ?? undefined} placeholder="Choose a model" size="small" disabled={disabled} data-test="recall-model-openrouter"
    onChange={(detail) => { if (typeof detail.value === 'string' && detail.value !== option.model) onChoose(detail.value) }}>
    <span slot="label">Model</span>
    {listed.map(model => <leo-option key={model.id} value={model.id}>{modelText(model)}</leo-option>)}
  </Dropdown>
}

function CustomFields({ option, draft, setDraft, disabled, testing, onSave, onRemoveKey }: {
  option: EmbeddingsOption
  draft: Draft
  setDraft: (draft: Draft) => void
  disabled: boolean
  testing: boolean
  onSave: () => void
  onRemoveKey: () => void
}): React.JSX.Element {
  const field = (name: keyof Draft) => ({
    value: String(draft[name]),
    disabled,
    onInput: ({ value }: { value: string }) => setDraft({ ...draft, [name]: value }),
  })
  return <>
    <Input type="url" size="small" placeholder="https://example.com/v1/embeddings" data-test="recall-custom-url" {...field('url')}>URL</Input>
    <Input size="small" placeholder="text-embedding-3-small" data-test="recall-custom-model" {...field('model')}>Model</Input>
    {option.keyStored && !draft.replacing
      ? <div className="recall-embeddings-key" data-test="recall-custom-key-stored">
        <span className="settings-caption">Key stored</span>
        <Button size="small" kind="plain-faint" isDisabled={disabled} onClick={() => setDraft({ ...draft, replacing: true })} data-test="recall-custom-key-replace">Replace</Button>
        <Button size="small" kind="plain-faint" isDisabled={disabled} onClick={onRemoveKey} data-test="recall-custom-key-remove">Remove</Button>
      </div>
      : <Input type="password" size="small" data-test="recall-custom-key" {...field('key')}>Access key (optional)</Input>}
    <Collapse title="Advanced" isOpen={option.queryPrefix || option.passagePrefix ? true : undefined} data-test="recall-custom-advanced">
      <div className="recall-embeddings-fields recall-embeddings-advanced">
        <Input size="small" data-test="recall-custom-query-prefix" {...field('queryPrefix')}>Query prefix</Input>
        <Input size="small" data-test="recall-custom-passage-prefix" {...field('passagePrefix')}>Passage prefix</Input>
      </div>
    </Collapse>
    <div className="settings-actions">
      <Button size="small" kind="filled" isDisabled={disabled || !draftReady(draft)} isLoading={testing} onClick={onSave} data-test="recall-custom-save">Save and test</Button>
      {!testing && draftChanged(draft, option) && <span className="settings-caption" data-test="recall-custom-unsaved">Unsaved changes</span>}
    </div>
  </>
}

export function TestOutcome({ option, testing, result, refused, stale, disabled, onAgain }: {
  option: EmbeddingsOption
  testing: boolean
  result: { test: EmbeddingsTest; saved: boolean } | null
  refused: string | null
  stale: boolean
  disabled: boolean
  onAgain: () => void
}): React.JSX.Element | null {
  const body = testing
    ? <p className="recall-embeddings-testing settings-caption" data-test="recall-test">
      <span className="recall-embeddings-spinner" aria-hidden="true"><ProgressRing mode="indeterminate" /></span>
      Testing {option.host ?? 'the endpoint'}…
    </p>
    : refused
      ? <Alert type="error" size="small" role="alert" data-test="recall-test-refused">Not changed, {refused}. {SERVICE_LABELS[option.service]} is still in use as before.</Alert>
    : result?.test.ok
      ? <Alert type="success" size="small" role="status" data-test="recall-test-ok">{testText(result.test, option, result.saved)}</Alert>
      : result
        ? <div className="recall-embeddings-problem" data-test="recall-test-error">
          <Alert type="error" size="small">{testText(result.test, option, result.saved)}</Alert>
          <Button size="small" kind="outline" isDisabled={disabled} onClick={onAgain} data-test="recall-test-again">Test again</Button>
        </div>
        : !stale && option.available && option.reason && complete(option)
          ? <p className="settings-caption" data-test="recall-not-ready">Not ready yet: {option.reason}.</p>
          : !stale && option.available && option.dimensions
            ? <p className="settings-caption" data-test="recall-dimensions">Each message becomes {option.dimensions} numbers.</p>
            : null
  return body && <div aria-live="polite">{body}</div>
}
