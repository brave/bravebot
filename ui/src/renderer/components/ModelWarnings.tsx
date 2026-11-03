import { signInWayOf } from '../../shared/accounts'
import { Alert } from '../nala'
import { FailureSentence } from './FailureSentence'
import { SignInButton } from './SignInButton'

/**
 * What the model list could not ask of a service, one warning alert each.
 * A warning that names a command to run has it beside the sentence, in the same order, and it is
 * drawn as code, with "Sign in" beside it where the window can start that sign-in itself (XAI-19).
 * `onSignedIn` is told when that sign-in is stored, so the list can be asked for again.
 */
export function ModelWarnings({ warnings, commands, session, onSignedIn }: {
  warnings: string[]
  commands?: (string | null)[]
  session?: string
  onSignedIn?: () => void
}): React.JSX.Element {
  return <>{warnings.map((warning, index) => {
    const way = signInWayOf(commands?.[index])
    return <Alert type="warning" size="small" className="model-status" hasActions={Boolean(way)} key={warning} data-test="model-warning">
      <FailureSentence text={warning} command={commands?.[index]} />
      {way && <div slot="actions"><SignInButton way={way} session={session} onSignedIn={onSignedIn} /></div>}
    </Alert>
  })}</>
}
