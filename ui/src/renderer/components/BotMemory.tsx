import { useEffect, useRef, useState } from 'react'
import { Alert, Button, Collapse, ProgressRing, TextArea } from '../nala'

/**
 * The memory a bot keeps in one folder: the text as it is, and the buttons that change it.
 *
 * The text is shown raw rather than rendered, because it is the bot's own file and what is in it
 * is what the next conversation reads. Earlier versions and reset sit beside Edit memory.
 */
export function BotMemory({ slug, directory }: { slug: string; directory: string }): React.JSX.Element {
  const [text, setText] = useState<string | null>(null)
  const [draft, setDraft] = useState('')
  const [editing, setEditing] = useState(false)
  const [showHistory, setShowHistory] = useState(false)
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
    void Promise.all([window.bravebot.readBotMemory(slug, directory), window.bravebot.readMemoryHistory(slug, directory)]).then(([memory, revisions]) => {
      if (!gone) { setText(memory); setHistory(revisions) }
    }).catch(() => { if (!gone) setProblem('Memory could not be loaded.') }).finally(() => { if (!gone) setBusy(false) })
    return () => { gone = true }
  }, [slug, directory])
  const save = async (value: string) => {
    setBusy(true); setProblem('')
    try {
      const saved = await window.bravebot.editBotMemory(slug, directory, value, text)
      document.dispatchEvent(new CustomEvent('bravebot:memory-edited', { detail: slug }))
      setText(saved); setEditing(false); setConfirmReset(false)
      setHistory(await window.bravebot.readMemoryHistory(slug, directory))
    } catch (error) { setProblem(String(error)) }
    finally { setBusy(false) }
  }
  return <section className="bot-memory-panel" data-test="bot-memory" aria-labelledby={`memory-${slug}`}>
    <span className="bot-field-label" id={`memory-${slug}`}>Memory</span>
    {problem && <Alert type="error" size="small" role="alert">{problem}</Alert>}
    {busy && <p role="status" className="memory-loading"><ProgressRing mode="indeterminate" /> Loading…</p>}
    {editing ? <>
      <TextArea autofocus aria-label="Edit memory" value={draft} minRows={8} data-test="memory-editor"
        onInput={({ value }) => setDraft(value)}
        onChange={({ value }) => setDraft(value)} />
      <div className="memory-actions">
        <Button size="small" kind="filled" isDisabled={busy} onClick={() => void save(draft)} data-test="memory-save">Save memory</Button>
        <Button size="small" kind="plain-faint" onClick={() => setEditing(false)}>Cancel edit</Button>
      </div>
    </> : <>
      <pre className="bot-memory" tabIndex={0}>{text || 'Nothing remembered yet.'}</pre>
      <div className="memory-actions">
        <Button size="small" kind="outline" isDisabled={busy} onClick={() => { setDraft(text ?? ''); setEditing(true) }} data-test="memory-edit">Edit memory</Button>
        <Button size="small" kind="plain-faint" aria-expanded={showHistory} onClick={() => setShowHistory(!showHistory)} data-test="memory-history">
          {showHistory ? 'Hide history' : 'History'}
        </Button>
        <Button size="small" kind="plain-faint" isDisabled={busy || !text} onClick={() => setConfirmReset(true)}>Reset…</Button>
      </div>
    </>}
    {showHistory && !editing && <div className="memory-history">{history.length ? [...history].reverse().map((revision, index) => (
      <Collapse key={`${revision.at}-${index}`} className="flat-collapse memory-revision" isOpen={undefined}
        title={`${new Date(revision.at).toLocaleString()} · ${revision.source === 'user' ? 'Your edit' : 'Bot update'}`}>
        <pre>{revision.text || '(Empty memory)'}</pre>
        <Button size="small" kind="outline" onClick={() => { setDraft(revision.text); setEditing(true) }}>Review for restore</Button>
      </Collapse>
    )) : <p className="bot-note">History begins with the next change to this memory. Up to 30 versions are kept.</p>}</div>}
    {confirmReset && <Alert type="warning" size="small" className="memory-reset" ref={reset} hasActions>
      <span>Reset this bot’s saved memory? The current version stays in History. Conversations are kept.</span>
      <div slot="actions" className="memory-actions">
        <Button size="small" kind="filled" isDisabled={busy} onClick={() => void save('')}>Reset saved memory</Button>
        <Button size="small" kind="plain-faint" onClick={() => setConfirmReset(false)}>Keep memory</Button>
      </div>
    </Alert>}
  </section>
}
