import type { CutOff } from '../../shared/protocol'
import { failureSummary } from '../failure'
import { Alert, Button, Collapse } from '../nala'
import { signInWayOf } from '../../shared/accounts'
import { FailureSentence } from './FailureSentence'
import { SignInButton } from './SignInButton'

/**
 * A failure, titled from the category the agent sent and never from the words in the detail.
 * Where the failure was one of a ChatGPT or an xAI account's, the sentence the bridge sent for it is the
 * wording instead, so the words of that failure are the bridge's and not a second table here. The
 * command it names arrives as `accountCommand` and is drawn as code, with "Sign in" beside it where
 * the window can start that sign-in itself (CHATGPT-15, XAI-20). Any other failure the bridge has a
 * sentence for arrives as `failureMessage`, the agent's own line, with the setting names in it as
 * `failureCodes`, and is the wording in the same way (BACKEND-37). Its title follows `failureCause`, the
 * closed name of which part of the request ended, where one arrived.
 *
 * The detail is prose: a backend's own diagnostic, sometimes translated, sometimes quoting
 * something a model wrote. Matching on it is what the wire protocol forbids in as many words,
 * and it is why every enum here arrives with a tag. A failure that carried no category gets
 * `failureSummary`'s unknown-category answer, which is the same one an unrecognised tag gets.
 * A stop at the output ceiling is described from `cutOff`, which is fields the agent sent, for the
 * same reason.
 */
export function ErrorCard({ detail, onRetry, onModel, category, accountMessage, accountCommand, failureMessage, failureCodes, failureCause, attempts, status, cutOff }: {
  category?: string | null; accountMessage?: string | null; accountCommand?: string | null; failureMessage?: string | null; failureCodes?: readonly string[] | null; failureCause?: string | null; attempts?: number | null; status?: number | null; cutOff?: CutOff | null
  detail: string; onRetry?: () => void; onModel?: () => void
}): React.JSX.Element {
  // A `/plan` line whose mode was not set started no turn, so there is no turn to continue and no
  // model to change.
  if (category === 'plan-mode') { onRetry = undefined; onModel = undefined }
  const classified = failureSummary(category ?? '', cutOff, accountMessage, failureMessage, failureCause)
  const way = accountMessage ? signInWayOf(accountCommand) : null
  const technical = `${detail}${attempts != null ? `\nRequests attempted: ${attempts}` : ''}${status != null ? `\nHTTP status: ${status}` : ''}`
  return (
    <Alert type="error" className="error-card" data-test="error-card" hasActions={Boolean(onRetry || onModel || way)}>
      <span slot="title">{classified.title}</span>
      <p>{accountMessage ? <FailureSentence text={classified.description} command={accountCommand} /> : failureMessage ? <FailureSentence text={classified.description} codes={failureCodes} /> : classified.description}</p>
      {(onRetry || onModel || way) && (
        <div slot="actions" className="error-actions">
          {way && <SignInButton way={way} />}
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
