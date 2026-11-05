/**
 * The page of the main process's sign-in window (CHATGPT-16, XAI-21, BACKEND-70).
 *
 * It draws what the main process recorded for the attempt and nothing else: no session, no
 * conversation, no bridge. Copying and opening are done by the main process from what it recorded,
 * so nothing here hands it text or an address. The words are English, like the main process's own
 * dialogs; it has no catalog.
 */

import { StrictMode, useEffect, useState } from 'react'
import { createRoot } from 'react-dom/client'
import { SIGN_IN_LOGOS, type SignInWindowState } from '../shared/accounts'
import type { BravebotSignInApi } from '../preload/sign-in'
import { Button } from './nala'
import { CopyField, SmallWindowPage } from './small-window-page'
import './nala-setup'
import './styles.css'
import './styles/small-window.css'

declare global {
  interface Window { bravebotSignIn?: BravebotSignInApi }
}

const minutes = (seconds: number) => Math.max(1, Math.round(seconds / 60))

export function SignInPage({ state }: { state: SignInWindowState }): React.JSX.Element {
  const api = window.bravebotSignIn
  const copy = (what: 'url' | 'code' | 'address') => () => api?.copy(what)
  const open = (label: string, isDisabled = false) =>
    <Button kind="filled" isDisabled={isDisabled} onClick={() => void api?.open()} data-test="sign-in-open">{label}</Button>
  const page = (actions: React.ReactNode, children: React.ReactNode) =>
    <SmallWindowPage logo={SIGN_IN_LOGOS[state.way]} logoTest="sign-in-logo" title={`Sign in to ${state.name}`} test="sign-in-window"
      fit={api?.fit} actions={<>
        <Button kind="plain-faint" onClick={() => void api?.cancel()} data-test="sign-in-cancel">Cancel</Button>
        {actions}
      </>}>{children}</SmallWindowPage>
  if (state.way === 'bedrock') {
    return page(open('Open browser again', state.url === null), <>
      <p>The AWS command-line tool opened your browser{state.id === null ? '' : <> for the profile <span data-test="sign-in-profile">{state.id}</span></>}. Check that the page shows this code, then allow access.</p>
      {state.code === null
        ? <p className="small-window-note" data-test="sign-in-code-pending">Waiting for the code…</p>
        : <CopyField kind="code" test="sign-in-code" copyTest="sign-in-copy-code" label="Copy code" copy={copy('code')}>{state.code}</CopyField>}
      {state.url !== null && <CopyField kind="url" test="sign-in-url" copyTest="sign-in-copy-url" label="Copy address" copy={copy('url')}>{state.url}</CopyField>}
    </>)
  }
  if (state.way === 'chatgpt') {
    return page(open('Open browser again'), <>
      <p>Finish signing in in your browser. If it did not open, open this address in it.</p>
      <CopyField kind="url" test="sign-in-url" copyTest="sign-in-copy-url" label="Copy address" copy={copy('url')}>{state.url}</CopyField>
      {state.port !== null && <p className="small-window-note">The sign-in waits for the browser on port <span className="num">{state.port}</span>. On another computer, forward that port to this one.</p>}
    </>)
  }
  return page(open('Open in browser'), <>
    <p>Enter this code at xAI</p>
    <CopyField kind="code" test="sign-in-code" copyTest="sign-in-copy-code" label="Copy code" copy={copy('code')}>{state.code}</CopyField>
    <CopyField kind="url" test="sign-in-address" copyTest="sign-in-copy-address" label="Copy address" copy={copy('address')}>{state.address}</CopyField>
    {state.expiresIn !== null && <p className="small-window-note">Code lasts {minutes(state.expiresIn)} {minutes(state.expiresIn) === 1 ? 'minute' : 'minutes'}.</p>}
  </>)
}

function Page(): React.JSX.Element | null {
  const [state, setState] = useState<SignInWindowState | null>(null)
  useEffect(() => {
    void window.bravebotSignIn?.read().then((read) => setState((now) => now ?? read))
    return window.bravebotSignIn?.onChanged(setState)
  }, [])
  return state ? <SignInPage state={state} /> : null
}

const root = document.getElementById('root')
if (root) createRoot(root).render(<StrictMode><Page /></StrictMode>)
