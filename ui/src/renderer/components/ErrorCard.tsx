import type { CutOff } from '../../shared/protocol'
import { failureSummary } from '../failure'
import { Alert, Button, Collapse } from '../nala'
import { FailureSentence } from './FailureSentence'

/**
 * A failure, titled from the category the agent sent and never from the words in the detail.
 * Where the failure was one of a ChatGPT or an xAI account's, the sentence the bridge sent for it is the
 * wording instead, so the words of that failure are the bridge's and not a second table here. The
 * command it names arrives as `accountCommand` and is drawn as code.
 *
 * The detail is prose: a backend's own diagnostic, sometimes translated, sometimes quoting
 * something a model wrote. Matching on it is what the wire protocol forbids in as many words,
 * and it is why every enum here arrives with a tag. A failure that carried no category gets
 * `failureSummary`'s unknown-category answer, which is the same one an unrecognised tag gets.
 * A stop at the output ceiling is described from `cutOff`, which is fields the agent sent, for the
 * same reason.
 */
export function ErrorCard({ detail, onRetry, onModel, category, accountMessage, accountCommand, attempts, status, cutOff }: {
  category?: string | null; accountMessage?: string | null; accountCommand?: string | null; attempts?: number | null; status?: number | null; cutOff?: CutOff | null
  detail: string; onRetry?: () => void; onModel?: () => void
}): React.JSX.Element {
  const classified = failureSummary(category ?? '', cutOff, accountMessage)
  const technical = `${detail}${attempts != null ? `\nRequests attempted: ${attempts}` : ''}${status != null ? `\nHTTP status: ${status}` : ''}`
  return (
    <Alert type="error" className="error-card" data-test="error-card" hasActions={Boolean(onRetry || onModel)}>
      <span slot="title">{classified.title}</span>
      <p>{accountMessage ? <FailureSentence text={classified.description} command={accountCommand} /> : classified.description}</p>
      {(onRetry || onModel) && (
        <div slot="actions" className="error-actions">
          {onRetry && <Button size="small" kind="outline" onClick={onRetry} data-test="error-retry">Draft continuation</Button>}
          {onModel && <Button size="small" kind="plain" onClick={onModel} data-test="error-model">Choose another model</Button>}
        </div>
      )}
      <Collapse title="Technical details" isOpen={undefined} data-test="error-details">
        <pre>{technical}</pre>
      </Collapse>
    </Alert>
  )
}
