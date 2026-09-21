import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { useEffect, useRef, useState } from 'react'

export function BotMemory({ slug }: { slug: string }): React.JSX.Element {
  const [text, setText] = useState<string | null>(null)
  const [draft, setDraft] = useState('')
  const [editing, setEditing] = useState(false)
  const [mode, setMode] = useState<'readable' | 'raw' | 'history'>('readable')
  const [history, setHistory] = useState<{ at: number; text: string; source: string }[]>([])
  const [problem, setProblem] = useState('')
  const [busy, setBusy] = useState(true)
  const [confirmReset, setConfirmReset] = useState(false)
  const reset = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (confirmReset) {
      reset.current?.scrollIntoView({ block: 'nearest' })
      reset.current?.querySelector('button')?.focus()
    }
  }, [confirmReset])
  useEffect(() => {
    let gone = false
    void Promise.all([window.bravebot.readBotMemory(slug), window.bravebot.readMemoryHistory(slug)]).then(([memory, revisions]) => {
      if (!gone) { setText(memory); setHistory(revisions) }
    }).catch(() => { if (!gone) setProblem('Memory could not be loaded.') }).finally(() => { if (!gone) setBusy(false) })
    return () => { gone = true }
  }, [slug])
  const save = async (value: string) => {
    setBusy(true); setProblem('')
    try {
      const saved = await window.bravebot.editBotMemory(slug, value, text)
      document.dispatchEvent(new CustomEvent('bravebot:memory-edited', { detail: slug }))
      setText(saved); setEditing(false); setConfirmReset(false)
      setHistory(await window.bravebot.readMemoryHistory(slug))
    } catch (error) { setProblem(String(error)) }
    finally { setBusy(false) }
  }
  return <section className="bot-memory-panel">
    <h3>Persistent memory</h3>
    <p className="bot-note">Saved memory is included when the bot is briefed in a conversation. It is separate from the full message history. Memory saves independently of bot details. Edits are included with your next message to this bot.</p>
    <p className="bot-note">Up to 30 memory revisions are stored locally. Reset keeps revisions for recovery; deleting the bot removes this local history. Project memory files and conversations remain.</p>
    <div className="memory-tabs">{(['readable', 'raw', 'history'] as const).map((value) => <button type="button" key={value} aria-pressed={mode === value} onClick={() => setMode(value)}>{value === 'readable' ? 'Read' : value === 'raw' ? 'Raw' : 'History'}</button>)}</div>
    {problem && <p role="alert">{problem}</p>}
    {busy && <p role="status">Loading…</p>}
    {editing ? <><textarea autoFocus aria-label="Edit persistent memory" rows={8} value={draft} onChange={(event) => setDraft(event.target.value)} />
      <div className="memory-actions"><button type="button" disabled={busy} onClick={() => void save(draft)}>Save memory</button><button type="button" onClick={() => setEditing(false)}>Cancel edit</button></div></>
    : mode === 'history' ? <div className="memory-history">{history.length ? [...history].reverse().map((revision, index) => <details key={`${revision.at}-${index}`}><summary>{new Date(revision.at).toLocaleString()} · {revision.source === 'user' ? 'Your edit' : 'Bot update'}</summary><pre>{revision.text || "(Empty memory)"}</pre><button type="button" onClick={() => { setDraft(revision.text); setEditing(true) }}>Review for restore</button></details>) : <p>History begins with memory updates captured by this version.</p>}</div>
    : mode === 'raw' ? <pre className="bot-memory">{text || 'Nothing remembered yet.'}</pre>
    : <div className="memory-readable"><ReactMarkdown remarkPlugins={[remarkGfm]} components={{ a: ({ children }) => <span>{children}</span>, img: ({ alt }) => <span>{alt}</span> }}>{text || 'Nothing remembered yet.'}</ReactMarkdown></div>}
    {!editing && <div className="memory-actions"><button type="button" disabled={busy} onClick={() => { setDraft(text ?? ''); setEditing(true) }}>Edit memory</button><button type="button" disabled={busy || !text} onClick={() => setConfirmReset(true)}>Reset memory…</button></div>}
    {confirmReset && <div className="memory-reset" ref={reset}><p>Reset this bot’s saved memory? The current version remains in History for restoration. Conversation messages are kept.</p><button type="button" disabled={busy} onClick={() => void save('')}>Reset saved memory</button><button type="button" onClick={() => setConfirmReset(false)}>Keep memory</button></div>}
  </section>
}
