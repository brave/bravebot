/**
 * The page of the main process's key window (PREM-16, CRED-24).
 *
 * It draws what the main process read from the bridge for the row and a field to type the key in:
 * no session, no conversation, no bridge. Save hands the key to the main process, which sends it to
 * the bridge, and nothing here keeps it once the window closes. The words are English, like the
 * main process's own dialogs; it has no catalog.
 */

import { StrictMode, useEffect, useState } from 'react'
import { createRoot } from 'react-dom/client'
import { accountLogo, type KeyWindowState } from '../shared/accounts'
import type { BravebotKeyApi } from '../preload/key-window'
import { Alert, Button, Input } from './nala'
import { SmallWindowPage } from './small-window-page'
import './nala-setup'
import './styles.css'
import './styles/small-window.css'

declare global {
  interface Window { bravebotKey?: BravebotKeyApi }
}

const where = (state: KeyWindowState) => state.way === 'brave' ? "Brave's premium service" : state.host ?? 'the service that issued it'

export function KeyPage({ state }: { state: KeyWindowState }): React.JSX.Element {
  const api = window.bravebotKey
  const [key, setKey] = useState('')
  const [saving, setSaving] = useState(false)
  const [refused, setRefused] = useState<string | null>(null)
  const empty = key.trim() === ''
  const save = async () => {
    if (empty || saving || !api) return
    setSaving(true)
    setRefused(null)
    try {
      const answer = await api.save(key)
      if ('refused' in answer) setRefused(answer.refused)
      else setKey('')
    } finally {
      setSaving(false)
    }
  }
  return <SmallWindowPage logo={accountLogo(state.way, state.id)} logoTest="key-logo" title={`Enter the key for ${state.name}`} test="key-window"
    fit={api?.fit} actions={<>
      <Button kind="plain-faint" onClick={() => void api?.cancel()} data-test="key-cancel">Cancel</Button>
      <Button kind="filled" isDisabled={empty || saving} onClick={() => void save()} data-test="key-save">
        {state.replace ? 'Replace key' : 'Save key'}
      </Button>
    </>}>
    <p data-test="key-where">{state.way === 'brave'
      ? "It is kept on this computer, readable only by your account, and sent only to Brave's premium service."
      : `It is kept on this computer and sent only to ${where(state)}.`}
      {state.records && ` Your settings are changed to name ${state.name}, so sessions send it.`}</p>
    {state.replace && <Alert type="warning" size="small" data-test="key-replace">
      A key is stored already. Saving replaces it, and the one replaced still works at {where(state)} until you revoke it there.
    </Alert>}
    {state.variable && <Alert type="info" size="small" data-test="key-variable">
      <code>{state.variable}</code> is set, and while it is, a session sends its value instead of this key.
    </Alert>}
    <Input className="small-window-input" type="password" autofocus value={key} autocomplete="off" spellcheck={false} data-test="key-input" disabled={saving}
      onInput={({ value }) => setKey(value)} onChange={({ value }) => setKey(value)}
      onKeyDown={({ innerEvent }) => { if ((innerEvent as unknown as KeyboardEvent).key === 'Enter') void save() }}>Key</Input>
    {refused && <Alert type="error" size="small" role="alert" data-test="key-refused">{refused}</Alert>}
  </SmallWindowPage>
}

function Page(): React.JSX.Element | null {
  const [state, setState] = useState<KeyWindowState | null>(null)
  useEffect(() => {
    void window.bravebotKey?.read().then((read) => setState((now) => now ?? read))
    return window.bravebotKey?.onChanged(setState)
  }, [])
  // Another row's window starts empty: a key typed for one row is never saved for another.
  return state ? <KeyPage key={`${state.way}:${state.id ?? ''}`} state={state} /> : null
}

const root = document.getElementById('root')
if (root) createRoot(root).render(<StrictMode><Page /></StrictMode>)
