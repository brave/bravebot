import { signInWayOf } from '../../shared/accounts'
import { FailureSentence } from './FailureSentence'
import { SignInButton } from './SignInButton'

/**
 * What the model list could not ask of a service, one sentence each, in the picker's status type.
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
    return <p className="model-status" key={warning}><FailureSentence text={warning} command={commands?.[index]} />{way && <> <SignInButton way={way} session={session} onSignedIn={onSignedIn} /></>}</p>
  })}</>
}
