import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { useEffect, useRef, useState } from 'react'
import { Alert, Button, Collapse, ControlItem, ProgressRing, SegmentedControl, TextArea } from '../nala'

export function BotMemory({ slug }: { slug: string }): React.JSX.Element {
  const [text, setText] = useState<string | null>(null)
  const [draft, setDraft] = useState('')
  const [editing, setEditing] = useState(false)
  const [mode, setMode] = useState<'readable' | 'raw' | 'history'>('readable')
  const [history, setHistory] = useState<{ at: number; text: string; source: string }[]>([])
  const [problem, setProblem] = useState('')
  const [busy, setBusy] = useState(true)
  const [confirmReset, setConfirmReset] = useState(false)
  const reset = useRef<HTMLElement>(null)
  useEffect(() => {
    if (confirmReset) {
      reset.current?.scrollIntoView({ block: 'nearest' })
      // Leo's button is a shadow host, so its inner <button> is out of reach of querySelector;
      // the host delegates focus to it.
      reset.current?.querySelector<HTMLElement>('leo-button')?.focus()
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
  return <section className="bot-memory-panel" data-test="bot-memory">
    <h3>Persistent memory</h3>
    <p className="bot-note">Saved memory is included when the bot is briefed in a conversation. It is separate from the full message history. Memory saves independently of bot details. Edits are included with your next message to this bot.</p>
    <p className="bot-note">Up to 30 memory revisions are stored locally. Reset keeps revisions for recovery; deleting the bot removes this local history. Project memory files and conversations remain.</p>
    <SegmentedControl className="memory-tabs" size="small" value={mode} data-test="memory-mode"
      onChange={({ value }) => { if (value === 'readable' || value === 'raw' || value === 'history') setMode(value) }}>
      <ControlItem value="readable">Read</ControlItem>
      <ControlItem value="raw">Raw</ControlItem>
      <ControlItem value="history">History</ControlItem>
    </SegmentedControl>
    {problem && <Alert type="error" size="small" role="alert">{problem}</Alert>}
    {busy && <p role="status" className="memory-loading"><ProgressRing mode="indeterminate" /> Loading…</p>}
    {editing ? <>
      <TextArea autofocus aria-label="Edit persistent memory" value={draft} minRows={8} data-test="memory-editor"
        onChange={({ value }) => setDraft(value)} />
      <div className="memory-actions">
        <Button size="small" kind="filled" isDisabled={busy} onClick={() => void save(draft)} data-test="memory-save">Save memory</Button>
        <Button size="small" kind="plain-faint" onClick={() => setEditing(false)}>Cancel edit</Button>
      </div>
    </>
    : mode === 'history' ? <div className="memory-history">{history.length ? [...history].reverse().map((revision, index) => (
      <Collapse key={`${revision.at}-${index}`} isOpen={undefined}
        title={`${new Date(revision.at).toLocaleString()} · ${revision.source === 'user' ? 'Your edit' : 'Bot update'}`}>
        <pre>{revision.text || '(Empty memory)'}</pre>
        <Button size="small" kind="outline" onClick={() => { setDraft(revision.text); setEditing(true) }}>Review for restore</Button>
      </Collapse>
    )) : <p>History begins with memory updates captured by this version.</p>}</div>
    : mode === 'raw' ? <pre className="bot-memory">{text || 'Nothing remembered yet.'}</pre>
    : <div className="memory-readable"><ReactMarkdown remarkPlugins={[remarkGfm]} components={{ a: ({ children }) => <span>{children}</span>, img: ({ alt }) => <span>{alt}</span> }}>{text || 'Nothing remembered yet.'}</ReactMarkdown></div>}
    {!editing && <div className="memory-actions">
      <Button size="small" kind="outline" isDisabled={busy} onClick={() => { setDraft(text ?? ''); setEditing(true) }} data-test="memory-edit">Edit memory</Button>
      <Button size="small" kind="plain-faint" isDisabled={busy || !text} onClick={() => setConfirmReset(true)}>Reset memory…</Button>
    </div>}
    {confirmReset && <Alert type="warning" size="small" className="memory-reset" ref={reset} hasActions>
      <span>Reset this bot’s saved memory? The current version remains in History for restoration. Conversation messages are kept.</span>
      <div slot="actions" className="memory-actions">
        <Button size="small" kind="filled" isDisabled={busy} onClick={() => void save('')}>Reset saved memory</Button>
        <Button size="small" kind="plain-faint" onClick={() => setConfirmReset(false)}>Keep memory</Button>
      </div>
    </Alert>}
  </section>
}
