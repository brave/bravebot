import { failureSummary } from '../failure'
export function ErrorCard({ detail, onRetry, onModel, category, attempts, status }: {
  category?: string | null; attempts?: number | null; status?: number | null
  detail: string; onRetry?: () => void; onModel?: () => void
}): React.JSX.Element {
  const denied = /401|403|unauthoriz|credential/i.test(detail)
  const billing = /402|credit|balance/i.test(detail)
  const limited = /429|rate.limit/i.test(detail)
  const stopped = /cancel/i.test(detail)
  const title = stopped ? 'Task stopped' : denied ? 'The provider could not authenticate this request' : billing
    ? 'The model provider rejected this request' : limited ? 'The provider is busy' : 'This step could not finish'
  const description = billing ? 'Check the provider’s account limits or choose another available model.'
    : denied ? 'Check backend credentials using Help → Diagnostics.'
    : limited ? 'Wait a moment before retrying, or choose another model.'
    : stopped ? 'Completed changes remain in the project. You can continue from here.'
    : 'Your conversation is preserved. Review the details, then continue from the current project state.'
  const classified = category ? failureSummary(category) : { title, description }
  return <div className="error-card" role="alert">
    <strong>{classified.title}</strong><p>{classified.description}</p>
    <div className="error-actions">
      {onRetry && <button onClick={onRetry}>Draft continuation</button>}
      {onModel && <button onClick={onModel}>Choose another model</button>}
    </div>
    <details><summary>Technical details</summary><pre>{detail}{attempts != null ? `\nRequests attempted: ${attempts}` : ''}{status != null ? `\nHTTP status: ${status}` : ''}</pre></details>
  </div>
}
