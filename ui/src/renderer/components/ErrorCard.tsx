import { failureSummary } from '../failure'

/**
 * A failure, titled from the category the agent sent and never from the words in the detail.
 *
 * The detail is prose: a backend's own diagnostic, sometimes translated, sometimes quoting
 * something a model wrote. Matching on it is what the wire protocol forbids in as many words,
 * and it is why every enum here arrives with a tag. A failure that carried no category gets
 * `failureSummary`'s unknown-category answer, which is the same one an unrecognised tag gets.
 */
export function ErrorCard({ detail, onRetry, onModel, category, attempts, status }: {
  category?: string | null; attempts?: number | null; status?: number | null
  detail: string; onRetry?: () => void; onModel?: () => void
}): React.JSX.Element {
  const classified = failureSummary(category ?? '')
  return <div className="error-card" role="alert">
    <strong>{classified.title}</strong><p>{classified.description}</p>
    <div className="error-actions">
      {onRetry && <button onClick={onRetry}>Draft continuation</button>}
      {onModel && <button onClick={onModel}>Choose another model</button>}
    </div>
    <details><summary>Technical details</summary><pre>{detail}{attempts != null ? `\nRequests attempted: ${attempts}` : ''}{status != null ? `\nHTTP status: ${status}` : ''}</pre></details>
  </div>
}
