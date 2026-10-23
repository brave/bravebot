import { memo, type ComponentType } from 'react'
import type { AskAnswer, JsonValue, Pending } from '@brave/agent-client'
import { readAsk, readConfirm, readFetch, readRun, readTrust } from '../../payloads'
import { AskCard, ConfirmCard, FetchCard, RunCard, TrustCard, UnanswerableCard } from './cards'

export interface ApprovalHandlers {
  onDecide: (request: number, decision: 'approve' | 'reject') => void
  onAnswer: (request: number, answers: AskAnswer[]) => void
  onTrust: (trusted: boolean) => void
}

/**
 * The question waiting for this person: the startup trust question, else the pending approval.
 * A payload that does not read completely is shown as unanswerable. Nothing here accepts a reply
 * for a request other than the one on screen, and nothing answers on the person's behalf.
 */
export const ApprovalPanel = memo(function ApprovalPanel({
  pending,
  startupTrust,
  target,
  busy,
  handlers,
}: {
  pending: Pending | null
  startupTrust: JsonValue | null
  target: string
  busy: boolean
  handlers: ApprovalHandlers
}) {
  if (startupTrust !== null) {
    const view = readTrust(startupTrust)
    return view === null ? (
      <UnanswerableCard
        kind="trust"
        reason="The trust question could not be read completely, so it is not offered for an answer."
        target={target}
        way="No turn has started, so there is none to cancel. Open the menu and start a new session."
      />
    ) : (
      <TrustCard key="trust" view={view} target={target} busy={busy} onAnswer={handlers.onTrust} />
    )
  }
  if (pending === null) return null
  if (!pending.supported) {
    return <UnanswerableCard kind={pending.kind} reason="This device cannot answer that kind of question." target={target} />
  }
  const unreadable = (
    <UnanswerableCard kind={pending.kind} reason="The request could not be read completely, so it is not offered for an answer." target={target} />
  )
  const request = pending.request
  // A card for a request is offered only if its payload read completely and names the request on screen.
  const decided = <V extends { request: number }>(view: V | null, Card: ComponentType<{ view: V; target: string; busy: boolean; onDecide: (decision: 'approve' | 'reject') => void }>) =>
    view === null || view.request !== request ? unreadable : <Card key={request} view={view} target={target} busy={busy} onDecide={(decision) => handlers.onDecide(request, decision)} />
  switch (pending.kind) {
    case 'confirm':
      return decided(readConfirm(pending.data), ConfirmCard)
    case 'run':
      return decided(readRun(pending.data), RunCard)
    case 'fetch':
      return decided(readFetch(pending.data), FetchCard)
    case 'ask': {
      const view = readAsk(pending.data)
      return view === null || view.request !== request ? unreadable : <AskCard key={request} view={view} target={target} busy={busy} onAnswer={(answers) => handlers.onAnswer(request, answers)} />
    }
    default:
      return unreadable
  }
})
