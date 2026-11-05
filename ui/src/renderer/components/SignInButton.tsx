import type { SignInWay } from '../../shared/accounts'
import { locale, t } from '../i18n'
import { Button, ProgressRing } from '../nala'
import { useSignIn, type SignInResult, type Waiting } from '../use-sign-in'

export const SIGN_IN_NAMES: Record<SignInWay, string> = { chatgpt: 'ChatGPT', xai: 'xAI', bedrock: 'AWS' }

const clock = (at: number) => new Intl.DateTimeFormat(locale, { hour: 'numeric', minute: '2-digit' }).format(at)

/** The row's words while a sign-in waits: no address and no code, only that it waits and for how long. */
export function SignInWaiting({ waiting, onShow, onCancel }: { waiting: Waiting; onShow: () => void; onCancel: () => void }): React.JSX.Element {
  const requesting = waiting.way === 'xai' && !waiting.started
  return <div className="sign-in-waiting" data-test={`sign-in-waiting-${waiting.way}`}>
    <ProgressRing mode="indeterminate" />
    <span role="status">{requesting ? t.uiAccountsRequestingCode() : t.uiAccountsWaiting()}</span>
    {waiting.port !== null && <span className="num" data-test="sign-in-port">{t.uiAccountsPort({ port: waiting.port })}</span>}
    {waiting.until !== null && <span className="settings-caption">{t.uiAccountsStopsWaiting({ time: clock(waiting.until) })}</span>}
    <span className="settings-actions">
      {waiting.started && <Button size="small" kind="outline" onClick={onShow} data-test="sign-in-show">{t.uiAccountsShowSignInWindow()}</Button>}
      <Button size="small" kind="plain-faint" onClick={onCancel} data-test="sign-in-cancel">{t.uiAccountsCancel()}</Button>
    </span>
  </div>
}

/**
 * What an ended sign-in came to, in one sentence, and for an AWS sign-in a second saying what
 * became of the switch that turns Bedrock on.
 */
export function signInSentence(result: SignInResult, name: string): string {
  if (result.outcome === 'signedIn') return result.sentence ? `${t.uiAccountsSignedInTo({ name })} ${result.sentence}` : t.uiAccountsSignedInTo({ name })
  if (result.outcome === 'cancelled') return t.uiAccountsCancelled()
  return result.sentence ?? t.uiAccountsSignInFailed({ name })
}

/**
 * "Sign in" beside a command that signs in to the ChatGPT or the xAI account, on a failure card
 * or a picker warning (CHATGPT-8, XAI-15, XAI-19). It asks the main process, as the Accounts row does.
 */
export function SignInButton({ way, session, onSignedIn, size = 'small' }: { way: SignInWay; session?: string; onSignedIn?: () => void; size?: 'tiny' | 'small' }): React.JSX.Element {
  const signIn = useSignIn(onSignedIn, session)
  const name = SIGN_IN_NAMES[way]
  if (signIn.waiting?.way === way) return <SignInWaiting waiting={signIn.waiting} onShow={signIn.show} onCancel={signIn.cancel} />
  return <span className="sign-in-ask">
    <Button size={size} kind="filled" isDisabled={signIn.busy} onClick={() => void signIn.start(way)} data-test={`sign-in-${way}`}>{t.uiAccountsSignIn()}</Button>
    {signIn.result && (signIn.result.way ?? way) === way && <span role="status" className="settings-caption">{signInSentence(signIn.result, name)}</span>}
  </span>
}
