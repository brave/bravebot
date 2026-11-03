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
import { Button, Icon } from './nala'
import './nala-setup'
import './styles.css'
import './styles/sign-in.css'

declare global {
  interface Window { bravebotSignIn?: BravebotSignInApi }
}

function Copy({ what, label }: { what: 'url' | 'code' | 'address'; label: string }): React.JSX.Element {
  const [copied, setCopied] = useState(false)
  useEffect(() => {
    if (!copied) return
    const timer = setTimeout(() => setCopied(false), 1500)
    return () => clearTimeout(timer)
  }, [copied])
  return <Button size="small" kind="outline" data-test={`sign-in-copy-${what}`} aria-label={label}
    onClick={() => void window.bravebotSignIn?.copy(what).then(setCopied)}>
    <Icon name={copied ? 'check-normal' : 'copy'} slot="icon-before" />{copied ? 'Copied' : 'Copy'}
  </Button>
}

const minutes = (seconds: number) => Math.max(1, Math.round(seconds / 60))

export function SignInPage({ state }: { state: SignInWindowState }): React.JSX.Element {
  const api = window.bravebotSignIn
  const cancel = <Button kind="plain" onClick={() => void api?.cancel()} data-test="sign-in-cancel">Cancel</Button>
  if (state.way === 'bedrock') {
    return <main className="sign-in" data-test="sign-in-window">
      <h1><Icon name={SIGN_IN_LOGOS.bedrock} aria-hidden="true" data-test="sign-in-logo" />Sign in to AWS</h1>
      <p>The AWS command-line tool opened your browser{state.id === null ? '' : <> for the profile <span data-test="sign-in-profile">{state.id}</span></>}. Check that the page shows this code, then allow access.</p>
      {state.code === null
        ? <p data-test="sign-in-code-pending">Waiting for the code…</p>
        : <div className="sign-in-field"><code className="sign-in-code" data-test="sign-in-code">{state.code}</code><Copy what="code" label="Copy code" /></div>}
      {state.url !== null && <div className="sign-in-field"><code className="sign-in-url" data-test="sign-in-url">{state.url}</code><Copy what="url" label="Copy address" /></div>}
      <div className="sign-in-actions">
        {cancel}
        <Button kind="filled" isDisabled={state.url === null} onClick={() => void api?.open()} data-test="sign-in-open">Open browser again</Button>
      </div>
    </main>
  }
  if (state.way === 'chatgpt') {
    return <main className="sign-in" data-test="sign-in-window">
      <h1><Icon name={SIGN_IN_LOGOS.chatgpt} aria-hidden="true" data-test="sign-in-logo" />Sign in to ChatGPT</h1>
      <p>Finish signing in in your browser. If it did not open, open this address in it.</p>
      <div className="sign-in-field"><code className="sign-in-url" data-test="sign-in-url">{state.url}</code><Copy what="url" label="Copy address" /></div>
      {state.port !== null && <p>The sign-in waits for the browser on port <span className="num">{state.port}</span>. On another computer, forward that port to this one.</p>}
      <div className="sign-in-actions">
        {cancel}
        <Button kind="filled" onClick={() => void api?.open()} data-test="sign-in-open">Open browser again</Button>
      </div>
    </main>
  }
  return <main className="sign-in" data-test="sign-in-window">
    <h1><Icon name={SIGN_IN_LOGOS.xai} aria-hidden="true" data-test="sign-in-logo" />Enter this code at xAI</h1>
    <div className="sign-in-field"><code className="sign-in-code" data-test="sign-in-code">{state.code}</code><Copy what="code" label="Copy code" /></div>
    <div className="sign-in-field"><code className="sign-in-url" data-test="sign-in-address">{state.address}</code><Copy what="address" label="Copy address" /></div>
    {state.expiresIn !== null && <p>Code lasts {minutes(state.expiresIn)} {minutes(state.expiresIn) === 1 ? 'minute' : 'minutes'}.</p>}
    <div className="sign-in-actions">
      {cancel}
      <Button kind="filled" onClick={() => void api?.open()} data-test="sign-in-open">Open in browser</Button>
    </div>
  </main>
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
