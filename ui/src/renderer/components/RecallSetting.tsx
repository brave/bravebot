import { useState } from 'react'
import { Modal } from './Modal'
import { SettingsGroup } from './SettingsGroup'
import { Alert, Button, Toggle } from '../nala'
import type { EmbeddingsService, Retrieval } from '../../shared/agent-settings'
import { EmbeddingsPicker } from './EmbeddingsPicker'

/** What `retrieval.set` is sent, or the disclosure dialog where the person has not seen it for this endpoint. */
export type RecallStep = { kind: 'ask' } | { kind: 'write'; params: { enabled: boolean; disclosed?: string } }

/** The switch was moved to `on`. */
export function recallChosen(state: Retrieval, on: boolean): RecallStep {
  // With no endpoint nothing is sent, so there is nothing to show first.
  if (on && state.endpoint && !state.disclosed) return { kind: 'ask' }
  return { kind: 'write', params: { enabled: on } }
}

/** Turn on recall was pressed in the dialog: the request names the endpoint the dialog showed. */
export function recallAgreed(state: Retrieval): RecallStep {
  return { kind: 'write', params: { enabled: true, disclosed: state.endpoint?.url } }
}

/** The service as a sentence names it. */
export function serviceName(state: Retrieval): string {
  return serviceNamed(state.endpoint?.service, !!state.endpoint?.development)
}

function serviceNamed(service: EmbeddingsService | undefined, development: boolean): string {
  switch (service) {
    case 'openai': return 'OpenAI’s embeddings service'
    case 'openrouter': return 'OpenRouter’s embeddings service'
    case 'custom': return 'your embeddings endpoint'
    default: return development ? 'Brave’s embeddings service (development)' : 'Brave’s embeddings service'
  }
}

/** Whether the switch can be turned on: only where an endpoint resolves, unless an older agent says nothing of the choice. */
export function switchBlocked(state: Retrieval): boolean {
  return !state.enabled && !state.endpoint && !!state.embeddings
}

/** Why the switch cannot be moved, or null where it can. */
export function lockedText(state: Retrieval): string | null {
  if (!state.locked) return null
  return state.locked.by === 'managed' ? 'Set by your administrator' : `Set by ${state.locked.path ?? 'the selected settings file'} for this run`
}

/** The caption under the switch: where it sends, what is never sent, and the file it is kept in. */
export function recallCaption(state: Retrieval): string {
  const kept = state.path ? ` Saved in ${state.path}.` : ''
  if (!state.endpoint && state.embeddings) return `Choose an embeddings service below to turn on recall.${kept}`
  if (!state.endpoint) return `No embeddings service is set up${state.unconfigured ? `: ${state.unconfigured}` : ''}, so nothing is sent and long conversations are summarised.${kept}`
  return `Sends message text to ${serviceName(state)}, ${state.endpoint.host}. Never sent: untrusted content, images and PDFs, or anything in incognito.${kept}`
}

/**
 * Shown the first time recall is turned on against an endpoint (RETRIEVE-25), and when the
 * embeddings service moves to a host the person was not shown while recall is on (`target`).
 */
export function RecallDisclosure({ state, target, confirm = 'Turn on recall', onCancel, onConfirm }: {
  state: Retrieval
  target?: { service: EmbeddingsService; host: string; development: boolean }
  confirm?: string
  onCancel: () => void
  onConfirm: () => void
}): React.JSX.Element {
  const name = target ? serviceNamed(target.service, target.development) : serviceName(state)
  const host = target ? target.host : state.endpoint?.host
  return (
    <Modal title="Recall earlier messages" size="md" className="recall-disclosure" onClose={onCancel}
      actions={<>
        <Button kind="plain-faint" size="small" onClick={onCancel} data-test="recall-cancel">Cancel</Button>
        <Button kind="filled" size="small" onClick={onConfirm} data-test="recall-confirm">{confirm}</Button>
      </>}>
      <p>
        In a long bot conversation, the earlier messages closest to what you ask are brought back
        instead of a summary. To find them, the text of each message is sent
        to {name}, <span className="settings-path">{host}</span>. The numbers
        that come back are kept on this computer.
      </p>
      <p>Never sent: emails, web pages and other content marked untrusted, images and PDFs, and anything in incognito.</p>
      <p>Project conversations are not affected. Messages already in a bot’s conversation are sent once, in the background.</p>
    </Modal>
  )
}

/** The Bots group at the top of Agent settings, with the one switch retrieval has (RETRIEVE-1). */
export function RecallGroup({ state, onSaved }: {
  state: Retrieval
  onSaved: (state: Retrieval) => void
}): React.JSX.Element {
  const [asking, setAsking] = useState(false)
  const [busy, setBusy] = useState(false)
  const [problem, setProblem] = useState('')
  // Leo's switch moves itself; remounting it puts it back where the state says after a cancel or a refusal.
  const [mount, setMount] = useState(0)
  const locked = lockedText(state)

  const run = async (step: RecallStep): Promise<void> => {
    if (step.kind === 'ask') { setAsking(true); return }
    setAsking(false); setBusy(true); setProblem('')
    try {
      const answer = await window.bravebot.request<Retrieval>('retrieval.set', step.params)
      if (answer.ok && typeof answer.ok.enabled === 'boolean') onSaved(answer.ok)
      else { setProblem(`Recall was not changed: ${answer.error?.message ?? 'the agent did not answer'}.`); setMount(n => n + 1) }
    } catch (error) {
      setProblem(`Recall was not changed: ${String(error)}.`); setMount(n => n + 1)
    } finally { setBusy(false) }
  }

  return (
    <SettingsGroup title="Bots">
      <div className="settings-row" data-test="recall-row">
        <div className="settings-row-text">
          <h3>Recall earlier messages</h3>
          <p>When a bot conversation runs long, bring back the earlier messages closest to what you ask instead of a summary.</p>
          <p className="settings-caption" data-test="recall-caption">{recallCaption(state)}</p>
          {locked && <p className="settings-caption" data-test="recall-locked">{locked}</p>}
        </div>
        <div className="settings-row-control">
          <Toggle key={mount} size="small" checked={state.enabled} disabled={busy || !!locked || switchBlocked(state)}
            aria-label="Recall earlier messages" data-test="recall-switch"
            onChange={({ checked }) => void run(recallChosen(state, checked))} />
        </div>
        {problem && <Alert type="error" role="alert" className="settings-row-alert" data-test="recall-error">{problem}</Alert>}
        {state.embeddings && <EmbeddingsPicker state={state} choice={state.embeddings} onSaved={onSaved} />}
      </div>
      {asking && <RecallDisclosure state={state}
        onCancel={() => { setAsking(false); setMount(n => n + 1) }}
        onConfirm={() => void run(recallAgreed(state))} />}
    </SettingsGroup>
  )
}
