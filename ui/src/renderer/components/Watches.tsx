import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import { Modal } from './Modal'
import { Alert, Button, Input, ProgressRing } from '../nala'

interface Watch { number: number; path: string; remainingSeconds: number; armedBy: number; state: string }
interface Listing { watches: Watch[]; busy: boolean }
export function Watches({ session, onClose }: { session: string; onClose: () => void }): React.JSX.Element {
  const [listing, setListing] = useState<Listing | null>(null)
  const [path, setPath] = useState('')
  const [problem, setProblem] = useState('')
  const [busy, setBusy] = useState(false)
  const [status, setStatus] = useState('')
  const request = async (method = 'watches.list', params: Record<string, unknown> = {}) => {
    if (method !== 'watches.list') { setBusy(true); setProblem('') }
    try {
      const response = await window.bravebot.request<Listing>(method, { session, ...params })
      if (response.error) throw new Error(response.error.message)
      if (response.ok) setListing(response.ok)
      if (method === 'watches.add') { setPath(''); setStatus('Watch added. File changes can now start a model turn.') }
      if (method === 'watches.stop') setStatus('Watch stopped. Any turn already running can be stopped in the conversation.')
    } catch (error) { setProblem(String(error)) } finally { if (method !== 'watches.list') setBusy(false) }
  }
  useEffect(() => { void request(); const timer = setInterval(() => void request(), 2000); return () => clearInterval(timer) }, [session])
  const form = useRef<HTMLFormElement>(null)
  const list = useRef<HTMLUListElement>(null)
  useLayoutEffect(() => {
    const root = list.current
    if (!root) return
    for (const item of root.querySelectorAll('li')) {
      const name = item.querySelector('strong')?.textContent
      const button = item.querySelector('leo-button')?.shadowRoot?.querySelector('button')
      const label = name ? `Stop watching ${name}` : ''
      if (button && label && button.getAttribute('aria-label') !== label) button.setAttribute('aria-label', label)
    }
  })
  const adding = useRef(false)
  // The submit control lives in Leo's shadow root, so it is not a participant
  // in this form. Ask the form to submit from the click instead. The click
  // reaches both the inner control and the host, so ignore the second one.
  const add = () => {
    if (adding.current) return
    adding.current = true
    queueMicrotask(() => { adding.current = false })
    form.current?.requestSubmit()
  }
  return <Modal title="File watches" onClose={onClose} className="watch-settings" actions={<>
    <Button size="small" kind="outline" isDisabled={busy || !listing?.watches.length} onClick={() => void request('watches.stop', { all: true })}>Stop all watches</Button>
    <Button size="small" kind="filled" onClick={onClose} data-test="watches-done">Done</Button>
  </>}>
    <p className="settings-lede">A file change starts a turn in this conversation and may use model credits. Normal read and approval rules still apply.</p>
    <p>Up to eight watches, for seven days each. They run while this conversation is open in the app. Closing it ends the watches.</p>
    {problem && <Alert type="error" size="small" role="alert">{problem}</Alert>}{status && <Alert type="success" size="small" role="status">{status}</Alert>}
    {!listing && !problem && <p role="status" className="settings-busy"><ProgressRing mode="indeterminate" /> Loading watches…</p>}
    {listing?.watches.length === 0 && <p>No files watched. Add one below, or ask the agent to watch a file.</p>}
    <ul className="watch-list" ref={list}>{listing?.watches.map(w => <li key={w.number}><div><strong>{w.path}</strong><p>{w.state === 'running' ? 'Automatic turn running' : listing.busy ? 'Waiting for this turn to finish' : 'Watching'} · Expires in {Math.max(1, Math.ceil(w.remainingSeconds / 3600))} hours</p><small>{w.armedBy ? `Armed by turn ${w.armedBy}` : 'Added by you'}</small></div><Button size="small" kind="plain-faint" isDisabled={busy} onClick={() => void request('watches.stop', { number: w.number })} aria-label={`Stop watching ${w.path}`}>Stop</Button></li>)}</ul>
    <form ref={form} onSubmit={e => { e.preventDefault(); void request('watches.add', { path: path.trim() }) }}>
        <Input value={path} placeholder="src/example.ts" data-test="watch-path"
          onInput={(event) => {
            const detail = event as { value?: unknown; target?: EventTarget | null }
            const value = typeof detail.value === 'string' ? detail.value
              : detail.target && typeof detail.target === 'object' && 'value' in detail.target && typeof detail.target.value === 'string' ? detail.target.value
                : null
            if (value !== null) setPath(value)
          }}>Project file</Input>
      <Button size="small" kind="filled" type="submit" onClick={add} isDisabled={busy || !path.trim() || !listing || listing.busy || listing.watches.length >= 8} data-test="watch-add">Watch file</Button>
    </form>
    {listing?.busy && <p>Wait for the current turn to finish before adding a watch.</p>}
  </Modal>
}
