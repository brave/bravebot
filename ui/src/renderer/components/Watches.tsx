import { useEffect, useState } from 'react'
import { Modal } from './Modal'
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
  return <Modal title="File watches" onClose={onClose} className="watch-settings">
    <div className="settings-heading"><h2>File watches</h2><button onClick={onClose} aria-label="Close file watches">×</button></div>
    <p>A file change starts a turn in this conversation and may use model credits. Normal read and approval rules still apply.</p>
    <p>Up to eight watches, for seven days each. They run while this conversation is open in the app. Closing it ends the watches.</p>
    {problem && <p role="alert">{problem}</p>}{status && <p role="status">{status}</p>}
    {!listing && !problem && <p role="status">Loading watches…</p>}
    {listing?.watches.length === 0 && <p>No files watched. Add one below, or ask the agent to watch a file.</p>}
    <ul className="watch-list">{listing?.watches.map(w => <li key={w.number}><div><strong>{w.path}</strong><p>{w.state === 'running' ? 'Automatic turn running' : listing.busy ? 'Waiting for this turn to finish' : 'Watching'} · Expires in {Math.max(1, Math.ceil(w.remainingSeconds / 3600))} hours</p><small>{w.armedBy ? `Armed by turn ${w.armedBy}` : 'Added by you'}</small></div><button disabled={busy} onClick={() => void request('watches.stop', { number: w.number })} aria-label={`Stop watching ${w.path}`}>Stop</button></li>)}</ul>
    <form onSubmit={e => { e.preventDefault(); void request('watches.add', { path: path.trim() }) }}><label>Project file<input value={path} onChange={e => setPath(e.target.value)} placeholder="src/example.ts" /></label><button disabled={busy || !path.trim() || !listing || listing.busy || listing.watches.length >= 8}>Watch file</button></form>
    {listing?.busy && <p>Wait for the current turn to finish before adding a watch.</p>}
    <div className="settings-actions"><button disabled={busy || !listing?.watches.length} onClick={() => void request('watches.stop', { all: true })}>Stop all watches</button><button onClick={onClose}>Done</button></div>
  </Modal>
}
